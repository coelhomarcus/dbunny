//! The AI assistant: the database tools an agent may use, how a conversation is kept, and
//! what the agent is told about the database.

pub mod approvals;
pub mod context;
pub mod conversation;
pub mod events;
#[cfg(test)]
mod fake;
pub mod result;
pub mod tools;
