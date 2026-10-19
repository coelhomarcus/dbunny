//! MySQL errors as the app shows them.

use std::io::ErrorKind;

use mysql_async::{DriverError, Error, IoError};

use super::connect::MysqlEndpoint;
use crate::db::sqlite::errors::CANCELLED;

/// A save waited for rows another transaction holds (error 1205).
pub const LOCKED: &str = "These rows are locked by another transaction (maybe the editor's): commit or roll it back.";
/// A write on a read-only session (error 1792).
pub const READ_ONLY: &str = "This connection is read-only.";
/// The server or the network dropped the connection.
pub const LOST: &str = "The connection to the server was lost.";

/// The message for `e`, met while talking to `endpoint`. The errors the app knows have
/// their own message; any other server error is "{server's message} (MySQL error {code})".
pub fn error_message(e: &Error, endpoint: &MysqlEndpoint) -> String {
    match e {
        Error::Server(server) => server_message(server.code, &server.message, endpoint),
        Error::Io(IoError::Io(io)) => io_message(io, endpoint),
        Error::Io(IoError::Tls(tls)) => tls_message(&tls.to_string(), endpoint)
            .unwrap_or_else(|| format!("TLS error with {}: {tls}", place(endpoint))),
        Error::Driver(DriverError::ConnectionClosed) => LOST.into(),
        other => other.to_string(),
    }
}

/// Server shutdown (1053), gone away (2006), lost (2013), killed (1927, MariaDB) and closed
/// for being idle too long (4031, MySQL 8.0.24+).
const LOST_CODES: [u16; 5] = [1053, 1927, 2006, 2013, 4031];
/// How a connection that was up and then dropped fails on the socket.
const LOST_KINDS: [ErrorKind; 5] =
    [ErrorKind::BrokenPipe, ErrorKind::ConnectionReset, ErrorKind::ConnectionAborted, ErrorKind::UnexpectedEof, ErrorKind::NotConnected];

fn server_message(code: u16, message: &str, endpoint: &MysqlEndpoint) -> String {
    match code {
        1045 => format!("Access denied for {}@{}: check the user and password.", endpoint.user, endpoint.host),
        1049 => {
            let name = quoted(message).or(endpoint.database.as_deref()).unwrap_or_default();
            format!("The database {name} doesn't exist.")
        }
        1205 => LOCKED.into(),
        1317 => CANCELLED.into(),
        1792 => READ_ONLY.into(),
        2003 => format!("Couldn't reach {}: {message}", place(endpoint)),
        // The console turns its "at line N" into a position in the script
        1064 => message.to_string(),
        code if LOST_CODES.contains(&code) => LOST.into(),
        code => format!("{message} ({} error {code})", endpoint.engine_label()),
    }
}

/// What rustls says when the certificate is fine but doesn't name the host
/// ("invalid peer certificate: certificate not valid for name ...").
const NAME_MISMATCH: &str = "certificate not valid for name";

/// The message for a TLS failure of the certificate's name, or None for any other failure.
///
/// With `verify-ca` the name should not matter, but `mysql_async` 0.37.1 only skips the name
/// error when rustls' text contains "NotValidForName", and current rustls reports
/// `NotValidForNameContext` with a readable text instead. Until that is fixed `verify-ca`
/// refuses what `verify-full` refuses, and says why.
fn tls_message(text: &str, endpoint: &MysqlEndpoint) -> Option<String> {
    if !text.contains(NAME_MISMATCH) {
        return None;
    }
    let host = &endpoint.host;
    Some(match endpoint.tls.as_ref().map(|tls| tls.mode.as_str()) {
        Some("verify-ca") => format!(
            "The server's certificate doesn't name {host}, and verifying only the CA isn't possible yet. \
             Use Required, or issue a certificate that names the host."
        ),
        _ => format!("The server's certificate doesn't name {host}."),
    })
}

fn io_message(io: &std::io::Error, endpoint: &MysqlEndpoint) -> String {
    if let Some(message) = tls_message(&io.to_string(), endpoint) {
        return message;
    }
    if LOST_KINDS.contains(&io.kind()) {
        LOST.into()
    } else {
        // Refused, timed out, unknown host, missing socket file...
        format!("Couldn't reach {}: {io}", place(endpoint))
    }
}

/// "host:port", or the socket's path.
fn place(endpoint: &MysqlEndpoint) -> String {
    if endpoint.host.starts_with('/') {
        endpoint.host.clone()
    } else {
        format!("{}:{}", endpoint.host, endpoint.port_or_default())
    }
}

/// The text between the first pair of single quotes ("Unknown database 'shop'" → "shop").
fn quoted(message: &str) -> Option<&str> {
    let start = message.find('\'')? + 1;
    let end = start + message[start..].rfind('\'')?;
    Some(&message[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use mysql_async::ServerError;

    fn endpoint() -> MysqlEndpoint {
        MysqlEndpoint { host: "db.example.com".into(), port: 3307, user: "app".into(), database: Some("shop".into()), ..Default::default() }
    }

    fn server(code: u16, message: &str) -> Error {
        Error::Server(ServerError { code, message: message.into(), state: "HY000".into() })
    }

    #[test]
    fn known_server_errors_have_their_own_message() {
        let e = endpoint();
        assert_eq!(error_message(&server(1045, "Access denied for user 'app'@'10.0.0.2' (using password: YES)"), &e), "Access denied for app@db.example.com: check the user and password.");
        assert_eq!(error_message(&server(1049, "Unknown database 'nope_x'"), &e), "The database nope_x doesn't exist.");
        assert_eq!(error_message(&server(1205, "Lock wait timeout exceeded; try restarting transaction"), &e), LOCKED);
        assert_eq!(error_message(&server(1317, "Query execution was interrupted"), &e), "canceling statement due to user request");
        assert_eq!(error_message(&server(1792, "Cannot execute statement in a READ ONLY transaction."), &e), "This connection is read-only.");
        assert_eq!(error_message(&server(2013, "Lost connection to MySQL server during query"), &e), "The connection to the server was lost.");
        assert_eq!(error_message(&server(1064, "You have an error in your SQL syntax; ... at line 2"), &e), "You have an error in your SQL syntax; ... at line 2");
    }

    #[test]
    fn other_server_errors_keep_the_message_and_add_the_code() {
        assert_eq!(error_message(&server(1146, "Table 'shop.nope' doesn't exist"), &endpoint()), "Table 'shop.nope' doesn't exist (MySQL error 1146)");
    }

    #[test]
    fn network_errors_name_the_server_or_say_the_connection_was_lost() {
        let refused = Error::Io(IoError::Io(std::io::Error::new(ErrorKind::ConnectionRefused, "Connection refused")));
        assert_eq!(error_message(&refused, &endpoint()), "Couldn't reach db.example.com:3307: Connection refused");
        let default_port = MysqlEndpoint { port: 0, ..endpoint() };
        assert!(error_message(&refused, &default_port).starts_with("Couldn't reach db.example.com:3306: "));
        let socket = MysqlEndpoint { host: "/tmp/mysql.sock".into(), ..endpoint() };
        assert!(error_message(&refused, &socket).starts_with("Couldn't reach /tmp/mysql.sock: "));

        let eof = Error::Io(IoError::Io(std::io::Error::new(ErrorKind::UnexpectedEof, "eof")));
        assert_eq!(error_message(&eof, &endpoint()), LOST);
        assert_eq!(error_message(&Error::Driver(DriverError::ConnectionClosed), &endpoint()), LOST);
        assert_eq!(error_message(&server(4031, "The client was disconnected by the server because of inactivity."), &endpoint()), LOST);
    }

    #[test]
    fn a_certificate_that_does_not_name_the_host_says_so() {
        let text = "invalid peer certificate: certificate not valid for name \"127.0.0.1\"; certificate is not valid for any names (according to its subjectAltName extension)";
        let error = || Error::Io(IoError::Io(std::io::Error::new(ErrorKind::InvalidData, text)));
        let with = |mode: &str| MysqlEndpoint {
            tls: Some(crate::db::net::tls::TlsSettings { mode: mode.into(), ..Default::default() }),
            ..endpoint()
        };
        assert_eq!(
            error_message(&error(), &with("verify-ca")),
            "The server's certificate doesn't name db.example.com, and verifying only the CA isn't possible yet. \
             Use Required, or issue a certificate that names the host."
        );
        assert_eq!(error_message(&error(), &with("verify-full")), "The server's certificate doesn't name db.example.com.");
        // Other certificate problems keep the generic text
        let other = Error::Io(IoError::Io(std::io::Error::new(ErrorKind::InvalidData, "invalid peer certificate: UnknownIssuer")));
        assert_eq!(error_message(&other, &with("verify-ca")), "Couldn't reach db.example.com:3307: invalid peer certificate: UnknownIssuer");
    }
}
