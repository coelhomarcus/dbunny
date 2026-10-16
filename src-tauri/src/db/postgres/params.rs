//! How the shared builders' text parameters travel to a PostgreSQL server.

use bytes::BytesMut;
use std::error::Error;
use tokio_postgres::types::{to_sql_checked, Format, IsNull, ToSql, Type};

use crate::db::sql::filter::TextParam;

impl ToSql for TextParam {
    fn to_sql(&self, _ty: &Type, out: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        out.extend_from_slice(self.0.as_bytes());
        Ok(IsNull::No)
    }

    fn accepts(_ty: &Type) -> bool {
        true
    }

    fn encode_format(&self, _ty: &Type) -> Format {
        Format::Text
    }

    to_sql_checked!();
}
