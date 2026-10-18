//! What MySQL and MariaDB share because they speak the same protocol: connection options,
//! error messages and values (later also the statement splitter).

pub mod connect;
pub mod errors;
#[allow(dead_code)] // used by the data and the console (Tasks 7 and 9)
pub mod values;
