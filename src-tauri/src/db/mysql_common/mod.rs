//! What MySQL and MariaDB share because they speak the same protocol: connection options,
//! error messages, values and the statement splitter.

pub mod connect;
pub mod errors;
pub mod split;
pub mod values;
