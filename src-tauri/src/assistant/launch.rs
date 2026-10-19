//! How the app starts the Claude agent: the command, the PATH it needs and the Node it runs on.

use std::time::Duration;

/// The adapter that speaks the Agent Client Protocol for Claude Code, pinned.
pub const ADAPTER_PACKAGE: &str = "@agentclientprotocol/claude-agent-acp@0.85.1";

const MIN_NODE_MAJOR: u32 = 20;
/// Windows creation flag: no console window for a program of a GUI app.
#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SHELL_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(unix)]
const PATH_START: &str = "<<dbunny-path>>";
#[cfg(unix)]
const PATH_END: &str = "<<dbunny-end>>";

#[derive(Clone, Debug, PartialEq)]
pub struct AgentCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub fn default_command(windows: bool) -> AgentCommand {
    AgentCommand { program: if windows { "npx.cmd" } else { "npx" }.into(), args: vec!["-y".into(), ADAPTER_PACKAGE.into()] }
}

/// What to type in a terminal to sign in to Claude with the Claude Code that comes with the adapter, for
/// someone who never installed `claude` themselves.
pub fn login_command() -> String {
    format!("npx -y {ADAPTER_PACKAGE} --cli auth login")
}

/// A command the person typed, split the way a shell would: spaces separate, quotes group,
/// a backslash escapes the next character.
pub fn parse_custom(text: &str) -> Result<AgentCommand, String> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, c) if c.is_whitespace() => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, '\'') | (None, '"') => {
                quote = Some(c);
                started = true;
            }
            (None, '\\') => {
                word.push(chars.next().ok_or("The command ends with a backslash")?);
                started = true;
            }
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('"'), '\\') => match chars.next() {
                Some(next @ ('"' | '\\')) => word.push(next),
                Some(next) => {
                    word.push('\\');
                    word.push(next);
                }
                None => return Err("A quote in the command isn't closed".into()),
            },
            (_, c) => {
                word.push(c);
                started = true;
            }
        }
    }
    if quote.is_some() {
        return Err("A quote in the command isn't closed".into());
    }
    if started {
        words.push(word);
    }
    let mut words = words.into_iter();
    let program = words.next().ok_or("Enter a command")?;
    Ok(AgentCommand { program, args: words.collect() })
}

pub fn parse_node_version(output: &str) -> Option<(u32, u32, u32)> {
    let mut parts = output.trim().strip_prefix('v')?.split('.').map(|part| part.parse::<u32>().ok());
    Some((parts.next()??, parts.next()??, parts.next()??))
}

pub fn node_is_supported(version: (u32, u32, u32)) -> bool {
    version.0 >= MIN_NODE_MAJOR
}

/// The PATH the person's login shell builds: an app opened from the Dock doesn't inherit it,
/// and Node installed by a version manager lives there.
#[cfg(unix)]
pub async fn login_shell_path() -> Option<String> {
    login_shell_path_with(&std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())).await
}

#[cfg(not(unix))]
pub async fn login_shell_path() -> Option<String> {
    None
}

#[cfg(unix)]
pub async fn login_shell_path_with(shell: &str) -> Option<String> {
    let run = tokio::process::Command::new(shell)
        .args(["-l", "-i", "-c", &format!("printf '%s%s%s' '{PATH_START}' \"$PATH\" '{PATH_END}'")])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let output = tokio::time::timeout(SHELL_TIMEOUT, run).await.ok()?.ok()?;
    // The rc files of an interactive shell may print too: only what is between the markers is the PATH
    let stdout = String::from_utf8_lossy(&output.stdout);
    let after = &stdout[stdout.find(PATH_START)? + PATH_START.len()..];
    let path = after[..after.find(PATH_END)?].trim();
    (output.status.success() && !path.is_empty()).then(|| path.to_string())
}

/// The Node on `path` (or the app's own PATH): its version, `Err(None)` when there is none and
/// `Err(Some(version))` when it is too old.
pub async fn check_node(path: Option<&str>) -> Result<(u32, u32, u32), Option<String>> {
    let mut command = tokio::process::Command::new("node");
    command.arg("--version").stdin(std::process::Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    let output = tokio::time::timeout(SHELL_TIMEOUT, command.output()).await.map_err(|_| None)?.map_err(|_| None)?;
    let version = parse_node_version(&String::from_utf8_lossy(&output.stdout)).ok_or(None)?;
    if node_is_supported(version) {
        Ok(version)
    } else {
        Err(Some(format!("{}.{}.{}", version.0, version.1, version.2)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_command_runs_the_pinned_adapter() {
        assert_eq!(default_command(false), AgentCommand { program: "npx".into(), args: vec!["-y".into(), ADAPTER_PACKAGE.into()] });
        assert_eq!(default_command(true).program, "npx.cmd");
        assert!(ADAPTER_PACKAGE.starts_with("@agentclientprotocol/claude-agent-acp@"));
    }

    #[test]
    fn the_login_command_runs_the_claude_that_comes_with_the_adapter() {
        assert_eq!(login_command(), format!("npx -y {ADAPTER_PACKAGE} --cli auth login"));
    }

    #[test]
    fn custom_command_splits_like_a_shell() {
        let command = parse_custom("/opt/x/claude-agent-acp --flag \"a b\"").unwrap();
        assert_eq!(command, AgentCommand { program: "/opt/x/claude-agent-acp".into(), args: vec!["--flag".into(), "a b".into()] });
        assert_eq!(parse_custom("run 'it''s' x\\ y").unwrap().args, vec!["its", "x y"]);
        assert_eq!(parse_custom("run \"say \\\"hi\\\"\"").unwrap().args, vec!["say \"hi\""]);
        assert_eq!(parse_custom("  one   two  ").unwrap(), AgentCommand { program: "one".into(), args: vec!["two".into()] });
        assert_eq!(parse_custom("").unwrap_err(), "Enter a command");
        assert_eq!(parse_custom("   ").unwrap_err(), "Enter a command");
        assert!(parse_custom("run \"unclosed").is_err());
    }

    #[test]
    fn node_versions() {
        assert_eq!(parse_node_version("v22.14.0\n"), Some((22, 14, 0)));
        assert_eq!(parse_node_version("v18.19.1"), Some((18, 19, 1)));
        assert_eq!(parse_node_version("garbage"), None);
        assert!(node_is_supported((22, 14, 0)) && node_is_supported((20, 0, 0)));
        assert!(!node_is_supported((18, 19, 1)));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn login_shell_path_ignores_what_the_shell_prints_on_its_own() {
        use std::os::unix::fs::PermissionsExt;
        // A shell whose rc files greet you, before and after the command
        let dir = std::env::temp_dir().join(format!("dbunny-test-shell-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let shell = dir.join("noisy-shell");
        std::fs::write(&shell, "#!/bin/sh\necho 'welcome back'\n/bin/sh -c \"$4\"\necho 'bye'\n").unwrap();
        std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
        let path = login_shell_path_with(shell.to_str().unwrap()).await.unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!path.contains("welcome") && !path.contains("bye") && path.contains("/bin"), "{path}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn login_shell_path_reads_path() {
        let path = login_shell_path_with("/bin/sh").await.unwrap();
        assert!(path.contains("/bin"), "{path}");
        assert!(login_shell_path_with("/definitely/not/a/shell").await.is_none());
    }

    #[tokio::test]
    async fn check_node_reports_a_missing_node() {
        assert_eq!(check_node(Some("/definitely/not/a/folder")).await, Err(None));
    }
}
