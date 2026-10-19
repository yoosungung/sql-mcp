//! AST-based identifier rewrite (spec Folder/Visitor pattern).

use std::collections::HashMap;
use std::ops::ControlFlow;

use sqlparser::ast::{Expr, Ident, Query, VisitMut, VisitorMut};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::AppError;

/// Rewrite any `Query` subqueries inside `expr` using `compile_fn`.
///
/// `compile_fn` returns `Ok(Some(sql))` to replace, `Ok(None)` to leave untouched
/// (literal / no cube), or `Err` to fail closed (D24).
pub fn rewrite_subqueries<F>(expr: &str, mut compile_fn: F) -> Result<String, AppError>
where
    F: FnMut(&str) -> Result<Option<String>, AppError>,
{
    let lower = expr.to_ascii_lowercase();
    if !lower.contains("select") {
        return Ok(expr.to_string());
    }
    let wrapped = format!("SELECT ({expr}) AS _e");
    let dialect = GenericDialect {};
    let mut stmts = Parser::parse_sql(&dialect, &wrapped)
        .map_err(|e| AppError::Validation(format!("subquery rewrite parse: {e}")))?;
    let Some(sqlparser::ast::Statement::Query(q)) = stmts.get_mut(0) else {
        return Err(AppError::Validation("rewrite: expected query".into()));
    };
    let sqlparser::ast::SetExpr::Select(select) = q.body.as_mut() else {
        return Err(AppError::Validation("rewrite: expected select".into()));
    };
    let target_expr = match select.projection.get_mut(0) {
        Some(sqlparser::ast::SelectItem::ExprWithAlias { expr, .. }) => expr,
        Some(sqlparser::ast::SelectItem::UnnamedExpr(e)) => e,
        _ => return Err(AppError::Validation("rewrite: bad projection".into())),
    };
    let mut v = SubqueryRewriter {
        compile_fn: &mut compile_fn,
        depth: 0,
        error: None,
    };
    let _ = target_expr.visit(&mut v);
    if let Some(err) = v.error {
        return Err(err);
    }
    Ok(strip_outer_parens(&target_expr.to_string()))
}

struct SubqueryRewriter<'a, F> {
    compile_fn: &'a mut F,
    depth: usize,
    error: Option<AppError>,
}

impl<F> VisitorMut for SubqueryRewriter<'_, F>
where
    F: FnMut(&str) -> Result<Option<String>, AppError>,
{
    type Break = ();

    fn pre_visit_query(&mut self, query: &mut Query) -> ControlFlow<Self::Break> {
        if self.error.is_some() {
            return ControlFlow::Break(());
        }
        if self.depth == 0 {
            let sub_sql = query.to_string();
            match (self.compile_fn)(&sub_sql) {
                Ok(Some(compiled)) => {
                    let dialect = GenericDialect {};
                    match Parser::parse_sql(&dialect, &compiled) {
                        Ok(mut stmts) => {
                            if let Some(sqlparser::ast::Statement::Query(new_q)) = stmts.pop() {
                                *query = *new_q;
                            } else {
                                self.error = Some(AppError::Validation(
                                    "subquery compile did not return a SELECT".into(),
                                ));
                                return ControlFlow::Break(());
                            }
                        }
                        Err(e) => {
                            self.error = Some(AppError::Validation(format!(
                                "compiled subquery parse failed: {e}"
                            )));
                            return ControlFlow::Break(());
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    self.error = Some(e);
                    return ControlFlow::Break(());
                }
            }
        }
        self.depth += 1;
        ControlFlow::Continue(())
    }

    fn post_visit_query(&mut self, _query: &mut Query) -> ControlFlow<Self::Break> {
        self.depth = self.depth.saturating_sub(1);
        ControlFlow::Continue(())
    }
}

/// Replace bare identifiers in `expr` using `map` (name → SQL fragment already trusted).
/// Identifiers inside nested `(SELECT ...)` subqueries are not rewritten with outer cube mappings.
pub fn rewrite_idents(expr: &str, map: &HashMap<String, String>) -> Result<String, AppError> {
    if map.is_empty() {
        return Ok(expr.to_string());
    }
    let wrapped = format!("SELECT ({expr}) AS _e");
    let dialect = GenericDialect {};
    let mut stmts = Parser::parse_sql(&dialect, &wrapped)
        .map_err(|e| AppError::Validation(format!("rewrite parse: {e}")))?;
    let Some(sqlparser::ast::Statement::Query(q)) = stmts.get_mut(0) else {
        return Err(AppError::Validation("rewrite: expected query".into()));
    };
    let sqlparser::ast::SetExpr::Select(select) = q.body.as_mut() else {
        return Err(AppError::Validation("rewrite: expected select".into()));
    };
    let Some(sqlparser::ast::SelectItem::ExprWithAlias { expr, .. }) =
        select.projection.get_mut(0)
    else {
        // UnnamedExpr path
        match select.projection.get_mut(0) {
            Some(sqlparser::ast::SelectItem::UnnamedExpr(e)) => {
                let mut v = IdentRewriter {
                    map,
                    subquery_depth: 0,
                };
                let _ = e.visit(&mut v);
                return Ok(strip_outer_parens(&e.to_string()));
            }
            _ => return Err(AppError::Validation("rewrite: bad projection".into())),
        }
    };
    // Expr is nested as (inner) from SELECT (expr)
    let mut v = IdentRewriter {
        map,
        subquery_depth: 0,
    };
    let _ = expr.visit(&mut v);
    Ok(strip_outer_parens(&expr.to_string()))
}

struct IdentRewriter<'a> {
    map: &'a HashMap<String, String>,
    subquery_depth: usize,
}

impl VisitorMut for IdentRewriter<'_> {
    type Break = ();

    fn pre_visit_query(&mut self, _query: &mut Query) -> ControlFlow<Self::Break> {
        self.subquery_depth += 1;
        ControlFlow::Continue(())
    }

    fn post_visit_query(&mut self, _query: &mut Query) -> ControlFlow<Self::Break> {
        self.subquery_depth = self.subquery_depth.saturating_sub(1);
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        if self.subquery_depth == 0 {
            if let Expr::Identifier(Ident { value, .. }) = expr {
                if let Some(repl) = self.map.get(value.as_str()) {
                    *expr = parse_expr_fragment(repl);
                }
            }
        }
        ControlFlow::Continue(())
    }
}

fn parse_expr_fragment(sql: &str) -> Expr {
    let wrapped = format!("SELECT ({sql})");
    let dialect = GenericDialect {};
    if let Ok(stmts) = Parser::parse_sql(&dialect, &wrapped) {
        if let Some(sqlparser::ast::Statement::Query(q)) = stmts.into_iter().next() {
            if let sqlparser::ast::SetExpr::Select(select) = *q.body {
                if let Some(sqlparser::ast::SelectItem::UnnamedExpr(e)) =
                    select.projection.into_iter().next()
                {
                    return unwrap_nested(e);
                }
            }
        }
    }
    Expr::Identifier(Ident::new(sql))
}

fn unwrap_nested(e: Expr) -> Expr {
    match e {
        Expr::Nested(inner) => *inner,
        other => other,
    }
}

fn strip_outer_parens(s: &str) -> String {
    let t = s.trim();
    if t.starts_with('(') && t.ends_with(')') {
        let inner = &t[1..t.len() - 1];
        // only strip if balanced single pair
        if paren_balanced(inner) {
            return inner.to_string();
        }
    }
    t.to_string()
}

fn paren_balanced(s: &str) -> bool {
    let mut depth = 0i32;
    for c in s.chars() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_fact_inside_sum() {
        let mut map = HashMap::new();
        map.insert("sale_amount".into(), "\"orders\".\"sale_amount\"".into());
        let out = rewrite_idents("sum(sale_amount)", &map).unwrap();
        assert!(out.contains("orders"), "{out}");
        assert!(out.to_ascii_lowercase().contains("sum"), "{out}");
    }

    #[test]
    fn rewrites_outer_idents_but_preserves_subquery_scope() {
        let mut map = HashMap::new();
        map.insert("sale_amount".into(), "\"orders\".\"sale_amount\"".into());
        let out = rewrite_idents(
            "sale_amount > (SELECT AVG(sale_amount) FROM other_tbl)",
            &map,
        )
        .unwrap();
        assert!(
            out.starts_with("\"orders\".\"sale_amount\" >"),
            "outer ident should be rewritten: {out}"
        );
        assert!(
            out.contains("(SELECT AVG(sale_amount) FROM other_tbl)"),
            "inner subquery ident should not be rewritten to outer table: {out}"
        );
    }

    #[test]
    fn rewrites_window_over_clause_idents() {
        let mut map = HashMap::new();
        map.insert("region_code".into(), "\"orders\".\"region_code\"".into());
        map.insert("sale_amount".into(), "\"orders\".\"sale_amount\"".into());
        let out = rewrite_idents(
            "ROW_NUMBER() OVER (PARTITION BY region_code ORDER BY sale_amount DESC)",
            &map,
        )
        .unwrap();
        assert!(out.contains("\"orders\".\"region_code\""), "{out}");
        assert!(out.contains("\"orders\".\"sale_amount\""), "{out}");
    }

    #[test]
    fn leaves_unknown_idents() {
        let map = HashMap::new();
        let out = rewrite_idents("status_code = 'PAID'", &map).unwrap();
        assert!(out.contains("status_code") || out.contains("PAID"), "{out}");
    }
}
