//! The assistant's MCP server: the database tools, on 127.0.0.1, for the agent the app runs.
//! Every call carries the token of its conversation, which says whose database it acts on.

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http::header::{AUTHORIZATION, HOST, ORIGIN};
use http::{HeaderMap, Request, Response, StatusCode};
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use rmcp::model::{CacheScope, CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::Value;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::conversation::{Conversation, Conversations};
use super::tools::{definitions, Tools};

/// How long the server waits after a connection could not be accepted.
const ACCEPT_RETRY: std::time::Duration = std::time::Duration::from_millis(50);

pub struct McpServer {
    pub port: u16,
    stop: CancellationToken,
}

impl McpServer {
    /// Listens on a free port of 127.0.0.1 until `stop`.
    pub async fn start(conversations: Arc<Conversations>, tools: Arc<Tools>) -> Result<McpServer, String> {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|e| format!("Couldn't start the assistant's tool server: {e}"))?;
        let port = listener.local_addr().map_err(|e| e.to_string())?.port();
        let stop = CancellationToken::new();
        // Each request stands alone: the conversation comes from its token, not from a session
        let config = StreamableHttpServerConfig::default()
            .with_legacy_session_mode(false)
            .with_json_response(true)
            .with_cancellation_token(stop.child_token());
        let handler = Handler { tools };
        let service: StreamableHttpService<Handler, LocalSessionManager> =
            StreamableHttpService::new(move || Ok(handler.clone()), Arc::new(LocalSessionManager::default()), config);
        let cancelled = stop.clone();
        tokio::spawn(async move {
            loop {
                let stream = tokio::select! {
                    _ = cancelled.cancelled() => break,
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => stream,
                        Err(_) => {
                            // Out of file descriptors, say: trying again at once would only spin
                            tokio::time::sleep(ACCEPT_RETRY).await;
                            continue;
                        }
                    },
                };
                let (service, conversations) = (service.clone(), Arc::clone(&conversations));
                tokio::spawn(async move {
                    let answer = service_fn(move |request: Request<Incoming>| {
                        let (service, conversations) = (service.clone(), Arc::clone(&conversations));
                        async move { Ok::<_, Infallible>(answer(&service, &conversations, port, request).await) }
                    });
                    let _ = hyper::server::conn::http1::Builder::new().serve_connection(TokioIo::new(stream), answer).await;
                });
            }
        });
        Ok(McpServer { port, stop })
    }

    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/mcp", self.port)
    }

    pub async fn stop(self) {
        self.stop.cancel();
    }
}

type Body = BoxBody<Bytes, Infallible>;

fn empty(status: StatusCode) -> Response<Body> {
    let mut response = Response::new(Full::new(Bytes::new()).boxed());
    *response.status_mut() = status;
    response
}

async fn answer(
    service: &StreamableHttpService<Handler, LocalSessionManager>,
    conversations: &Conversations,
    port: u16,
    mut request: Request<Incoming>,
) -> Response<Body> {
    if request.uri().path() != "/mcp" {
        return empty(StatusCode::NOT_FOUND);
    }
    match authorize(request.headers(), port, conversations) {
        Ok(conversation) => {
            request.extensions_mut().insert(conversation);
            service.handle(request).await
        }
        Err(status) => empty(status),
    }
}

/// Who a request is from: refuses a `Host` or `Origin` that isn't this server's own (a web page
/// reaching it through DNS rebinding) with 403, and a missing or unknown token with 401.
pub(crate) fn authorize(headers: &HeaderMap, port: u16, conversations: &Conversations) -> Result<Arc<Conversation>, StatusCode> {
    let is_own = |value: &[u8], allowed: [String; 2]| allowed.iter().any(|own| value.eq_ignore_ascii_case(own.as_bytes()));
    if let Some(host) = headers.get(HOST) {
        if !is_own(host.as_bytes(), [format!("127.0.0.1:{port}"), format!("localhost:{port}")]) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    if let Some(origin) = headers.get(ORIGIN) {
        if !is_own(origin.as_bytes(), [format!("http://127.0.0.1:{port}"), format!("http://localhost:{port}")]) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    let token = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok()).and_then(|v| v.strip_prefix("Bearer ")).unwrap_or("");
    conversations.by_token(token).ok_or(StatusCode::UNAUTHORIZED)
}

#[derive(Clone)]
struct Handler {
    tools: Arc<Tools>,
}

impl ServerHandler for Handler {
    #[allow(deprecated)]
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    async fn list_tools(&self, _request: Option<PaginatedRequestParams>, _context: RequestContext<RoleServer>) -> Result<ListToolsResult, McpError> {
        let tools = definitions()
            .into_iter()
            .map(|d| Tool::new(d.name, d.description, Arc::new(d.input_schema.as_object().cloned().unwrap_or_default())))
            .collect();
        // The newest protocol wants every list to say how long it may be kept; 0 asks again each time
        Ok(ListToolsResult::with_all_items(tools).with_ttl_ms(0).with_cache_scope(CacheScope::Private))
    }

    async fn call_tool(&self, request: CallToolRequestParams, context: RequestContext<RoleServer>) -> Result<CallToolResponse, McpError> {
        let conversation = context
            .extensions
            .get::<http::request::Parts>()
            .and_then(|parts| parts.extensions.get::<Arc<Conversation>>())
            .cloned()
            .ok_or_else(|| McpError::internal_error("The request has no conversation", None))?;
        let output = self.tools.call(&conversation, &request.name, request.arguments.map_or(Value::Null, Value::Object)).await;
        let content = vec![ContentBlock::text(output.text)];
        Ok(if output.is_error { CallToolResult::error(content) } else { CallToolResult::success(content) }.into())
    }
}

#[cfg(test)]
mod tests {
    use http::header::{AUTHORIZATION, HOST, ORIGIN};
    use serde_json::{json, Value};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::sync::Mutex;

    use super::*;
    use crate::assistant::approvals::Approvals;
    use crate::assistant::conversation::ConversationOptions;
    use crate::assistant::events::{EventSink, MemorySink};
    use crate::assistant::fake::FakeDb;
    use crate::db::Driver;

    fn options() -> ConversationOptions {
        ConversationOptions { ask_before_queries: false, schema_only: false, row_limit: 100, instructions: String::new() }
    }

    fn headers(host: Option<&str>, origin: Option<&str>, token: Option<&str>) -> HeaderMap {
        let mut map = HeaderMap::new();
        if let Some(host) = host {
            map.insert(HOST, host.parse().unwrap());
        }
        if let Some(origin) = origin {
            map.insert(ORIGIN, origin.parse().unwrap());
        }
        if let Some(token) = token {
            map.insert(AUTHORIZATION, format!("Bearer {token}").parse().unwrap());
        }
        map
    }

    #[test]
    fn authorize_in_table() {
        let all = Conversations::default();
        let live = all.create("s1", options());
        let gone = all.create("s1", options());
        all.remove(&gone.id);
        let port = 4000;
        let own = format!("127.0.0.1:{port}");
        let ok = |h: HeaderMap| authorize(&h, port, &all).map(|c| c.id.clone());
        assert_eq!(ok(headers(None, None, None)), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ok(headers(Some(&own), None, Some(&"0".repeat(64)))), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ok(headers(Some(&own), None, Some(&gone.token))), Err(StatusCode::UNAUTHORIZED));
        assert_eq!(ok(headers(Some("evil.com:4000"), None, Some(&live.token))), Err(StatusCode::FORBIDDEN));
        assert_eq!(ok(headers(Some(&own), Some("https://evil.com"), Some(&live.token))), Err(StatusCode::FORBIDDEN));
        assert_eq!(ok(headers(Some(&own), Some("http://127.0.0.1:4000"), Some(&live.token))), Ok(live.id.clone()));
        assert_eq!(ok(headers(Some("localhost:4000"), Some("http://localhost:4000"), Some(&live.token))), Ok(live.id.clone()));
        assert_eq!(ok(headers(Some(&own), None, Some(&live.token))), Ok(live.id.clone()));
    }

    struct Running {
        server: McpServer,
        conversations: Arc<Conversations>,
        db: Arc<FakeDb>,
    }

    async fn running() -> Running {
        let db = FakeDb::new();
        let conversations = Arc::new(Conversations::default());
        let sessions: Arc<Mutex<std::collections::HashMap<String, Arc<dyn Driver>>>> = Default::default();
        sessions.lock().await.insert("s1".into(), db.clone());
        let events: Arc<dyn EventSink> = MemorySink::new();
        let tools = Arc::new(Tools { sessions, approvals: Arc::new(Approvals::default()), events });
        let server = McpServer::start(Arc::clone(&conversations), tools).await.unwrap();
        Running { server, conversations, db }
    }

    /// One POST over a plain socket: the status and the JSON of the answer.
    async fn post(port: u16, token: Option<&str>, body: &Value) -> (u16, Value) {
        post_as(port, token, body, "2025-06-18").await
    }

    async fn post_as(port: u16, token: Option<&str>, body: &Value, protocol: &str) -> (u16, Value) {
        let method = body["method"].as_str().unwrap_or("").to_string();
        let body = body.to_string();
        let auth = token.map(|t| format!("Authorization: Bearer {t}\r\n")).unwrap_or_default();
        let request = format!(
            "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Type: application/json\r\nAccept: application/json, text/event-stream\r\nMCP-Protocol-Version: {protocol}\r\nMcp-Method: {method}\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8_lossy(&raw).into_owned();
        let status = text.split_whitespace().nth(1).unwrap().parse().unwrap();
        let payload = text.split("\r\n\r\n").nth(1).unwrap_or("");
        // Whatever the framing (chunks, an event stream), the JSON is the line that starts with a brace
        let json = payload.lines().map(|l| l.trim().trim_start_matches("data:").trim()).find(|l| l.starts_with('{')).unwrap_or("null");
        (status, serde_json::from_str(json).unwrap_or(Value::Null))
    }

    fn initialize() -> Value {
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}})
    }

    #[tokio::test]
    async fn server_lists_the_five_tools() {
        let r = running().await;
        let token = r.conversations.create("s1", options()).token.clone();
        let (status, answer) = post(r.server.port, Some(&token), &initialize()).await;
        assert_eq!(status, 200, "{answer}");
        let (status, answer) = post(r.server.port, Some(&token), &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).await;
        assert_eq!(status, 200, "{answer}");
        let names: Vec<&str> = answer["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["list_tables", "describe_table", "relationships", "run_query", "explain_query"]);
        r.server.stop().await;
    }

    /// Claude Code speaks the 2026-07-28 protocol, whose lists must say how long they may be kept;
    /// without it the client throws the tools away.
    #[tokio::test]
    async fn tools_list_carries_the_cache_hints_of_the_newest_protocol() {
        let r = running().await;
        let token = r.conversations.create("s1", options()).token.clone();
        let list = json!({"jsonrpc": "2.0", "id": 0, "method": "tools/list", "params": {"_meta": {
            "io.modelcontextprotocol/protocolVersion": "2026-07-28",
            "io.modelcontextprotocol/clientInfo": {"name": "claude-code", "version": "2"},
            "io.modelcontextprotocol/clientCapabilities": {},
        }}});
        let (status, answer) = post_as(r.server.port, Some(&token), &list, "2026-07-28").await;
        assert_eq!(status, 200, "{answer}");
        assert_eq!(answer["result"]["tools"].as_array().unwrap().len(), 5, "{answer}");
        assert!(answer["result"]["ttlMs"].is_number(), "{answer}");
        assert!(matches!(answer["result"]["cacheScope"].as_str(), Some("public" | "private")), "{answer}");
        r.server.stop().await;
    }

    #[tokio::test]
    async fn server_calls_a_tool_on_the_token_conversation() {
        let r = running().await;
        let token = r.conversations.create("s1", options()).token.clone();
        let call = json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "run_query", "arguments": {"sql": "SELECT 1"}}});
        let (status, answer) = post(r.server.port, Some(&token), &call).await;
        assert_eq!(status, 200, "{answer}");
        let text = answer["result"]["content"][0]["text"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(text).unwrap()["rowCount"], 2);
        assert_eq!(r.db.reads(), vec!["SELECT 1"]);
        // A refusal comes back as a tool error, not a protocol error
        let call = json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "run_query", "arguments": {"sql": "DELETE FROM t"}}});
        let (_, answer) = post(r.server.port, Some(&token), &call).await;
        assert_eq!(answer["result"]["isError"], true);
        r.server.stop().await;
    }

    #[tokio::test]
    async fn server_refuses_requests_without_a_valid_token() {
        let r = running().await;
        let list = json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"});
        assert_eq!(post(r.server.port, None, &list).await.0, 401);
        assert_eq!(post(r.server.port, Some(&"0".repeat(64)), &list).await.0, 401);
        let token = r.conversations.create("s1", options()).token.clone();
        r.conversations.remove_for_session("s1");
        assert_eq!(post(r.server.port, Some(&token), &list).await.0, 401);
        r.server.stop().await;
    }

    #[tokio::test]
    async fn server_binds_loopback_only() {
        let r = running().await;
        assert!(TcpStream::connect(("127.0.0.1", r.server.port)).await.is_ok());
        assert_eq!(r.server.url(), format!("http://127.0.0.1:{}/mcp", r.server.port));
        r.server.stop().await;
    }
}
