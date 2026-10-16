mod mysql;
mod oracle;
mod postgres;
mod registry;
mod sqlite;

pub use mysql::MysqlTarget;
pub use oracle::OracleTarget;
pub use postgres::PostgresTarget;
pub use registry::{TargetInfo, TargetRegistry, TargetSql};
pub use sqlite::SqliteTarget;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::AppError;

#[derive(Debug, Clone)]
pub struct TableInfo {
    pub schema: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
}

#[async_trait]
pub trait TargetDb: Send + Sync {
    fn info(&self) -> TargetInfo;
    async fn list_tables(&self, schema: Option<&str>) -> Result<Vec<TableInfo>, AppError>;
    async fn describe_table(
        &self,
        schema: Option<&str>,
        table: &str,
    ) -> Result<Vec<ColumnInfo>, AppError>;
    async fn execute_select(
        &self,
        sql: &str,
        max_rows: usize,
    ) -> Result<QueryResult, AppError>;
    async fn sample_column(
        &self,
        schema: Option<&str>,
        table: &str,
        column: &str,
        limit: usize,
    ) -> Result<Vec<Value>, AppError>;
}

#[derive(Debug, Clone)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
}
