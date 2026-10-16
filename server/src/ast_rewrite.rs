//! AST-based identifier rewrite (spec Folder/Visitor pattern).

use std::collections::HashMap;
use std::ops::ControlFlow;

use sqlparser::ast::{Expr, Ident, VisitMut, VisitorMut};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::error::AppError;

/// Replace bare identifiers in `expr` using `map` (name → SQL fragment already trusted).
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
                let mut v = IdentRewriter { map };
                let _ = e.visit(&mut v);
                return Ok(strip_outer_parens(&e.to_string()));
            }
            _ => return Err(AppError::Validation("rewrite: bad projection".into())),
        }
    };
    // Expr is nested as (inner) from SELECT (expr)
    let mut v = IdentRewriter { map };
    let _ = expr.visit(&mut v);
    Ok(strip_outer_parens(&expr.to_string()))
}

struct IdentRewriter<'a> {
    map: &'a HashMap<String, String>,
}

impl VisitorMut for IdentRewriter<'_> {
    type Break = ();

    fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        if let Expr::Identifier(Ident { value, .. }) = expr {
            if let Some(repl) = self.map.get(value.as_str()) {
                *expr = parse_expr_fragment(repl);
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
    fn leaves_unknown_idents() {
        let map = HashMap::new();
        let out = rewrite_idents("status_code = 'PAID'", &map).unwrap();
        assert!(out.contains("status_code") || out.contains("PAID"), "{out}");
    }
}
