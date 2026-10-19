//! The AI assistant: the database tools an agent may use, how a conversation is kept, and
//! what the agent is told about the database.

pub mod agent;
pub mod approvals;
#[cfg(test)]
mod claude_tests;
pub mod context;
pub mod conversation;
pub mod events;
pub mod launch;
pub mod mcp;
#[cfg(test)]
mod fake;
#[cfg(test)]
mod fake_agent;
pub mod result;
pub mod tools;
