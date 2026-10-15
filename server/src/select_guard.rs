use sqlparser::ast::Statement;
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::AppError;

/// Accept a single read-only SELECT (including read WITH/CTE). Reject writes and multi-statement.
pub fn assert_select_only(sql: &str) -> Result<(), AppError> {
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, sql).map_err(|e| AppError::SqlParse(e.to_string()))?;
    if stmts.len() != 1 {
        return Err(AppError::NotSelect);
    }
    match stmts.into_iter().next() {
        Some(Statement::Query(_)) => Ok(()),
        _ => Err(AppError::NotSelect),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_simple_select() {
        assert_select_only("SELECT 1").unwrap();
    }

    #[test]
    fn accepts_select_with_read_cte() {
        assert_select_only("WITH t AS (SELECT 1 AS x) SELECT * FROM t").unwrap();
    }

    #[test]
    fn rejects_insert() {
        assert!(matches!(
            assert_select_only("INSERT INTO t VALUES (1)"),
            Err(AppError::NotSelect)
        ));
    }

    #[test]
    fn rejects_update_delete_ddl() {
        assert!(matches!(
            assert_select_only("UPDATE t SET a = 1"),
            Err(AppError::NotSelect)
        ));
        assert!(matches!(
            assert_select_only("DELETE FROM t"),
            Err(AppError::NotSelect)
        ));
        assert!(matches!(
            assert_select_only("CREATE TABLE t (id int)"),
            Err(AppError::NotSelect)
        ));
    }

    #[test]
    fn rejects_multi_statement() {
        assert!(matches!(
            assert_select_only("SELECT 1; SELECT 2"),
            Err(AppError::NotSelect)
        ));
    }

    #[test]
    fn rejects_write_cte_as_insert() {
        // INSERT … WITH is a write statement even when the CTE itself is read-only.
        assert!(matches!(
            assert_select_only("INSERT INTO u WITH t AS (SELECT 1 AS x) SELECT * FROM t"),
            Err(AppError::NotSelect)
        ));
    }

    #[test]
    fn rejects_garbage() {
        assert!(matches!(
            assert_select_only("NOT SQL"),
            Err(AppError::SqlParse(_))
        ));
    }
}
