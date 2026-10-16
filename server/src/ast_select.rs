//! Assemble a SELECT statement via sqlparser AST round-trip (Folder pattern).

use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::AppError;

#[derive(Debug, Clone, Default)]
pub struct SelectClauses {
    pub aggregation: Option<String>,
    pub having: Option<String>,
    pub order_by: Vec<String>,
    pub limit: Option<String>,
    pub offset: Option<String>,
}

/// Build `SELECT … FROM … [WHERE …] [GROUP BY …]` and normalize through the parser AST.
pub fn assemble_select(
    projections: &[(String, String)],
    from_sql: &str,
    where_parts: &[String],
    group_by: &[String],
) -> Result<String, AppError> {
    assemble_select_with_clauses(
        projections,
        from_sql,
        where_parts,
        group_by,
        &SelectClauses::default(),
    )
}

/// Build `SELECT … FROM … [WHERE …] [GROUP BY [ROLLUP|CUBE|GROUPING SETS] …] [HAVING …] [ORDER BY …] [LIMIT …] [OFFSET …]`
/// and normalize through the parser AST.
pub fn assemble_select_with_clauses(
    projections: &[(String, String)],
    from_sql: &str,
    where_parts: &[String],
    group_by: &[String],
    clauses: &SelectClauses,
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
                if t.starts_with('(') && t.ends_with(')') {
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
    let agg_mode = clauses.aggregation.as_deref().unwrap_or("group");
    if !group_by.is_empty() && agg_mode != "none" {
        let gb_items = group_by
            .iter()
            .map(|g| format!("({g})"))
            .collect::<Vec<_>>()
            .join(", ");
        match agg_mode {
            "rollup" => {
                sql.push_str(&format!(" GROUP BY ROLLUP({gb_items})"));
            }
            "cube" => {
                sql.push_str(&format!(" GROUP BY CUBE({gb_items})"));
            }
            "grouping_sets" => {
                sql.push_str(&format!(" GROUP BY GROUPING SETS ({gb_items})"));
            }
            _ => {
                sql.push_str(" GROUP BY ");
                sql.push_str(&gb_items);
            }
        }
    }
    if let Some(h) = &clauses.having {
        let ht = h.trim();
        if !ht.is_empty() {
            sql.push_str(" HAVING ");
            sql.push_str(ht);
        }
    }
    if !clauses.order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&clauses.order_by.join(", "));
    }
    if let Some(lim) = &clauses.limit {
        let lt = lim.trim();
        if !lt.is_empty() {
            sql.push_str(" LIMIT ");
            sql.push_str(lt);
        }
    }
    if let Some(off) = &clauses.offset {
        let ot = off.trim();
        if !ot.is_empty() {
            sql.push_str(" OFFSET ");
            sql.push_str(ot);
        }
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

    #[test]
    fn assembles_rollup_having_order_limit() {
        let clauses = SelectClauses {
            aggregation: Some("rollup".into()),
            having: Some("sum(\"orders\".\"sale_amount\") > 100".into()),
            order_by: vec!["\"revenue\" DESC".into()],
            limit: Some("10".into()),
            offset: Some("5".into()),
        };
        let sql = assemble_select_with_clauses(
            &[
                ("\"orders\".\"region_code\"".into(), "region_code".into()),
                ("sum(\"orders\".\"sale_amount\")".into(), "revenue".into()),
            ],
            "\"sales\".\"orders\" AS \"orders\"",
            &[],
            &["\"orders\".\"region_code\"".into()],
            &clauses,
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("ROLLUP"), "{sql}");
        assert!(u.contains("HAVING"), "{sql}");
        assert!(u.contains("ORDER BY"), "{sql}");
        assert!(u.contains("LIMIT 10"), "{sql}");
        assert!(u.contains("OFFSET 5"), "{sql}");
        assert_select_only(&sql).unwrap();
    }
}
