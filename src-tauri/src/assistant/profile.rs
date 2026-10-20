//! What differs between the agents the assistant can run: the command, its environment, what a
//! session is told, how the system prompt reaches it and how to sign in. The rest of the client
//! is the same for all of them.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::launch::{self, AgentCommand};

/// The adapter that speaks the Agent Client Protocol for Codex, pinned.
pub const CODEX_PACKAGE: &str = "@agentclientprotocol/codex-acp@2.1.1";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AgentKind {
    Claude,
    Codex,
    Custom,
}

/// How the instructions about the database reach the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptDelivery {
    /// As the session's system prompt.
    System,
    /// In the first message of the conversation: the agent has no system prompt per session.
    FirstMessage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AgentProfile {
    pub kind: AgentKind,
    pub command: AgentCommand,
    /// Set on the agent's process, over what it inherits.
    pub env: Vec<(String, String)>,
}

/// The profile of `kind`. `custom` is the person's own command, which only `Custom` takes.
pub fn resolve(kind: AgentKind, custom: Option<&str>, windows: bool) -> Result<AgentProfile, String> {
    let npx = if windows { "npx.cmd" } else { "npx" };
    match kind {
        AgentKind::Claude => Ok(AgentProfile { kind, command: launch::default_command(windows), env: vec![] }),
        AgentKind::Codex => Ok(AgentProfile {
            kind,
            command: AgentCommand { program: npx.into(), args: vec!["-y".into(), CODEX_PACKAGE.into()] },
            env: vec![
                ("INITIAL_AGENT_MODE".into(), "read-only".into()),
                ("CODEX_CONFIG".into(), codex_config(&format!("{}\n\n{}", super::context::BASE_PROMPT, super::context::CODEX_CONTEXT_NOTE)).to_string()),
            ],
        }),
        AgentKind::Custom => {
            let text = custom.map(str::trim).filter(|text| !text.is_empty()).ok_or("Enter a command")?;
            Ok(AgentProfile { kind, command: launch::parse_custom(text)?, env: vec![] })
        }
    }
}

/// What Codex is started with so that, of its own tools, none is left: the session is read-only
/// and the shell, the plugins, the browser and the web are off. `rules` are its instructions.
pub fn codex_config(rules: &str) -> Value {
    json!({
        "features": {
            "shell_tool": false, "unified_exec": false, "multi_agent": false, "plugins": false, "apps": false,
            "browser_use": false, "computer_use": false, "image_generation": false, "goals": false,
            "in_app_browser": false, "browser_use_external": false, "browser_use_full_cdp_access": false, "code_mode_host": false,
            "hooks": false, "tool_suggest": false,
        },
        "web_search": "disabled",
        "include_environment_context": false,
        "developer_instructions": rules,
    })
}

impl AgentProfile {
    /// Codex runs with a home of its own: the person's `~/.codex` has tools of their own (a Node REPL
    /// the ChatGPT app installs, say) that Codex runs without asking, and a login that DBunny's
    /// sign-out must not touch.
    pub fn with_home(mut self, home: &std::path::Path) -> AgentProfile {
        if self.kind == AgentKind::Codex {
            self.env.push(("CODEX_HOME".into(), home.to_string_lossy().into_owned()));
        }
        self
    }

    /// How the instructions reach this agent. `takes_system_prompt` is whether it said it takes one per
    /// session: an agent of the person's own that doesn't gets them in the first message instead.
    pub fn delivery_for(&self, takes_system_prompt: bool) -> PromptDelivery {
        match self.kind {
            AgentKind::Codex => PromptDelivery::FirstMessage,
            AgentKind::Custom if !takes_system_prompt => PromptDelivery::FirstMessage,
            _ => PromptDelivery::System,
        }
    }

    /// The `_meta` of a new session: what the agent is told to leave out.
    pub fn session_meta(&self) -> Option<Value> {
        match self.kind {
            AgentKind::Codex => None,
            _ => Some(super::agent::session_meta()),
        }
    }

    /// What to type in a terminal to sign in, for the agent that is signed in that way.
    pub fn login_command(&self) -> Option<String> {
        (self.kind == AgentKind::Claude).then(launch::login_command)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::assistant::launch;

    fn config_of(profile: &AgentProfile) -> Value {
        let (_, json) = profile.env.iter().find(|(key, _)| key == "CODEX_CONFIG").expect("CODEX_CONFIG is set");
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn claude_resolves_to_the_pinned_adapter() {
        let profile = resolve(AgentKind::Claude, None, false).unwrap();
        assert_eq!(profile.command, launch::default_command(false));
        assert!(profile.env.is_empty());
        assert_eq!(profile.delivery_for(false), PromptDelivery::System);
        assert_eq!(profile.delivery_for(true), PromptDelivery::System);
        assert!(profile.session_meta().is_some());
        assert_eq!(profile.login_command(), Some(launch::login_command()));
    }

    #[test]
    fn codex_resolves_to_its_adapter_with_a_read_only_environment() {
        let profile = resolve(AgentKind::Codex, None, false).unwrap();
        assert_eq!(profile.command, launch::AgentCommand { program: "npx".into(), args: vec!["-y".into(), CODEX_PACKAGE.into()] });
        assert_eq!(resolve(AgentKind::Codex, None, true).unwrap().command.program, "npx.cmd");
        assert!(profile.env.contains(&("INITIAL_AGENT_MODE".to_string(), "read-only".to_string())));
        let config = config_of(&profile);
        for feature in [
            "shell_tool", "unified_exec", "multi_agent", "plugins", "apps", "browser_use", "computer_use", "in_app_browser", "browser_use_external",
            "browser_use_full_cdp_access", "code_mode_host", "hooks", "tool_suggest",
        ] {
            assert_eq!(config["features"][feature], false, "{feature}");
        }
        assert_eq!(config["web_search"], "disabled");
        assert_eq!(config["include_environment_context"], false);
        assert!(!config["developer_instructions"].as_str().unwrap().is_empty());
        assert_eq!(profile.delivery_for(true), PromptDelivery::FirstMessage);
        assert!(profile.session_meta().is_none());
        assert_eq!(profile.login_command(), None);
    }

    #[test]
    fn codex_can_be_given_a_home_of_its_own() {
        let home = std::path::Path::new("/data/codex-home");
        let profile = resolve(AgentKind::Codex, None, false).unwrap().with_home(home);
        assert!(profile.env.contains(&("CODEX_HOME".to_string(), "/data/codex-home".to_string())), "{:?}", profile.env);
        // Only Codex has one
        assert!(resolve(AgentKind::Claude, None, false).unwrap().with_home(home).env.is_empty());
    }

    #[test]
    fn codex_is_given_the_rules_and_told_where_the_database_comes_from() {
        let instructions = config_of(&resolve(AgentKind::Codex, None, false).unwrap())["developer_instructions"].as_str().unwrap().to_string();
        assert!(instructions.starts_with(super::super::context::BASE_PROMPT), "{instructions}");
        assert!(instructions.contains("<dbunny-context>") && instructions.contains("first message"), "{instructions}");
    }

    #[test]
    fn custom_needs_a_command_and_ignores_the_environment() {
        assert_eq!(resolve(AgentKind::Custom, None, false).unwrap_err(), "Enter a command");
        assert_eq!(resolve(AgentKind::Custom, Some("   "), false).unwrap_err(), "Enter a command");
        let profile = resolve(AgentKind::Custom, Some("/opt/x --flag"), false).unwrap();
        assert_eq!(profile.command.program, "/opt/x");
        assert!(profile.env.is_empty());
        // The agent of the person's choosing gets the instructions the way it can take them
        assert_eq!(profile.delivery_for(true), PromptDelivery::System);
        assert_eq!(profile.delivery_for(false), PromptDelivery::FirstMessage);
        assert_eq!(profile.login_command(), None);
    }

    #[test]
    fn a_command_is_ignored_for_the_known_agents() {
        assert_eq!(resolve(AgentKind::Claude, Some("/opt/x"), false).unwrap().command, launch::default_command(false));
        assert_eq!(resolve(AgentKind::Codex, Some("/opt/x"), false).unwrap().command.args[1], CODEX_PACKAGE);
    }
}
