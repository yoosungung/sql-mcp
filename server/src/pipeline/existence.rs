//! Rewrite literal-only projections (`SELECT 1` / `SELECT true`) to a cube existence dimension.
//! `SELECT *` stays forbidden (semantic cubes have no all-columns surface).

use serde_json::Value;
use sqlparser::ast::{Expr, Ident, Select, SelectItem, Value as AstValue};

use crate::error::AppError;

use super::{find_doc, relation_alias_cube};

/// If every projection item is a SQL literal, replace the list with the cube's existence dimension.
/// Leaves `SELECT *` untouched so existing validation still rejects it.
pub(crate) fn rewrite_literal_existence_projection(
    docs: &[Value],
    select: &mut Select,
) -> Result<(), AppError> {
    if select.projection.is_empty() || !projection_is_literal_only(&select.projection) {
        return Ok(());
    }
    let (_, cube_name) = relation_alias_cube(&select.from[0].relation)?;
    let member = resolve_existence_member(docs, &cube_name)?;
    select.projection = vec![SelectItem::UnnamedExpr(Expr::Identifier(Ident::new(
        member,
    )))];
    Ok(())
}

fn projection_is_literal_only(items: &[SelectItem]) -> bool {
    !items.is_empty()
        && items.iter().all(|item| match item {
            SelectItem::UnnamedExpr(expr) | SelectItem::ExprWithAlias { expr, .. } => {
                is_literal_expr(expr)
            }
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => false,
        })
}

fn is_literal_expr(expr: &Expr) -> bool {
    match expr {
        Expr::Value(v) => matches!(
            &v.value,
            AstValue::Number(_, _)
                | AstValue::Boolean(_)
                | AstValue::SingleQuotedString(_)
                | AstValue::DoubleQuotedString(_)
                | AstValue::Null
        ),
        Expr::Nested(inner) => is_literal_expr(inner),
        Expr::UnaryOp { expr, .. } => is_literal_expr(expr),
        _ => false,
    }
}

/// Prefer a cube dimension whose view column maps to a leaf table `pk`; else `dimensions[0]`.
pub(crate) fn resolve_existence_member(docs: &[Value], cube_name: &str) -> Result<String, AppError> {
    let cube = find_doc(docs, "cube", cube_name)?;
    let dims = cube
        .get("dimensions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| AppError::Validation(format!("cube {cube_name} has no dimensions")))?;
    if dims.is_empty() {
        return Err(AppError::Validation(format!(
            "cube {cube_name} has no dimensions for existence projection"
        )));
    }

    if let Some(view_name) = cube.get("view").and_then(|v| v.as_str()) {
        if let Ok(view) = find_doc(docs, "view", view_name) {
            if let Some(pk_dim) = dimension_matching_table_pk(docs, view, dims) {
                return Ok(pk_dim);
            }
        }
    }

    let first = dims[0]
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::Validation(format!("cube {cube_name} dimension[0] missing name"))
        })?;
    Ok(first.to_string())
}

fn dimension_matching_table_pk(
    docs: &[Value],
    view: &Value,
    dims: &[Value],
) -> Option<String> {
    let view_cols = view.get("columns")?.as_array()?;
    for dim in dims {
        let Some(dim_name) = dim.get("name").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(col_name) = dim.get("column").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(view_col) = view_cols
            .iter()
            .find(|c| c.get("name").and_then(|v| v.as_str()) == Some(col_name))
        else {
            continue;
        };
        let Some(maps) = view_col.get("mappings").and_then(|v| v.as_array()) else {
            continue;
        };
        for m in maps {
            let Some(source) = m.get("source").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(phys_col) = m.get("column").and_then(|v| v.as_str()) else {
                continue;
            };
            let Ok(table) = find_doc(docs, "table", source) else {
                continue;
            };
            let Some(pk) = table.get("pk").and_then(|v| v.as_array()) else {
                continue;
            };
            if pk.iter().any(|p| p.as_str() == Some(phys_col)) {
                return Some(dim_name.to_string());
            }
        }
    }
    None
}
