//! The errors a command hands to the interface: the English text people could always read, and,
//! when DBunny itself made the error, a code and its parameters so the interface can say it in
//! the language of the app. What a database server says is passed on as it came, without a code.

use std::fmt;

use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AppError {
    code: Option<&'static str>,
    params: Map<String, Value>,
    message: String,
}

impl AppError {
    /// An error DBunny made: `message` is the English text, with the parameters already in it.
    pub fn coded(code: &'static str, message: impl Into<String>) -> Self {
        AppError { code: Some(code), params: Map::new(), message: message.into() }
    }

    /// A value the translated message fills in: a path, a number, the name of an object.
    pub fn param(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.params.insert(name.to_string(), value.into());
        self
    }

    pub fn code(&self) -> Option<&'static str> {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        AppError { code: None, params: Map::new(), message }
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        AppError::from(message.to_string())
    }
}

/// For code that still returns plain strings: the English text.
impl From<AppError> for String {
    fn from(error: AppError) -> Self {
        error.message
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn from_below() -> Result<(), String> {
        Err("the driver said no".to_string())
    }

    fn command() -> Result<(), AppError> {
        from_below()?;
        Ok(())
    }

    #[test]
    fn a_plain_string_becomes_an_error_without_code() {
        let error = AppError::from("host is required");
        assert_eq!(error.code(), None);
        assert_eq!(error.message(), "host is required");
        assert_eq!(AppError::from("x".to_string()), AppError::from("x"));
    }

    #[test]
    fn a_coded_error_carries_its_params_and_its_english_message() {
        let error = AppError::coded("saved.notFound", "Connection with id 7 not found").param("id", 7).param("name", "prod");
        assert_eq!(error.code(), Some("saved.notFound"));
        assert_eq!(error.message(), "Connection with id 7 not found");
        assert_eq!(error.to_string(), "Connection with id 7 not found");
    }

    #[test]
    fn it_serializes_as_code_params_and_message() {
        let coded = AppError::coded("connection.hostRequired", "host is required");
        assert_eq!(serde_json::to_value(&coded).unwrap(), json!({"code": "connection.hostRequired", "params": {}, "message": "host is required"}));
        let with_params = AppError::coded("saved.notFound", "not found").param("id", 7);
        assert_eq!(serde_json::to_value(&with_params).unwrap(), json!({"code": "saved.notFound", "params": {"id": 7}, "message": "not found"}));
        let plain = AppError::from("the server said no");
        assert_eq!(serde_json::to_value(&plain).unwrap(), json!({"code": null, "params": {}, "message": "the server said no"}));
    }

    #[test]
    fn code_that_still_returns_strings_gets_the_english_message() {
        let text: String = AppError::coded("saved.notFound", "Connection with id 7 not found").into();
        assert_eq!(text, "Connection with id 7 not found");
    }

    #[test]
    fn the_question_mark_turns_a_string_error_into_an_app_error() {
        let error = command().unwrap_err();
        assert_eq!(error.code(), None);
        assert_eq!(error.message(), "the driver said no");
    }
}
