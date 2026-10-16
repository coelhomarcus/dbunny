//! SSH tunnels: the database is reached through an SSH server ("bastion"). A local
//! port forwards each connection over the SSH session to the database host.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use russh::client;
use russh::keys::{self, HashAlg, PrivateKeyWithHashAlg, PublicKey};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

/// How to reach and log in to the SSH server.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SshConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    /// "password", "key" or "agent".
    pub auth_method: String,
    #[serde(default)]
    pub password: String,
    /// Private key file, for "key".
    #[serde(default)]
    pub key_path: String,
    /// Passphrase of an encrypted private key.
    #[serde(default)]
    pub passphrase: String,
}

/// Why the server's host key wasn't accepted. Sent to the UI as JSON so it can ask the user.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyProblem {
    /// "unknown": never seen, the user can choose to trust it.
    /// "changed": differs from the recorded key, which may mean an attack.
    pub status: String,
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub algorithm: String,
    /// The mismatching record is in ~/.ssh/known_hosts (fixed there, not in DBunny).
    pub in_user_known_hosts: bool,
}

/// Host keys the user accepted in DBunny, by "host:port". Stored as JSON in the app data folder.
pub struct KnownHosts {
    path: PathBuf,
}

impl KnownHosts {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn load(&self) -> HashMap<String, String> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    fn get(&self, host: &str, port: u16) -> Option<String> {
        self.load().remove(&format!("{host}:{port}"))
    }

    fn remember(&self, host: &str, port: u16, fingerprint: &str) -> std::io::Result<()> {
        let mut all = self.load();
        all.insert(format!("{host}:{port}"), fingerprint.to_string());
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&self.path, serde_json::to_string_pretty(&all).unwrap_or_default())
    }
}

fn user_known_hosts() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(|home| Path::new(&home).join(".ssh").join("known_hosts"))
}

struct Verifier {
    host: String,
    port: u16,
    known: KnownHosts,
    /// Fingerprint the user confirmed for this connection attempt.
    trusted: Option<String>,
    /// Filled when the key is rejected, to explain why after the handshake fails.
    problem: Arc<Mutex<Option<HostKeyProblem>>>,
}

impl Verifier {
    fn check(&self, key: &PublicKey) -> Result<(), HostKeyProblem> {
        let fingerprint = key.fingerprint(HashAlg::Sha256).to_string();
        let problem = |status: &str, in_user_known_hosts: bool| HostKeyProblem {
            status: status.into(),
            host: self.host.clone(),
            port: self.port,
            fingerprint: fingerprint.clone(),
            algorithm: key.algorithm().to_string(),
            in_user_known_hosts,
        };

        // The user's own known_hosts (what `ssh` uses) comes first; DBunny never writes to it
        if let Some(path) = user_known_hosts().filter(|p| p.exists()) {
            match keys::check_known_hosts_path(&self.host, self.port, key, &path) {
                Ok(true) => return Ok(()),
                Err(keys::Error::KeyChanged { .. }) => return Err(problem("changed", true)),
                _ => {}
            }
        }

        let trusted_now = self.trusted.as_deref() == Some(fingerprint.as_str());
        match self.known.get(&self.host, self.port) {
            Some(recorded) if recorded == fingerprint => Ok(()),
            Some(_) if !trusted_now => Err(problem("changed", false)),
            None if !trusted_now => Err(problem("unknown", false)),
            _ => {
                // Confirmed by the user: remember it (a failure to save only means asking again)
                let _ = self.known.remember(&self.host, self.port, &fingerprint);
                Ok(())
            }
        }
    }
}

struct Handler {
    verifier: Verifier,
}

impl client::Handler for Handler {
    type Error = russh::Error;

    async fn check_server_key(&mut self, server_key: &keys::PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let key = match server_key {
            keys::PublicKeyOrCertificate::PublicKey { key, .. } => key.clone(),
            keys::PublicKeyOrCertificate::Certificate(cert) => cert.public_key().clone().into(),
        };
        match self.verifier.check(&key) {
            Ok(()) => Ok(true),
            Err(problem) => {
                *self.verifier.problem.lock().unwrap() = Some(problem);
                Ok(false)
            }
        }
    }
}

/// A running tunnel. Dropping it closes the local port and the SSH session.
pub struct Tunnel {
    pub local_port: u16,
    accept_loop: JoinHandle<()>,
    _session: Arc<client::Handle<Handler>>,
}

impl Tunnel {
    /// The SSH session is still up and the local port still forwards.
    pub fn is_alive(&self) -> bool {
        !self.accept_loop.is_finished() && !self._session.is_closed()
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        self.accept_loop.abort();
    }
}

/// Errors as text for the UI; a rejected host key is JSON (`{"sshHostKey": ...}`) so the
/// UI can show the fingerprint and offer to trust it.
fn host_key_error(problem: &HostKeyProblem) -> String {
    serde_json::json!({ "sshHostKey": problem }).to_string()
}

async fn authenticate(session: &mut client::Handle<Handler>, ssh: &SshConfig) -> Result<bool, String> {
    match ssh.auth_method.as_str() {
        "password" => session
            .authenticate_password(&ssh.user, &ssh.password)
            .await
            .map(|r| r.success())
            .map_err(|e| e.to_string()),
        "key" => {
            let path = expand_home(&ssh.key_path);
            let passphrase = (!ssh.passphrase.is_empty()).then_some(ssh.passphrase.as_str());
            let key = keys::load_secret_key(&path, passphrase).map_err(|e| match e {
                keys::Error::KeyIsEncrypted => "The private key is encrypted: enter its passphrase".to_string(),
                keys::Error::IO(io) => format!("Couldn't read the private key {}: {io}", path.display()),
                other => format!("Couldn't use the private key: {other}"),
            })?;
            let hash = session.best_supported_rsa_hash().await.map_err(|e| e.to_string())?.flatten();
            session
                .authenticate_publickey(&ssh.user, PrivateKeyWithHashAlg::new(Arc::new(key), hash))
                .await
                .map(|r| r.success())
                .map_err(|e| e.to_string())
        }
        "agent" => authenticate_with_agent(session, &ssh.user).await,
        other => Err(format!("Unknown SSH login method \"{other}\"")),
    }
}

#[cfg(unix)]
async fn agent() -> Result<keys::agent::client::AgentClient<tokio::net::UnixStream>, String> {
    keys::agent::client::AgentClient::connect_env()
        .await
        .map_err(|_| "No SSH agent is running (SSH_AUTH_SOCK isn't set)".to_string())
}

#[cfg(windows)]
async fn agent() -> Result<keys::agent::client::AgentClient<tokio::net::windows::named_pipe::NamedPipeClient>, String> {
    keys::agent::client::AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
        .await
        .map_err(|_| "The OpenSSH agent service isn't running".to_string())
}

/// Tries every key the agent holds until the server accepts one.
async fn authenticate_with_agent(session: &mut client::Handle<Handler>, user: &str) -> Result<bool, String> {
    let mut agent = agent().await?;
    let identities = agent.request_identities().await.map_err(|e| format!("SSH agent: {e}"))?;
    if identities.is_empty() {
        return Err("The SSH agent has no keys loaded (add one with ssh-add)".into());
    }
    let hash = session.best_supported_rsa_hash().await.map_err(|e| e.to_string())?.flatten();
    for identity in identities {
        let keys::agent::AgentIdentity::PublicKey { key, .. } = identity else { continue };
        let result = session
            .authenticate_publickey_with(user, key, hash, &mut agent)
            .await
            .map_err(|e| format!("SSH agent: {e}"))?;
        if result.success() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn expand_home(path: &str) -> PathBuf {
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        Some(rest) => user_known_hosts()
            .and_then(|p| p.parent().and_then(Path::parent).map(Path::to_path_buf))
            .map(|home| home.join(rest))
            .unwrap_or_else(|| PathBuf::from(path)),
        None => PathBuf::from(path),
    }
}

/// Logs in to the SSH server and opens a local port forwarding to `target`.
/// `trusted_fingerprint` is a host key the user just confirmed.
pub async fn open_tunnel(
    ssh: &SshConfig,
    target: (String, u16),
    known_hosts: KnownHosts,
    trusted_fingerprint: Option<String>,
) -> Result<Tunnel, String> {
    if ssh.host.trim().is_empty() || ssh.user.trim().is_empty() {
        return Err("SSH host and user are required".into());
    }
    let port = if ssh.port == 0 { 22 } else { ssh.port };
    let problem = Arc::new(Mutex::new(None));
    let handler = Handler {
        verifier: Verifier {
            host: ssh.host.clone(),
            port,
            known: known_hosts,
            trusted: trusted_fingerprint,
            problem: Arc::clone(&problem),
        },
    };
    let config = Arc::new(client::Config {
        nodelay: true,
        inactivity_timeout: None,
        // Keeps idle tunnels (and NAT entries on the way) alive
        keepalive_interval: Some(std::time::Duration::from_secs(30)),
        ..Default::default()
    });

    let connecting = client::connect(config, (ssh.host.as_str(), port), handler);
    let mut session = match tokio::time::timeout(std::time::Duration::from_secs(15), connecting).await {
        Err(_) => return Err(format!("Timed out reaching the SSH server {}:{port}", ssh.host)),
        Ok(Err(e)) => {
            if let Some(problem) = problem.lock().unwrap().take() {
                return Err(host_key_error(&problem));
            }
            return Err(format!("Couldn't reach the SSH server {}:{port}: {e}", ssh.host));
        }
        Ok(Ok(session)) => session,
    };

    if !authenticate(&mut session, ssh).await? {
        return Err(format!(
            "SSH login refused for {}@{}: check the {}",
            ssh.user,
            ssh.host,
            match ssh.auth_method.as_str() {
                "password" => "password",
                "key" => "private key",
                _ => "keys in your SSH agent",
            }
        ));
    }

    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|e| format!("Couldn't open a local port: {e}"))?;
    let local: SocketAddr = listener.local_addr().map_err(|e| e.to_string())?;
    let session = Arc::new(session);

    let forwarder = Arc::clone(&session);
    let accept_loop = tokio::spawn(async move {
        while let Ok((mut socket, origin)) = listener.accept().await {
            let session = Arc::clone(&forwarder);
            let (host, port) = target.clone();
            tokio::spawn(async move {
                let channel = session
                    .channel_open_direct_tcpip(host, port as u32, origin.ip().to_string(), origin.port() as u32)
                    .await;
                match channel {
                    Ok(channel) => {
                        let mut stream = channel.into_stream();
                        let _ = tokio::io::copy_bidirectional(&mut socket, &mut stream).await;
                    }
                    Err(e) => eprintln!("SSH tunnel: couldn't open a channel: {e}"),
                }
            });
        }
    });

    Ok(Tunnel { local_port: local.port(), accept_loop, _session: session })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (KnownHosts, PathBuf) {
        let path = std::env::temp_dir().join(format!("dbunny-known-hosts-{}.json", uuid::Uuid::new_v4()));
        (KnownHosts::new(path.clone()), path)
    }

    fn key(seed: u8) -> PublicKey {
        let private = keys::PrivateKey::new(
            keys::ssh_key::private::KeypairData::Ed25519(keys::ssh_key::private::Ed25519Keypair::from_seed(&[seed; 32])),
            "",
        )
        .unwrap();
        private.public_key().clone()
    }

    fn verifier(known: KnownHosts, trusted: Option<String>) -> Verifier {
        // A host nobody has in ~/.ssh/known_hosts
        Verifier { host: "dbunny-test.invalid".into(), port: 2222, known, trusted, problem: Default::default() }
    }

    #[test]
    fn unknown_keys_are_refused_until_trusted_then_remembered() {
        let (known, path) = store();
        let k = key(1);
        let fingerprint = k.fingerprint(HashAlg::Sha256).to_string();

        let problem = verifier(KnownHosts::new(path.clone()), None).check(&k).unwrap_err();
        assert_eq!((problem.status.as_str(), problem.fingerprint.as_str()), ("unknown", fingerprint.as_str()));
        assert!(fingerprint.starts_with("SHA256:"));

        // Trusting a different fingerprint than the server's doesn't let it in
        assert!(verifier(KnownHosts::new(path.clone()), Some("SHA256:other".into())).check(&k).is_err());

        verifier(known, Some(fingerprint)).check(&k).unwrap();
        // Remembered: accepted without asking
        verifier(KnownHosts::new(path.clone()), None).check(&k).unwrap();
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_changed_key_is_refused_unless_the_new_one_is_confirmed() {
        let (known, path) = store();
        let old = key(1);
        verifier(known, Some(old.fingerprint(HashAlg::Sha256).to_string())).check(&old).unwrap();

        let new = key(2);
        let problem = verifier(KnownHosts::new(path.clone()), None).check(&new).unwrap_err();
        assert_eq!(problem.status, "changed");
        assert!(!problem.in_user_known_hosts);

        let new_fingerprint = new.fingerprint(HashAlg::Sha256).to_string();
        verifier(KnownHosts::new(path.clone()), Some(new_fingerprint)).check(&new).unwrap();
        // The old key is now the stranger
        assert_eq!(verifier(KnownHosts::new(path.clone()), None).check(&old).unwrap_err().status, "changed");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn home_relative_key_paths_are_expanded() {
        let expanded = expand_home("~/.ssh/id_ed25519");
        assert!(expanded.ends_with(".ssh/id_ed25519") || expanded.ends_with(r".ssh\id_ed25519"));
        assert!(!expanded.starts_with("~"));
        assert_eq!(expand_home("/etc/key"), PathBuf::from("/etc/key"));
    }
}

/// Tests against a real SSH server that can reach the test database. Run with:
///   DBUNNY_TEST_SSH=user:password@host:port DBUNNY_TEST_SSH_KEYS=<dir with id_plain, id_locked>
///   DBUNNY_TEST_SSH_TARGET=<db host:port as seen from the SSH server>
///   DBUNNY_TEST_DATABASE_URL=... cargo test --lib ssh -- --ignored
/// `id_locked` must be encrypted with the passphrase "testphrase".
#[cfg(test)]
mod server_tests {
    use super::*;

    struct Env {
        ssh: SshConfig,
        keys: PathBuf,
        target: (String, u16),
        db_url: url::Url,
    }

    fn env() -> Env {
        let var = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run the SSH tests"));
        let ssh = url::Url::parse(&format!("ssh://{}", var("DBUNNY_TEST_SSH"))).unwrap();
        let target = var("DBUNNY_TEST_SSH_TARGET");
        let (target_host, target_port) = target.rsplit_once(':').unwrap();
        Env {
            ssh: SshConfig {
                host: ssh.host_str().unwrap().into(),
                port: ssh.port().unwrap_or(22),
                user: ssh.username().into(),
                auth_method: "password".into(),
                password: ssh.password().unwrap_or("").into(),
                ..Default::default()
            },
            keys: PathBuf::from(var("DBUNNY_TEST_SSH_KEYS")),
            target: (target_host.into(), target_port.parse().unwrap()),
            db_url: url::Url::parse(&var("DBUNNY_TEST_DATABASE_URL")).unwrap(),
        }
    }

    fn fresh_known_hosts() -> (KnownHosts, PathBuf) {
        let path = std::env::temp_dir().join(format!("dbunny-ssh-test-{}.json", uuid::Uuid::new_v4()));
        (KnownHosts::new(path.clone()), path)
    }

    /// Opens the tunnel, accepting the host key the way the UI does: the first attempt is
    /// refused with the fingerprint, the second one passes it as confirmed.
    async fn tunnel(ssh: &SshConfig, target: (String, u16)) -> Result<Tunnel, String> {
        let (known, path) = fresh_known_hosts();
        let first = open_tunnel(ssh, target.clone(), known, None).await;
        let err = match first {
            Ok(_) => panic!("an unknown host key must not be accepted silently"),
            Err(e) => e,
        };
        let problem: serde_json::Value = serde_json::from_str(&err).unwrap_or_else(|_| panic!("not a host key error: {err}"));
        assert_eq!(problem["sshHostKey"]["status"], "unknown");
        let fingerprint = problem["sshHostKey"]["fingerprint"].as_str().unwrap().to_string();
        open_tunnel(ssh, target, KnownHosts::new(path), Some(fingerprint)).await
    }

    async fn query_through(tunnel: &Tunnel, db_url: &url::Url) -> i32 {
        let mut url = db_url.clone();
        url.set_host(Some("127.0.0.1")).unwrap();
        url.set_port(Some(tunnel.local_port)).unwrap();
        let (client, connection) = tokio_postgres::connect(url.as_str(), tokio_postgres::NoTls).await.unwrap();
        tokio::spawn(connection);
        client.query_one("SELECT 41 + 1", &[]).await.unwrap().get(0)
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SSH"]
    async fn password_login_forwards_to_the_database() {
        let env = env();
        let tunnel = tunnel(&env.ssh, env.target.clone()).await.unwrap();
        assert_eq!(query_through(&tunnel, &env.db_url).await, 42);
        // Several connections share the one SSH session
        assert_eq!(query_through(&tunnel, &env.db_url).await, 42);

        // The app's own path: the URL keeps the database host, hostaddr points at the tunnel
        let url = crate::commands::connect::through_tunnel(&env.db_url, tunnel.local_port).unwrap();
        let (config, tls) = crate::commands::connect::connection_config(&url).unwrap();
        let link = crate::commands::state::Link { config, tls };
        let client = crate::commands::connect::open_connection(&link, &Default::default()).await.unwrap();
        let value: i32 = client.query_one("SELECT 7", &[]).await.unwrap().get(0);
        assert_eq!(value, 7);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SSH"]
    async fn private_keys_work_with_and_without_a_passphrase() {
        let env = env();
        let plain = SshConfig { auth_method: "key".into(), key_path: env.keys.join("id_plain").display().to_string(), password: String::new(), ..env.ssh.clone() };
        let tunnel_plain = tunnel(&plain, env.target.clone()).await.unwrap();
        assert_eq!(query_through(&tunnel_plain, &env.db_url).await, 42);

        let locked = SshConfig { key_path: env.keys.join("id_locked").display().to_string(), ..plain.clone() };
        let err = tunnel(&locked, env.target.clone()).await.err().unwrap();
        assert!(err.contains("passphrase"), "{err}");
        let unlocked = SshConfig { passphrase: "testphrase".into(), ..locked };
        let tunnel_locked = tunnel(&unlocked, env.target.clone()).await.unwrap();
        assert_eq!(query_through(&tunnel_locked, &env.db_url).await, 42);
    }

    #[tokio::test]
    #[ignore = "needs DBUNNY_TEST_SSH"]
    async fn wrong_passwords_and_unreachable_servers_explain_themselves() {
        let env = env();
        let wrong = SshConfig { password: "nope".into(), ..env.ssh.clone() };
        let err = tunnel(&wrong, env.target.clone()).await.err().unwrap();
        assert!(err.contains("SSH login refused"), "{err}");

        let (known, _) = fresh_known_hosts();
        let nowhere = SshConfig { port: 1, ..env.ssh.clone() };
        let err = open_tunnel(&nowhere, env.target.clone(), known, None).await.err().unwrap();
        assert!(err.contains("Couldn't reach the SSH server"), "{err}");
    }
}
