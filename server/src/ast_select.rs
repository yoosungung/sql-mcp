//! Assemble a SELECT statement via sqlparser AST round-trip (Folder pattern).

use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::AppError;

/// Build `SELECT … FROM … [WHERE …] [GROUP BY …]` and normalize through the parser AST.
pub fn assemble_select(
    projections: &[(String, String)],
    from_sql: &str,
    where_parts: &[String],
    group_by: &[String],
) -> Result<String, AppError> {
    if projections.is_empty() {
        return Err(AppError::Validation("SELECT needs projections".into()));
    }
    let proj = projections
        .iter()
        .map(|(expr, alias)| format!("({expr}) AS \"{alias}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!("SELECT {proj} FROM {from_sql}");
    if !where_parts.is_empty() {
        let w = where_parts
            .iter()
            .map(|p| {
                let t = p.trim();
                if t.starts_with('(') {
                    t.to_string()
                } else {
                    format!("({t})")
                }
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        sql.push_str(" WHERE ");
        sql.push_str(&w);
    }
    if !group_by.is_empty() {
        let gb = group_by
            .iter()
            .map(|g| format!("({g})"))
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(" GROUP BY ");
        sql.push_str(&gb);
    }
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, &sql)
        .map_err(|e| AppError::Validation(format!("assemble_select parse: {e}")))?;
    if stmts.len() != 1 {
        return Err(AppError::Validation(
            "assemble_select: expected single statement".into(),
        ));
    }
    Ok(stmts[0].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::select_guard::assert_select_only;

    #[test]
    fn assembles_group_by_query() {
        let sql = assemble_select(
            &[
                ("\"orders\".\"region_code\"".into(), "region_code".into()),
                ("sum(\"orders\".\"sale_amount\")".into(), "revenue".into()),
            ],
            "\"sales\".\"orders\" AS \"orders\"",
            &["(\"orders\".\"status_code\" = 'PAID')".into()],
            &["\"orders\".\"region_code\"".into()],
        )
        .unwrap();
        assert!(sql.to_ascii_uppercase().contains("SELECT"));
        assert!(sql.to_ascii_uppercase().contains("GROUP BY"));
        assert_select_only(&sql).unwrap();
    }
}
