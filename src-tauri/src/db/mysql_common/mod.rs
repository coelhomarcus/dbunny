//! What MySQL and MariaDB share because they speak the same protocol: connection options,
//! error messages, values and the statement splitter.

pub mod connect;
pub mod driver;
pub mod errors;
pub mod flavor;
pub mod split;
pub mod values;
pub mod version;
#[cfg(test)]
pub(crate) mod tests_support;
