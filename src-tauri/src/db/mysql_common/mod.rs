//! What MySQL and MariaDB share because they speak the same protocol: connection options,
//! error messages, values and the statement splitter.

pub mod alter;
pub mod catalog;
pub mod connect;
pub mod data;
pub mod driver;
pub mod editor;
pub mod errors;
pub mod flavor;
pub mod objects;
pub mod split;
pub mod structure;
pub mod transactions;
pub mod values;
pub mod version;
#[cfg(test)]
pub(crate) mod tests_support;
