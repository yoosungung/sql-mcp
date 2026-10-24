//! Multi-cube JOIN compile, ON validate/rewrite, outer projection and ad-hoc aggregate wiring.
use std::collections::HashMap;

use serde_json::Value;
use sqlparser::ast::{
    BinaryOperator, Expr, GroupByExpr, Join, JoinConstraint, JoinOperator, Query, Select,
};

use crate::compile::{
    apply_binds, assert_additive_aggregates, check_role_access, compile_cube_query, quote_ident,
    CubeProjection, CubeQuerySpec,
};
use crate::error::{unknown_member_message, AppError};
use crate::expr_guard::assert_scalar_expr;

use super::clauses::{
    classify_projection_for_cube, collect_expr_quals_and_idents,
    expand_segments_and_strip_qualifier, extract_limit, extract_offset, extract_order_by,
    extract_where_for_cube, materialize_adhoc_metrics, member_on_cube, resolve_member_side,
    ClassifiedProjection,
};
use super::existence::rewrite_literal_existence_projection;
use super::{find_doc, relation_alias_cube};

pub(crate) fn compile_cube_join(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let mut select = select.clone();
    rewrite_literal_existence_projection(docs, &mut select)?;
    let select = &select;
    let distinct = select.distinct.is_some();
    let tw = &select.from[0];
    let mut sides: Vec<(String, String)> = Vec::new(); // (alias, cube_name)
    let (alias0, cube0) = relation_alias_cube(&tw.relation)?;
    sides.push((alias0, cube0));
    for j in &tw.joins {
        let (alias, cube) = relation_alias_cube(&j.relation)?;
        sides.push((alias, cube));
    }
    for (_, cube_name) in &sides {
        let cube = find_doc(docs, "cube", cube_name)?;
        assert_additive_aggregates(select, cube)?;
    }

    // Validate and rewrite JOIN ON clauses first, recording dimensions needed from each side.
    let mut on_dims_per_side: HashMap<String, Vec<String>> = HashMap::new();
    let mut rewritten_joins: Vec<(String, &'static str, String)> = Vec::new();
    for join in &tw.joins {
        let (alias, _) = relation_alias_cube(&join.relation)?;
        let join_kw = join_keyword(&join.join_operator)?;
        let on_sql =
            rewrite_and_validate_join_on(docs, &sides, join, role, binds, &mut on_dims_per_side)?;
        rewritten_joins.push((alias, join_kw, on_sql));
    }

    // Classify each side; D23 allows mixing named metrics and ad-hoc aggregates in Grouped mode.
    let mut side_class: Vec<(String, String, ClassifiedProjection)> = Vec::new();
    let mut any_named = false;
    let mut any_adhoc = false;
    for (alias, cube_name) in &sides {
        let classified = classify_projection_for_cube(
            select,
            docs,
            cube_name,
            Some(alias),
            false,
            Some(&sides),
        )?;
        if !classified.named_metrics.is_empty() {
            any_named = true;
        }
        if !classified.adhoc_metrics.is_empty() {
            any_adhoc = true;
        }
        side_class.push((alias.clone(), cube_name.clone(), classified));
    }

    let has_explicit_gb =
        matches!(&select.group_by, GroupByExpr::Expressions(exprs, _) if !exprs.is_empty());
    let mode_ungrouped = !any_named && !any_adhoc && !has_explicit_gb;

    if mode_ungrouped {
        if select.having.is_some() {
            return Err(AppError::Validation(
                "HAVING is not allowed in dim/fact-only ungrouped SELECT (D23)".into(),
            ));
        }
        if !distinct {
            return Err(AppError::Validation(
                "ungrouped cube JOIN requires SELECT DISTINCT, GROUP BY, or metrics/aggregates to prevent fan-out (D23)".into(),
            ));
        }
    }

    let mut subqueries: HashMap<String, String> = HashMap::new();
    for (alias, cube_name, classified) in &side_class {
        let mut dims = classified.dims.clone();
        if let Some(on_dims) = on_dims_per_side.get(alias) {
            for od in on_dims {
                if !dims.iter().any(|d| d == od) {
                    dims.push(od.clone());
                }
            }
        }

        let (segments, where_exprs) =
            extract_where_for_cube(select, docs, cube_name, Some(alias), false, role)?;

        let (docs_ref_owned, metrics, explicit_group_by, ungrouped) = if mode_ungrouped {
            if dims.is_empty() {
                return Err(AppError::Validation(format!(
                    "joined cube {cube_name} (alias {alias}) must project at least one dimension or fact"
                )));
            }
            (None, Vec::new(), None, true)
        } else {
            let side_gb = group_by_dims_for_join_side(select, cube_name, alias, &dims)?;
            let (owned, metric_names) =
                materialize_adhoc_metrics(docs, cube_name, classified, &dims, &side_gb)?;
            if metric_names.is_empty() && dims.is_empty() {
                return Err(AppError::Validation(format!(
                    "joined cube {cube_name} (alias {alias}) must project or join on at least one dimension or metric"
                )));
            }
            let gb_for_spec = if !classified.named_metrics.is_empty()
                && classified.adhoc_metrics.is_empty()
                && dims.is_empty()
            {
                None
            } else {
                side_gb
            };
            (owned, metric_names, gb_for_spec, false)
        };

        let docs_ref: &[Value] = docs_ref_owned.as_deref().unwrap_or(docs);

        let mut sub_projs = classified.projections.clone();
        for d in &dims {
            if !sub_projs.iter().any(|p| p.member == *d || p.alias == *d) {
                sub_projs.push(CubeProjection {
                    member: d.clone(),
                    alias: d.clone(),
                });
            }
        }
        let projections = if ungrouped || !dims.is_empty() || !sub_projs.is_empty() {
            sub_projs
        } else {
            Vec::new()
        };

        let spec = CubeQuerySpec {
            projections,
            metrics,
            dimensions: dims,
            segments,
            where_exprs,
            explicit_group_by,
            aggregation: None,
            having: None,
            order_by: Vec::new(),
            limit: None,
            offset: None,
            ungrouped,
            distinct: ungrouped && distinct,
        };
        let physical = compile_cube_query(docs_ref, cube_name, &spec, role, binds)?;
        subqueries.insert(alias.clone(), physical);
    }

    let mut from_sql = String::new();
    let (a0, _) = &sides[0];
    let p0 = subqueries.get(a0).unwrap();
    let qa0 = quote_ident(a0)?;
    from_sql.push_str(&format!("({p0}) AS {qa0}"));
    for (alias, join_kw, on_sql) in &rewritten_joins {
        let phys = subqueries.get(alias).unwrap();
        let qalias = quote_ident(alias)?;
        from_sql.push_str(&format!(" {join_kw} ({phys}) AS {qalias} ON {on_sql}"));
    }

    let mut proj_parts = Vec::with_capacity(select.projection.len());
    for idx in 0..select.projection.len() {
        let mut found: Option<(&str, &str)> = None;
        for (alias, _cube, classified) in &side_class {
            if let Some((_, out_alias)) = classified
                .attributed_items
                .iter()
                .find(|(item_idx, _)| *item_idx == idx)
            {
                found = Some((alias.as_str(), out_alias.as_str()));
                break;
            }
        }
        let (side_alias, out_alias) = found.ok_or_else(|| {
            AppError::Validation(format!(
                "projection item #{} ({}) could not be attributed to a joined cube",
                idx + 1,
                select.projection[idx]
            ))
        })?;
        let qside = quote_ident(side_alias)?;
        let qout = quote_ident(out_alias)?;
        proj_parts.push(format!("{qside}.{qout} AS {qout}"));
    }
    if proj_parts.is_empty() {
        return Err(AppError::Validation(
            "JOIN SELECT needs at least one projection".into(),
        ));
    }

    let select_kw = if distinct {
        "SELECT DISTINCT"
    } else {
        "SELECT"
    };
    let mut sql = format!("{select_kw} {} FROM {}", proj_parts.join(", "), from_sql);
    let order_by = extract_order_by(query, None, &sides)?;
    if !order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_by.join(", "));
    }
    if let Some(lim) = extract_limit(query, binds)? {
        sql.push_str(" LIMIT ");
        sql.push_str(&lim);
    }
    if let Some(off) = extract_offset(query, binds)? {
        sql.push_str(" OFFSET ");
        sql.push_str(&off);
    }
    Ok(sql)
}

/// GROUP BY dims for one JOIN side: keep this cube's qualifiers, skip others; merge projected dims.
pub(crate) fn group_by_dims_for_join_side(
    select: &Select,
    cube_name: &str,
    cube_alias: &str,
    projected_dims: &[String],
) -> Result<Option<Vec<String>>, AppError> {
    let GroupByExpr::Expressions(exprs, _) = &select.group_by else {
        return Ok(if projected_dims.is_empty() {
            Some(Vec::new())
        } else {
            Some(projected_dims.to_vec())
        });
    };
    if exprs.is_empty() {
        return Ok(if projected_dims.is_empty() {
            Some(Vec::new())
        } else {
            Some(projected_dims.to_vec())
        });
    }
    let mut dims = Vec::new();
    for e in exprs {
        match try_extract_cube_ident_for_side(e, cube_name, cube_alias) {
            Ok(Some(d)) => {
                if projected_dims.iter().any(|x| x == &d) && !dims.iter().any(|x| x == &d) {
                    dims.push(d);
                }
            }
            Ok(None) => {} // other side's qualifier — skip
            Err(e) => return Err(e),
        }
    }
    for d in projected_dims {
        if !dims.iter().any(|x| x == d) {
            dims.push(d.clone());
        }
    }
    Ok(Some(dims))
}

pub(crate) fn try_extract_cube_ident_for_side(
    expr: &Expr,
    cube_name: &str,
    cube_alias: &str,
) -> Result<Option<String>, AppError> {
    match expr {
        Expr::Identifier(id) => Ok(Some(id.value.clone())),
        Expr::Value(v) if v.to_string().parse::<usize>().is_ok() => Ok(None),
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = &parts[0].value;
            let n = &parts[1].value;
            if q != cube_name && q != cube_alias {
                return Ok(None);
            }
            Ok(Some(n.clone()))
        }
        Expr::Nested(inner) => try_extract_cube_ident_for_side(inner, cube_name, cube_alias),
        Expr::Rollup(_) | Expr::Cube(_) | Expr::GroupingSets(_) => Err(AppError::Validation(
            "ROLLUP/CUBE/GROUPING SETS not supported in cube JOIN ad-hoc GROUP BY".into(),
        )),
        other => {
            let empty = HashMap::new();
            let mut quals = Vec::new();
            let mut bare = Vec::new();
            collect_expr_quals_and_idents(other, &mut quals, &mut bare);
            if !quals.is_empty() && quals.iter().any(|q| q != cube_name && q != cube_alias) {
                return Ok(None);
            }
            let stripped = expand_segments_and_strip_qualifier(
                other,
                &empty,
                cube_name,
                Some(cube_alias),
                false,
                None,
            )?;
            let s = stripped.to_string();
            assert_scalar_expr(&s)?;
            Ok(Some(s))
        }
    }
}

pub(crate) fn join_keyword(op: &JoinOperator) -> Result<&'static str, AppError> {
    match op {
        JoinOperator::Join(_) | JoinOperator::Inner(_) => Ok("INNER JOIN"),
        JoinOperator::Left(_) | JoinOperator::LeftOuter(_) => Ok("LEFT JOIN"),
        JoinOperator::Right(_) | JoinOperator::RightOuter(_) => Ok("RIGHT JOIN"),
        JoinOperator::FullOuter(_) => Ok("FULL JOIN"),
        JoinOperator::CrossJoin => Err(AppError::Validation(
            "CROSS JOIN of cubes is not supported".into(),
        )),
        _ => Err(AppError::Validation("unsupported cube join type".into())),
    }
}

pub(crate) fn rewrite_and_validate_join_on(
    docs: &[Value],
    sides: &[(String, String)],
    join: &Join,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    on_dims_per_side: &mut HashMap<String, Vec<String>>,
) -> Result<String, AppError> {
    let expr = match &join.join_operator {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::Left(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::Right(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c) => match c {
            JoinConstraint::On(expr) => expr,
            JoinConstraint::Using(_) => {
                return Err(AppError::Validation(
                    "USING join not supported for cubes; use ON".into(),
                ));
            }
            JoinConstraint::Natural => {
                return Err(AppError::Validation(
                    "NATURAL join not supported for cubes".into(),
                ));
            }
            JoinConstraint::None => {
                return Err(AppError::Validation("cube JOIN requires ON".into()));
            }
        },
        _ => return Err(AppError::Validation("cube JOIN requires ON".into())),
    };

    let rewritten = rewrite_join_on_expr(docs, sides, expr, role, binds, on_dims_per_side)?;
    Ok(format!("({rewritten})"))
}

pub(crate) fn rewrite_join_on_expr(
    docs: &[Value],
    sides: &[(String, String)],
    expr: &Expr,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    on_dims_per_side: &mut HashMap<String, Vec<String>>,
) -> Result<String, AppError> {
    match expr {
        Expr::Nested(inner) => {
            let s = rewrite_join_on_expr(docs, sides, inner, role, binds, on_dims_per_side)?;
            Ok(format!("({s})"))
        }
        Expr::BinaryOp { left, op, right } => match op {
            BinaryOperator::And | BinaryOperator::Or => {
                let l = rewrite_join_on_expr(docs, sides, left, role, binds, on_dims_per_side)?;
                let r = rewrite_join_on_expr(docs, sides, right, role, binds, on_dims_per_side)?;
                Ok(format!("{l} {op} {r}"))
            }
            BinaryOperator::Eq
            | BinaryOperator::NotEq
            | BinaryOperator::Lt
            | BinaryOperator::LtEq
            | BinaryOperator::Gt
            | BinaryOperator::GtEq => {
                let l = rewrite_join_on_operand(docs, sides, left, role, binds, on_dims_per_side)?;
                let r = rewrite_join_on_operand(docs, sides, right, role, binds, on_dims_per_side)?;
                Ok(format!("{l} {op} {r}"))
            }
            other => Err(AppError::Validation(format!(
                "operator {other} not allowed in cube JOIN ON"
            ))),
        },
        other => Err(AppError::Validation(format!(
            "unsupported expression in cube JOIN ON: {other}"
        ))),
    }
}

pub(crate) fn rewrite_join_on_operand(
    docs: &[Value],
    sides: &[(String, String)],
    expr: &Expr,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    on_dims_per_side: &mut HashMap<String, Vec<String>>,
) -> Result<String, AppError> {
    match expr {
        Expr::Identifier(id) => {
            let (side_alias, cube_name) = resolve_member_side(docs, sides, None, &id.value)?;
            validate_join_member(docs, &cube_name, &id.value, role)?;
            record_on_dim(docs, &cube_name, &side_alias, &id.value, on_dims_per_side)?;
            Ok(format!(
                "{}.{}",
                quote_ident(&side_alias)?,
                quote_ident(&id.value)?
            ))
        }
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let qual = &parts[0].value;
            let mem = &parts[1].value;
            let (side_alias, cube_name) = resolve_member_side(docs, sides, Some(qual), mem)?;
            if !member_on_cube(docs, &cube_name, mem)? {
                return Err(AppError::Validation(unknown_member_message(&format!(
                    "unknown semantic member {mem} on cube {cube_name}"
                ))));
            }
            validate_join_member(docs, &cube_name, mem, role)?;
            record_on_dim(docs, &cube_name, &side_alias, mem, on_dims_per_side)?;
            Ok(format!(
                "{}.{}",
                quote_ident(&side_alias)?,
                quote_ident(mem)?
            ))
        }
        Expr::Value(_) => Ok(apply_binds(&expr.to_string(), binds)),
        other => Err(AppError::Validation(format!(
            "unsupported operand in cube JOIN ON: {other}"
        ))),
    }
}

pub(crate) fn validate_join_member(
    docs: &[Value],
    cube_name: &str,
    member: &str,
    role: Option<&str>,
) -> Result<(), AppError> {
    let cube = find_doc(docs, "cube", cube_name)?;
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if item.get("name").and_then(|v| v.as_str()) == Some(member) {
                    check_role_access(item.get("role"), role)?;
                    if item
                        .get("encryption")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        return Err(AppError::Validation(format!(
                            "encrypted member {member} cannot be used in JOIN ON"
                        )));
                    }
                    return Ok(());
                }
            }
        }
    }
    if let Ok(m_doc) = find_doc(docs, "metric", member) {
        if m_doc.get("cube").and_then(|v| v.as_str()) == Some(cube_name) {
            return check_role_access(m_doc.get("role"), role);
        }
    }
    Err(AppError::Validation(format!(
        "unknown member {member} on cube {cube_name}"
    )))
}

pub(crate) fn record_on_dim(
    docs: &[Value],
    cube_name: &str,
    side_alias: &str,
    member: &str,
    on_dims_per_side: &mut HashMap<String, Vec<String>>,
) -> Result<(), AppError> {
    let cube = find_doc(docs, "cube", cube_name)?;
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            if items
                .iter()
                .any(|d| d.get("name").and_then(|v| v.as_str()) == Some(member))
            {
                let entry = on_dims_per_side.entry(side_alias.to_string()).or_default();
                if !entry.iter().any(|x| x == member) {
                    entry.push(member.to_string());
                }
                return Ok(());
            }
        }
    }
    Ok(())
}
