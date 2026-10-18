//! What MySQL and MariaDB share because they speak the same protocol: connection options,
//! error messages and values (later also the statement splitter).

pub mod connect;
pub mod errors;
pub mod values;
