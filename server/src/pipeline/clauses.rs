//! SELECT projection classification, WHERE segment expand, GROUP BY/HAVING/ORDER BY/LIMIT extract.
use std::collections::{HashMap, HashSet};

use serde_json::{json, Value};
use sqlparser::ast::{
    BinaryOperator, Expr, GroupByExpr, Ident, LimitClause, OrderByExpr, OrderByKind, Query,
    Select, SelectItem, SetExpr, Statement, Value as SqlValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::compile::{check_role_access, CubeProjection, quote_ident};
use crate::error::AppError;
use crate::expr_guard::{
    assert_adhoc_agg_expr, assert_idents_known, assert_scalar_expr,
    contains_aggregate_sql, contains_window_sql, extract_idents,
};

use super::find_doc;

#[derive(Debug, Clone, Default)]
pub(crate) struct ClassifiedProjection {
    pub(crate) projections: Vec<CubeProjection>,
    pub(crate) named_metrics: Vec<String>,
    /// (internal metric name `__adhoc_N`, SQL expression, output alias)
    pub(crate) adhoc_metrics: Vec<(String, String, String)>,
    pub(crate) dims: Vec<String>,
    /// (0-based index in `select.projection`, output alias) claimed by this cube side.
    pub(crate) attributed_items: Vec<(usize, String)>,
}

pub(crate) fn ensure_projection_mode(c: &ClassifiedProjection) -> Result<(), AppError> {
    let named = !c.named_metrics.is_empty();
    let adhoc = !c.adhoc_metrics.is_empty();
    let members = !c.dims.is_empty();
    if !named && !adhoc && !members && c.projections.is_empty() {
        return Err(AppError::Validation(
            "semantic SELECT must project a named metric, ad-hoc aggregate, or dimension/fact (D23)".into(),
        ));
    }
    Ok(())
}

/// Build ephemeral metric docs for ad-hoc aggregates; return owned docs + all metric names (named + ad-hoc).
pub(crate) fn materialize_adhoc_metrics(
    docs: &[Value],
    cube_name: &str,
    classified: &ClassifiedProjection,
    dims: &[String],
    explicit_group_by: &Option<Vec<String>>,
) -> Result<(Option<Vec<Value>>, Vec<String>), AppError> {
    if classified.adhoc_metrics.is_empty() {
        return Ok((None, classified.named_metrics.clone()));
    }

    let cube = find_doc(docs, "cube", cube_name)?;
    let mut known = HashSet::new();
    let mut all_dim_names = Vec::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    known.insert(n.to_string());
                    all_dim_names.push(n.to_string());
                }
            }
        }
    }

    let mut allowed_gb = all_dim_names;
    for d in dims {
        if !allowed_gb.iter().any(|x| x == d) {
            allowed_gb.push(d.clone());
        }
    }
    if let Some(gb) = explicit_group_by {
        for d in gb {
            if !allowed_gb.iter().any(|x| x == d) {
                allowed_gb.push(d.clone());
            }
        }
    }

    let mut owned = docs.to_vec();
    let mut metric_names = classified.named_metrics.clone();
    for (internal, expr, _alias) in &classified.adhoc_metrics {
        assert_adhoc_agg_expr(expr)?;
        assert_idents_known(expr, &known)?;
        owned.push(json!({
            "kind": "metric",
            "name": internal,
            "description": "ad-hoc aggregate (D23)",
            "cube": cube_name,
            "expression": expr,
            "aggregation": "group",
            "group_by": allowed_gb,
            "filter": null,
            "role": "all"
        }));
        metric_names.push(internal.clone());
    }
    Ok((Some(owned), metric_names))
}

pub(crate) fn try_projection_member(
    item: &SelectItem,
) -> Result<Option<(Option<String>, String, String)>, AppError> {
    match item {
        SelectItem::UnnamedExpr(Expr::Identifier(id)) => {
            Ok(Some((None, id.value.clone(), id.value.clone())))
        }
        SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) if parts.len() == 2 => Ok(Some((
            Some(parts[0].value.clone()),
            parts[1].value.clone(),
            parts[1].value.clone(),
        ))),
        SelectItem::ExprWithAlias {
            expr: Expr::Identifier(id),
            alias,
        } => Ok(Some((None, id.value.clone(), alias.value.clone()))),
        SelectItem::ExprWithAlias {
            expr: Expr::CompoundIdentifier(parts),
            alias,
        } if parts.len() == 2 => Ok(Some((
            Some(parts[0].value.clone()),
            parts[1].value.clone(),
            alias.value.clone(),
        ))),
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => Err(
            AppError::Validation("SELECT * not allowed in semantic SQL".into()),
        ),
        _ => Ok(None),
    }
}

pub(crate) fn sanitize_adhoc_alias(expr: &str) -> String {
    let mut s: String = expr
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    s = s.trim_matches('_').to_string();
    if s.is_empty() {
        return "adhoc".into();
    }
    if s.chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit())
    {
        s = format!("m_{s}");
    }
    s
}

pub(crate) fn resolve_member_side(
    docs: &[Value],
    sides: &[(String, String)],
    qual: Option<&str>,
    member: &str,
) -> Result<(String, String), AppError> {
    if let Some(q) = qual {
        for (alias, cube) in sides {
            if alias == q || cube == q {
                if !member_on_cube(docs, cube, member)? {
                    return Err(AppError::Validation(format!(
                        "unknown semantic member: {q}.{member}"
                    )));
                }
                return Ok((alias.clone(), cube.clone()));
            }
        }
        return Err(AppError::Validation(format!(
            "unknown cube qualifier {q} for member {member}"
        )));
    }
    let mut hits = Vec::new();
    for (alias, cube) in sides {
        if member_on_cube(docs, cube, member)? {
            hits.push((alias.clone(), cube.clone()));
        }
    }
    match hits.len() {
        1 => Ok(hits.remove(0)),
        0 => Err(AppError::Validation(format!(
            "unknown semantic member: {member}"
        ))),
        _ => Err(AppError::Validation(format!(
            "ambiguous member {member}; qualify with cube alias"
        ))),
    }
}

pub(crate) fn member_on_cube(docs: &[Value], cube_name: &str, member: &str) -> Result<bool, AppError> {
    if docs.iter().any(|d| {
        d.get("kind").and_then(|v| v.as_str()) == Some("metric")
            && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
            && d.get("name").and_then(|v| v.as_str()) == Some(member)
    }) {
        return Ok(true);
    }
    let cube = find_doc(docs, "cube", cube_name)?;
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if item.get("name").and_then(|v| v.as_str()) == Some(member) {
                    return Ok(true);
                }
            }
        }
    }
    Ok(false)
}

/// Attribute a complex expression in Cube JOIN to `side_alias` (`cube_name`) and strip qualifiers.
/// - If qualified, all qualifiers must belong to `side_alias` / `cube_name`.
/// - If unqualified and has identifiers, must uniquely resolve to `side_alias`.
/// - If unqualified with 0 identifiers (e.g. `COUNT(*)`), attributed to `sides[0]`.
pub(crate) fn complex_expr_for_join_side(
    raw_expr: &Expr,
    docs: &[Value],
    cube_name: &str,
    side_alias: &str,
    sides: &[(String, String)],
    known_on_this_side: &HashSet<String>,
) -> Result<Option<Expr>, AppError> {
    let mut expr = raw_expr.clone();
    let mut foreign = false;
    let mut saw_side = false;
    strip_adhoc_join_qualifiers(
        &mut expr,
        cube_name,
        side_alias,
        known_on_this_side,
        &mut foreign,
        &mut saw_side,
    )?;
    if foreign {
        return Ok(None);
    }
    if saw_side {
        let idents = extract_idents(&expr.to_string())?;
        if idents.iter().any(|id| !known_on_this_side.contains(id)) {
            return Ok(None);
        }
        return Ok(Some(expr));
    }

    // Unqualified expression in Cube JOIN:
    let idents = extract_idents(&expr.to_string())?;
    if idents.is_empty() {
        // e.g. COUNT(*) — attribute to the primary (leftmost) cube side.
        if sides.first().map(|(a, _)| a.as_str()) == Some(side_alias) {
            return Ok(Some(expr));
        }
        return Ok(None);
    }

    let mut matching_sides = Vec::new();
    for (al, cname) in sides {
        let mut all_match = true;
        for id in &idents {
            if !member_on_cube(docs, cname, id)? {
                all_match = false;
                break;
            }
        }
        if all_match {
            matching_sides.push(al.as_str());
        }
    }
    match matching_sides.len() {
        0 => Err(AppError::Validation(format!(
            "unknown semantic member in expression: {raw_expr}"
        ))),
        1 => {
            if matching_sides[0] == side_alias {
                Ok(Some(expr))
            } else {
                Ok(None)
            }
        }
        _ => Err(AppError::Validation(format!(
            "ambiguous unqualified expression {raw_expr} in cube JOIN; qualify with cube alias"
        ))),
    }
}

pub(crate) fn strip_adhoc_join_qualifiers(
    expr: &mut Expr,
    cube_name: &str,
    side_alias: &str,
    member_names: &HashSet<String>,
    foreign: &mut bool,
    saw_side: &mut bool,
) -> Result<(), AppError> {
    match expr {
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = parts[0].value.clone();
            let n = parts[1].value.clone();
            if q == cube_name || q == side_alias {
                if !member_names.contains(&n) {
                    return Err(AppError::Validation(format!(
                        "unknown member {n} on cube {cube_name}"
                    )));
                }
                *saw_side = true;
                *expr = Expr::Identifier(Ident::new(n));
            } else {
                *foreign = true;
            }
            Ok(())
        }
        Expr::Identifier(_) => Ok(()),
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::Cast { expr: inner, .. } => strip_adhoc_join_qualifiers(
            inner,
            cube_name,
            side_alias,
            member_names,
            foreign,
            saw_side,
        ),
        Expr::BinaryOp { left, right, .. } => {
            strip_adhoc_join_qualifiers(
                left,
                cube_name,
                side_alias,
                member_names,
                foreign,
                saw_side,
            )?;
            strip_adhoc_join_qualifiers(
                right,
                cube_name,
                side_alias,
                member_names,
                foreign,
                saw_side,
            )
        }
        Expr::Function(func) => {
            if let sqlparser::ast::FunctionArguments::List(list) = &mut func.args {
                for arg in &mut list.args {
                    match arg {
                        sqlparser::ast::FunctionArg::Unnamed(
                            sqlparser::ast::FunctionArgExpr::Expr(e),
                        )
                        | sqlparser::ast::FunctionArg::Named {
                            arg: sqlparser::ast::FunctionArgExpr::Expr(e),
                            ..
                        } => {
                            strip_adhoc_join_qualifiers(
                                e,
                                cube_name,
                                side_alias,
                                member_names,
                                foreign,
                                saw_side,
                            )?;
                        }
                        _ => {}
                    }
                }
            }
            if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &mut func.over {
                for e in &mut spec.partition_by {
                    strip_adhoc_join_qualifiers(
                        e,
                        cube_name,
                        side_alias,
                        member_names,
                        foreign,
                        saw_side,
                    )?;
                }
                for ob in &mut spec.order_by {
                    strip_adhoc_join_qualifiers(
                        &mut ob.expr,
                        cube_name,
                        side_alias,
                        member_names,
                        foreign,
                        saw_side,
                    )?;
                }
            }
            Ok(())
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand.as_mut() {
                strip_adhoc_join_qualifiers(
                    op,
                    cube_name,
                    side_alias,
                    member_names,
                    foreign,
                    saw_side,
                )?;
            }
            for cw in conditions.iter_mut() {
                strip_adhoc_join_qualifiers(
                    &mut cw.condition,
                    cube_name,
                    side_alias,
                    member_names,
                    foreign,
                    saw_side,
                )?;
                strip_adhoc_join_qualifiers(
                    &mut cw.result,
                    cube_name,
                    side_alias,
                    member_names,
                    foreign,
                    saw_side,
                )?;
            }
            if let Some(el) = else_result.as_mut() {
                strip_adhoc_join_qualifiers(
                    el,
                    cube_name,
                    side_alias,
                    member_names,
                    foreign,
                    saw_side,
                )?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Replace aggregate function calls (`sum/count/avg/min/max`) inside a hybrid expression
/// with ephemeral metric identifiers `__adhoc_N`.
pub(crate) fn replace_agg_calls_with_adhoc(
    expr: &mut Expr,
    member_names: &HashSet<String>,
    adhoc_idx: &mut usize,
    adhoc_metrics: &mut Vec<(String, String, String)>,
) -> Result<(), AppError> {
    match expr {
        Expr::Function(func) => {
            let fname = func.name.to_string().to_ascii_lowercase();
            if func.over.is_none()
                && matches!(fname.as_str(), "sum" | "count" | "avg" | "min" | "max")
            {
                let func_sql = expr.to_string();
                assert_adhoc_agg_expr(&func_sql)?;
                assert_idents_known(&func_sql, member_names)?;
                let internal = format!("__adhoc_{}", *adhoc_idx);
                *adhoc_idx += 1;
                adhoc_metrics.push((internal.clone(), func_sql, internal.clone()));
                *expr = Expr::Identifier(Ident::new(internal));
                return Ok(());
            }
            if let sqlparser::ast::FunctionArguments::List(list) = &mut func.args {
                for arg in &mut list.args {
                    match arg {
                        sqlparser::ast::FunctionArg::Unnamed(
                            sqlparser::ast::FunctionArgExpr::Expr(e),
                        )
                        | sqlparser::ast::FunctionArg::Named {
                            arg: sqlparser::ast::FunctionArgExpr::Expr(e),
                            ..
                        } => {
                            replace_agg_calls_with_adhoc(e, member_names, adhoc_idx, adhoc_metrics)?;
                        }
                        _ => {}
                    }
                }
            }
            if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &mut func.over {
                for e in &mut spec.partition_by {
                    replace_agg_calls_with_adhoc(e, member_names, adhoc_idx, adhoc_metrics)?;
                }
                for ob in &mut spec.order_by {
                    replace_agg_calls_with_adhoc(&mut ob.expr, member_names, adhoc_idx, adhoc_metrics)?;
                }
            }
            Ok(())
        }
        Expr::BinaryOp { left, right, .. } => {
            replace_agg_calls_with_adhoc(left, member_names, adhoc_idx, adhoc_metrics)?;
            replace_agg_calls_with_adhoc(right, member_names, adhoc_idx, adhoc_metrics)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::Cast { expr: inner, .. } => {
            replace_agg_calls_with_adhoc(inner, member_names, adhoc_idx, adhoc_metrics)
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand.as_mut() {
                replace_agg_calls_with_adhoc(op, member_names, adhoc_idx, adhoc_metrics)?;
            }
            for cw in conditions.iter_mut() {
                replace_agg_calls_with_adhoc(
                    &mut cw.condition,
                    member_names,
                    adhoc_idx,
                    adhoc_metrics,
                )?;
                replace_agg_calls_with_adhoc(
                    &mut cw.result,
                    member_names,
                    adhoc_idx,
                    adhoc_metrics,
                )?;
            }
            if let Some(el) = else_result.as_mut() {
                replace_agg_calls_with_adhoc(el, member_names, adhoc_idx, adhoc_metrics)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}


pub(crate) fn classify_projection_for_cube(
    select: &Select,
    docs: &[Value],
    cube_name: &str,
    only_alias: Option<&str>,
    is_single_cube: bool,
    join_sides: Option<&[(String, String)]>,
) -> Result<ClassifiedProjection, AppError> {
    // D24: registered metrics are examples only — not SQL identifiers.
    let metric_names: HashSet<String> = HashSet::new();
    let cube = find_doc(docs, "cube", cube_name)?;
    let mut dim_names = HashSet::new();
    let mut member_names = HashSet::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    member_names.insert(n.to_string());
                    dim_names.insert(n.to_string());
                }
            }
        }
    }
    let known_on_side = member_names.clone();

    let mut out = ClassifiedProjection::default();
    let mut adhoc_idx = 0usize;
    let empty_seg = HashMap::new();

    for (idx, item) in select.projection.iter().enumerate() {
        if let Some((qual, member, out_alias)) = try_projection_member(item)? {
            if let Some(only) = only_alias {
                match &qual {
                    Some(q) if q != only && q != cube_name => {
                        if is_single_cube {
                            return Err(AppError::Validation(format!(
                                "qualifier {q} does not match FROM cube {cube_name}"
                            )));
                        }
                        if let Some(sides) = join_sides {
                            if !sides.iter().any(|(a, c)| a == q || c == q) {
                                return Err(AppError::Validation(format!(
                                    "unknown cube qualifier {q} for member {member}"
                                )));
                            }
                        }
                        continue;
                    }
                    None if !is_single_cube => {
                        if let Some(sides) = join_sides {
                            let (owner_alias, _) =
                                resolve_member_side(docs, sides, None, &member)?;
                            if owner_alias != only {
                                continue;
                            }
                        } else if !metric_names.contains(&member) && !dim_names.contains(&member) {
                            continue;
                        }
                    }
                    _ => {}
                }
            }
            if metric_names.contains(&member) {
                if !out.named_metrics.iter().any(|m| m == &member) {
                    out.named_metrics.push(member.clone());
                }
                out.projections.push(CubeProjection {
                    member,
                    alias: out_alias.clone(),
                });
                out.attributed_items.push((idx, out_alias));
            } else if dim_names.contains(&member) {
                if !out.dims.iter().any(|d| d == &member) {
                    out.dims.push(member.clone());
                }
                out.projections.push(CubeProjection {
                    member,
                    alias: out_alias.clone(),
                });
                out.attributed_items.push((idx, out_alias));
            } else if is_single_cube || qual.is_some() {
                return Err(AppError::Validation(format!(
                    "unknown semantic member: {member}"
                )));
            }
            continue;
        }

        let (raw_expr, alias_opt) = match item {
            SelectItem::UnnamedExpr(e) => (e, None),
            SelectItem::ExprWithAlias { expr, alias } => (expr, Some(alias.value.clone())),
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                return Err(AppError::Validation(
                    "SELECT * not allowed in semantic SQL".into(),
                ));
            }
        };

        let mut stripped_expr = if is_single_cube {
            expand_segments_and_strip_qualifier(
                raw_expr,
                &empty_seg,
                cube_name,
                only_alias,
                true,
                None,
            )?
        } else {
            let Some(alias) = only_alias else {
                continue;
            };
            match complex_expr_for_join_side(
                raw_expr,
                docs,
                cube_name,
                alias,
                join_sides.unwrap_or(&[]),
                &known_on_side,
            )? {
                Some(e) => e,
                None => continue,
            }
        };

        let stripped_sql = stripped_expr.to_string();
        let out_alias = alias_opt.unwrap_or_else(|| sanitize_adhoc_alias(&stripped_sql));
        let ids = extract_idents(&stripped_sql)?;
        let refs_named_metric = ids.iter().any(|id| metric_names.contains(id));
        let has_agg = contains_aggregate_sql(&stripped_sql)?;
        let has_window = contains_window_sql(&stripped_sql)?;
        let has_subquery = expr_contains_subquery(&stripped_expr);

        if !refs_named_metric && has_agg {
            // Pure ad-hoc aggregate expression (e.g., sum(sale_amount), round(avg(x), 2), RANK() OVER (ORDER BY sum(sale_amount))).
            assert_adhoc_agg_expr(&stripped_sql)?;
            assert_idents_known(&stripped_sql, &member_names)?;
            let internal = format!("__adhoc_{adhoc_idx}");
            adhoc_idx += 1;
            out.adhoc_metrics
                .push((internal.clone(), stripped_sql, out_alias.clone()));
            out.projections.push(CubeProjection {
                member: internal,
                alias: out_alias.clone(),
            });
            out.attributed_items.push((idx, out_alias));
        } else if refs_named_metric && !has_agg {
            // Derived expression over named metric(s) (e.g., round(revenue, 2), RANK() OVER (PARTITION BY region_code ORDER BY revenue DESC)).
            assert_scalar_expr(&stripped_sql)?;
            if has_window {
                assert_idents_known(&stripped_sql, &known_on_side)?;
            } else {
                assert_idents_known(&stripped_sql, &metric_names)?;
            }
            for id in ids {
                if metric_names.contains(&id) && !out.named_metrics.iter().any(|m| m == &id) {
                    out.named_metrics.push(id);
                }
            }
            out.projections.push(CubeProjection {
                member: stripped_sql,
                alias: out_alias.clone(),
            });
            out.attributed_items.push((idx, out_alias));
        } else if refs_named_metric && has_agg {
            // Hybrid expression combining named metric(s) and ad-hoc aggregate(s) (e.g., revenue / count(*)).
            replace_agg_calls_with_adhoc(
                &mut stripped_expr,
                &member_names,
                &mut adhoc_idx,
                &mut out.adhoc_metrics,
            )?;
            let composite_sql = stripped_expr.to_string();
            assert_scalar_expr(&composite_sql)?;
            for id in extract_idents(&composite_sql)? {
                if metric_names.contains(&id) {
                    if !out.named_metrics.iter().any(|m| m == &id) {
                        out.named_metrics.push(id);
                    }
                } else if !id.starts_with("__adhoc_") && !(has_window && member_names.contains(&id))
                {
                    return Err(AppError::Validation(format!(
                        "cannot mix unaggregated member {id} with metric in expression: {stripped_sql}"
                    )));
                }
            }
            out.projections.push(CubeProjection {
                member: composite_sql,
                alias: out_alias.clone(),
            });
            out.attributed_items.push((idx, out_alias));
        } else {
            // Non-aggregate scalar, window, or subquery expression over cube dimensions/facts.
            if ids.is_empty() && !has_window && !has_subquery {
                return Err(AppError::Validation(
                    "semantic SELECT projection must reference a cube member".into(),
                ));
            }
            assert_scalar_expr(&stripped_sql)?;
            if !ids.is_empty() {
                assert_idents_known(&stripped_sql, &member_names)?;
            }
            if !has_window && !has_subquery && !out.dims.iter().any(|d| d == &stripped_sql) {
                out.dims.push(stripped_sql.clone());
            }
            out.projections.push(CubeProjection {
                member: stripped_sql,
                alias: out_alias.clone(),
            });
            out.attributed_items.push((idx, out_alias));
        }
    }
    Ok(out)
}

fn expr_contains_subquery(expr: &Expr) -> bool {
    use sqlparser::ast::{Visit, Visitor};
    struct Det;
    impl Visitor for Det {
        type Break = ();
        fn pre_visit_query(&mut self, _q: &Query) -> std::ops::ControlFlow<Self::Break> {
            std::ops::ControlFlow::Break(())
        }
    }
    expr.visit(&mut Det).is_break()
}

pub(crate) fn extract_where_for_cube(
    select: &Select,
    docs: &[Value],
    cube_name: &str,
    only_alias: Option<&str>,
    is_single_cube: bool,
    role: Option<&str>,
) -> Result<(Vec<String>, Vec<String>), AppError> {
    let Some(selection) = &select.selection else {
        return Ok((Vec::new(), Vec::new()));
    };
    // D24: segments are examples only — not WHERE identifiers.
    let empty_seg: HashMap<String, &Value> = HashMap::new();

    let mut conjuncts = Vec::new();
    split_and_conjuncts(selection, &mut conjuncts);

    let mut where_exprs = Vec::new();

    for conj in conjuncts {
        if !is_single_cube && !expr_belongs_to_cube(conj, docs, cube_name, only_alias)? {
            continue;
        }
        let rewritten = expand_segments_and_strip_qualifier(
            conj,
            &empty_seg,
            cube_name,
            only_alias,
            is_single_cube,
            role,
        )?;
        where_exprs.push(rewritten.to_string());
    }

    Ok((Vec::new(), where_exprs))
}

pub(crate) fn split_and_conjuncts<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
    match expr {
        Expr::BinaryOp {
            left,
            op: BinaryOperator::And,
            right,
        } => {
            split_and_conjuncts(left, out);
            split_and_conjuncts(right, out);
        }
        Expr::Nested(inner) => split_and_conjuncts(inner, out),
        other => out.push(other),
    }
}

pub(crate) fn match_segment_ident(
    expr: &Expr,
    seg_map: &HashMap<String, &Value>,
    only_alias: Option<&str>,
    cube_name: &str,
) -> Option<String> {
    match expr {
        Expr::Identifier(id) if seg_map.contains_key(&id.value) => Some(id.value.clone()),
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = &parts[0].value;
            let n = &parts[1].value;
            let ok_qual = match only_alias {
                Some(a) => q == a || q == cube_name,
                None => q == cube_name,
            };
            if ok_qual && seg_map.contains_key(n) {
                Some(n.clone())
            } else {
                None
            }
        }
        Expr::Nested(inner) => match_segment_ident(inner, seg_map, only_alias, cube_name),
        _ => None,
    }
}

pub(crate) fn expr_belongs_to_cube(
    expr: &Expr,
    docs: &[Value],
    cube_name: &str,
    only_alias: Option<&str>,
) -> Result<bool, AppError> {
    let mut quals = Vec::new();
    let mut bare = Vec::new();
    collect_expr_quals_and_idents(expr, &mut quals, &mut bare);
    if !quals.is_empty() {
        return Ok(quals
            .iter()
            .all(|q| Some(q.as_str()) == only_alias || q == cube_name));
    }
    let cube = find_doc(docs, "cube", cube_name)?;
    let mut known = HashSet::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    known.insert(n.to_string());
                }
            }
        }
    }
    Ok(!bare.is_empty() && bare.iter().all(|b| known.contains(b)))
}

pub(crate) fn collect_expr_quals_and_idents(expr: &Expr, quals: &mut Vec<String>, bare: &mut Vec<String>) {
    match expr {
        Expr::Identifier(id) => bare.push(id.value.clone()),
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            quals.push(parts[0].value.clone());
            bare.push(parts[1].value.clone());
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_expr_quals_and_idents(left, quals, bare);
            collect_expr_quals_and_idents(right, quals, bare);
        }
        Expr::UnaryOp { expr, .. }
        | Expr::Nested(expr)
        | Expr::IsNull(expr)
        | Expr::IsNotNull(expr)
        | Expr::Cast { expr, .. } => {
            collect_expr_quals_and_idents(expr, quals, bare);
        }
        Expr::Between {
            expr, low, high, ..
        } => {
            collect_expr_quals_and_idents(expr, quals, bare);
            collect_expr_quals_and_idents(low, quals, bare);
            collect_expr_quals_and_idents(high, quals, bare);
        }
        Expr::InList { expr, list, .. } => {
            collect_expr_quals_and_idents(expr, quals, bare);
            for item in list {
                collect_expr_quals_and_idents(item, quals, bare);
            }
        }
        Expr::Like { expr, pattern, .. } | Expr::ILike { expr, pattern, .. } => {
            collect_expr_quals_and_idents(expr, quals, bare);
            collect_expr_quals_and_idents(pattern, quals, bare);
        }
        Expr::Function(func) => {
            if let sqlparser::ast::FunctionArguments::List(list) = &func.args {
                for arg in &list.args {
                    match arg {
                        sqlparser::ast::FunctionArg::Unnamed(
                            sqlparser::ast::FunctionArgExpr::Expr(e),
                        )
                        | sqlparser::ast::FunctionArg::Named {
                            arg: sqlparser::ast::FunctionArgExpr::Expr(e),
                            ..
                        } => {
                            collect_expr_quals_and_idents(e, quals, bare);
                        }
                        _ => {}
                    }
                }
            }
            if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &func.over {
                for e in &spec.partition_by {
                    collect_expr_quals_and_idents(e, quals, bare);
                }
                for ob in &spec.order_by {
                    collect_expr_quals_and_idents(&ob.expr, quals, bare);
                }
            }
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand {
                collect_expr_quals_and_idents(op, quals, bare);
            }
            for cw in conditions {
                collect_expr_quals_and_idents(&cw.condition, quals, bare);
                collect_expr_quals_and_idents(&cw.result, quals, bare);
            }
            if let Some(el) = else_result {
                collect_expr_quals_and_idents(el, quals, bare);
            }
        }
        _ => {}
    }
}

pub(crate) fn expand_segments_and_strip_qualifier(
    expr: &Expr,
    seg_map: &HashMap<String, &Value>,
    cube_name: &str,
    only_alias: Option<&str>,
    is_single_cube: bool,
    role: Option<&str>,
) -> Result<Expr, AppError> {
    let mut cloned = expr.clone();
    rewrite_where_ast(
        &mut cloned,
        seg_map,
        cube_name,
        only_alias,
        is_single_cube,
        role,
    )?;
    Ok(cloned)
}

pub(crate) fn rewrite_where_ast(
    expr: &mut Expr,
    seg_map: &HashMap<String, &Value>,
    cube_name: &str,
    only_alias: Option<&str>,
    is_single_cube: bool,
    role: Option<&str>,
) -> Result<(), AppError> {
    match expr {
        Expr::Identifier(id) => {
            if let Some(seg) = seg_map.get(&id.value) {
                check_role_access(seg.get("role"), role)?;
                let f = seg
                    .get("filter")
                    .and_then(|v| v.get("expression"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| AppError::Validation("segment.filter.expression".into()))?;
                *expr = parse_cond_fragment(f)?;
            }
            Ok(())
        }
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = &parts[0].value;
            let n = &parts[1].value;
            let ok_qual = match only_alias {
                Some(a) => q == a || q == cube_name,
                None => q == cube_name,
            };
            if !ok_qual && is_single_cube {
                return Err(AppError::Validation(format!(
                    "qualifier {q} does not match cube {cube_name}"
                )));
            }
            if ok_qual {
                if let Some(seg) = seg_map.get(n) {
                    check_role_access(seg.get("role"), role)?;
                    let f = seg
                        .get("filter")
                        .and_then(|v| v.get("expression"))
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| AppError::Validation("segment.filter.expression".into()))?;
                    *expr = parse_cond_fragment(f)?;
                } else {
                    *expr = Expr::Identifier(Ident::new(n.clone()));
                }
            }
            Ok(())
        }
        Expr::BinaryOp { left, right, .. } => {
            rewrite_where_ast(left, seg_map, cube_name, only_alias, is_single_cube, role)?;
            rewrite_where_ast(right, seg_map, cube_name, only_alias, is_single_cube, role)
        }
        Expr::UnaryOp { expr: inner, .. }
        | Expr::Nested(inner)
        | Expr::IsNull(inner)
        | Expr::IsNotNull(inner)
        | Expr::Cast { expr: inner, .. } => {
            rewrite_where_ast(inner, seg_map, cube_name, only_alias, is_single_cube, role)
        }
        Expr::Between {
            expr: e, low, high, ..
        } => {
            rewrite_where_ast(e, seg_map, cube_name, only_alias, is_single_cube, role)?;
            rewrite_where_ast(low, seg_map, cube_name, only_alias, is_single_cube, role)?;
            rewrite_where_ast(high, seg_map, cube_name, only_alias, is_single_cube, role)
        }
        Expr::InList { expr: e, list, .. } => {
            rewrite_where_ast(e, seg_map, cube_name, only_alias, is_single_cube, role)?;
            for item in list {
                rewrite_where_ast(item, seg_map, cube_name, only_alias, is_single_cube, role)?;
            }
            Ok(())
        }
        Expr::Like {
            expr: e, pattern, ..
        }
        | Expr::ILike {
            expr: e, pattern, ..
        } => {
            rewrite_where_ast(e, seg_map, cube_name, only_alias, is_single_cube, role)?;
            rewrite_where_ast(pattern, seg_map, cube_name, only_alias, is_single_cube, role)
        }
        Expr::Function(func) => {
            if let sqlparser::ast::FunctionArguments::List(list) = &mut func.args {
                for arg in &mut list.args {
                    match arg {
                        sqlparser::ast::FunctionArg::Unnamed(
                            sqlparser::ast::FunctionArgExpr::Expr(e),
                        )
                        | sqlparser::ast::FunctionArg::Named {
                            arg: sqlparser::ast::FunctionArgExpr::Expr(e),
                            ..
                        } => {
                            rewrite_where_ast(
                                e,
                                seg_map,
                                cube_name,
                                only_alias,
                                is_single_cube,
                                role,
                            )?;
                        }
                        _ => {}
                    }
                }
            }
            if let Some(sqlparser::ast::WindowType::WindowSpec(spec)) = &mut func.over {
                for e in &mut spec.partition_by {
                    rewrite_where_ast(
                        e,
                        seg_map,
                        cube_name,
                        only_alias,
                        is_single_cube,
                        role,
                    )?;
                }
                for ob in &mut spec.order_by {
                    rewrite_where_ast(
                        &mut ob.expr,
                        seg_map,
                        cube_name,
                        only_alias,
                        is_single_cube,
                        role,
                    )?;
                }
            }
            Ok(())
        }
        Expr::Case {
            operand,
            conditions,
            else_result,
            ..
        } => {
            if let Some(op) = operand.as_mut() {
                rewrite_where_ast(op, seg_map, cube_name, only_alias, is_single_cube, role)?;
            }
            for cw in conditions.iter_mut() {
                rewrite_where_ast(
                    &mut cw.condition,
                    seg_map,
                    cube_name,
                    only_alias,
                    is_single_cube,
                    role,
                )?;
                rewrite_where_ast(
                    &mut cw.result,
                    seg_map,
                    cube_name,
                    only_alias,
                    is_single_cube,
                    role,
                )?;
            }
            if let Some(el) = else_result.as_mut() {
                rewrite_where_ast(el, seg_map, cube_name, only_alias, is_single_cube, role)?;
            }
            Ok(())
        }
        Expr::Substring {
            expr: inner,
            substring_from,
            substring_for,
            ..
        } => {
            rewrite_where_ast(inner, seg_map, cube_name, only_alias, is_single_cube, role)?;
            if let Some(f) = substring_from.as_mut() {
                rewrite_where_ast(f, seg_map, cube_name, only_alias, is_single_cube, role)?;
            }
            if let Some(l) = substring_for.as_mut() {
                rewrite_where_ast(l, seg_map, cube_name, only_alias, is_single_cube, role)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn parse_cond_fragment(sql: &str) -> Result<Expr, AppError> {
    let wrapped = format!("SELECT 1 WHERE ({sql})");
    let dialect = GenericDialect {};
    let stmts = Parser::parse_sql(&dialect, &wrapped)
        .map_err(|e| AppError::Validation(format!("segment filter parse: {e}")))?;
    let Some(Statement::Query(q)) = stmts.into_iter().next() else {
        return Err(AppError::Validation("segment filter query".into()));
    };
    let SetExpr::Select(sel) = *q.body else {
        return Err(AppError::Validation("segment filter select".into()));
    };
    sel.selection
        .ok_or_else(|| AppError::Validation("segment filter empty".into()))
}

pub(crate) fn extract_group_by_for_cube(
    select: &Select,
    cube_name: &str,
    cube_alias: &str,
    projections: &[CubeProjection],
    dim_exprs: &[String],
) -> Result<(Option<Vec<String>>, Option<String>), AppError> {
    let GroupByExpr::Expressions(exprs, _) = &select.group_by else {
        return Ok((None, None));
    };
    if exprs.is_empty() {
        return Ok((None, None));
    }
    let mut dims = Vec::new();
    let mut agg_mode = None;
    for e in exprs {
        match e {
            Expr::Rollup(groups) => {
                agg_mode = Some("rollup".to_string());
                for g in groups {
                    for item in g {
                        dims.push(extract_cube_group_expr(
                            item,
                            cube_name,
                            cube_alias,
                            projections,
                            dim_exprs,
                        )?);
                    }
                }
            }
            Expr::Cube(groups) => {
                agg_mode = Some("cube".to_string());
                for g in groups {
                    for item in g {
                        dims.push(extract_cube_group_expr(
                            item,
                            cube_name,
                            cube_alias,
                            projections,
                            dim_exprs,
                        )?);
                    }
                }
            }
            Expr::GroupingSets(groups) => {
                agg_mode = Some("grouping_sets".to_string());
                for g in groups {
                    for item in g {
                        dims.push(extract_cube_group_expr(
                            item,
                            cube_name,
                            cube_alias,
                            projections,
                            dim_exprs,
                        )?);
                    }
                }
            }
            other => {
                dims.push(extract_cube_group_expr(
                    other,
                    cube_name,
                    cube_alias,
                    projections,
                    dim_exprs,
                )?);
            }
        }
    }
    Ok((Some(dims), agg_mode))
}

pub(crate) fn extract_cube_group_expr(
    expr: &Expr,
    cube_name: &str,
    cube_alias: &str,
    projections: &[CubeProjection],
    dim_exprs: &[String],
) -> Result<String, AppError> {
    match expr {
        Expr::Value(v) => {
            if let Ok(pos) = v.to_string().parse::<usize>() {
                if pos >= 1 && pos <= projections.len() {
                    let target = &projections[pos - 1].member;
                    if dim_exprs.iter().any(|d| d == target) {
                        return Ok(target.clone());
                    }
                }
            }
            Err(AppError::Validation(format!(
                "invalid GROUP BY position: {v}"
            )))
        }
        Expr::Identifier(id) => {
            if let Some(proj) = projections.iter().find(|p| p.alias == id.value) {
                if dim_exprs.iter().any(|d| d == &proj.member) {
                    return Ok(proj.member.clone());
                }
            }
            Ok(id.value.clone())
        }
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = &parts[0].value;
            let n = &parts[1].value;
            if q != cube_name && q != cube_alias {
                return Err(AppError::Validation(format!(
                    "qualifier {q} does not match cube {cube_name}"
                )));
            }
            if let Some(proj) = projections.iter().find(|p| p.alias == *n) {
                if dim_exprs.iter().any(|d| d == &proj.member) {
                    return Ok(proj.member.clone());
                }
            }
            Ok(n.clone())
        }
        Expr::Nested(inner) => {
            extract_cube_group_expr(inner, cube_name, cube_alias, projections, dim_exprs)
        }
        other => {
            let empty = HashMap::new();
            let stripped = expand_segments_and_strip_qualifier(
                other,
                &empty,
                cube_name,
                Some(cube_alias),
                true,
                None,
            )?;
            let s = stripped.to_string();
            if contains_window_sql(&s)? {
                return Err(AppError::Validation(
                    "window functions are not allowed in GROUP BY".into(),
                ));
            }
            assert_scalar_expr(&s)?;
            Ok(s)
        }
    }
}

pub(crate) fn extract_having_for_cube(
    select: &Select,
    cube_name: &str,
    cube_alias: &str,
) -> Result<Option<String>, AppError> {
    let Some(having) = &select.having else {
        return Ok(None);
    };
    let empty = HashMap::new();
    let stripped = expand_segments_and_strip_qualifier(
        having,
        &empty,
        cube_name,
        Some(cube_alias),
        true,
        None,
    )?;
    Ok(Some(stripped.to_string()))
}

pub(crate) fn extract_order_by(
    query: Option<&Query>,
    single_cube: Option<(&str, &str)>,
    join_sides: &[(String, String)],
) -> Result<Vec<String>, AppError> {
    let Some(q) = query else {
        return Ok(Vec::new());
    };
    let Some(ob) = &q.order_by else {
        return Ok(Vec::new());
    };
    let OrderByKind::Expressions(exprs) = &ob.kind else {
        return Err(AppError::Validation(
            "ORDER BY ALL is not supported in semantic SQL".into(),
        ));
    };
    let empty = HashMap::new();
    let mut out = Vec::new();
    for OrderByExpr { expr, options, .. } in exprs {
        let target = if let Some((alias, cname)) = single_cube {
            match expr {
                Expr::Identifier(id) => quote_ident(&id.value)?,
                Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
                    let qname = &parts[0].value;
                    let col = &parts[1].value;
                    if qname != alias && qname != cname {
                        return Err(AppError::Validation(format!(
                            "ORDER BY qualifier {qname} does not match cube {cname}"
                        )));
                    }
                    quote_ident(col)?
                }
                Expr::Value(v) if v.to_string().parse::<usize>().is_ok() => v.to_string(),
                other => {
                    let stripped = expand_segments_and_strip_qualifier(
                        other,
                        &empty,
                        cname,
                        Some(alias),
                        true,
                        None,
                    )?;
                    stripped.to_string()
                }
            }
        } else {
            match expr {
                Expr::Identifier(id) => quote_ident(&id.value)?,
                Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
                    let qname = &parts[0].value;
                    let col = &parts[1].value;
                    if let Some((side_alias, _)) =
                        join_sides.iter().find(|(a, c)| a == qname || c == qname)
                    {
                        format!("{}.{}", quote_ident(side_alias)?, quote_ident(col)?)
                    } else {
                        return Err(AppError::Validation(format!(
                            "unknown ORDER BY qualifier: {qname}"
                        )));
                    }
                }
                Expr::Value(v) if v.to_string().parse::<usize>().is_ok() => v.to_string(),
                other => {
                    return Err(AppError::Validation(format!(
                        "ORDER BY must reference a semantic column or alias, got: {other}"
                    )));
                }
            }
        };
        let mut item = target;
        match options.asc {
            Some(true) => item.push_str(" ASC"),
            Some(false) => item.push_str(" DESC"),
            None => {}
        }
        match options.nulls_first {
            Some(true) => item.push_str(" NULLS FIRST"),
            Some(false) => item.push_str(" NULLS LAST"),
            None => {}
        }
        out.push(item);
    }
    Ok(out)
}

pub(crate) fn extract_limit(
    query: Option<&Query>,
    binds: Option<&HashMap<String, String>>,
) -> Result<Option<String>, AppError> {
    let Some(q) = query else {
        return Ok(None);
    };
    let Some(lc) = &q.limit_clause else {
        return Ok(None);
    };
    match lc {
        LimitClause::LimitOffset {
            limit: Some(lim), ..
        } => parse_non_negative_int_expr(lim, binds, "LIMIT").map(Some),
        LimitClause::LimitOffset { limit: None, .. } => Ok(None),
        LimitClause::OffsetCommaLimit { limit, .. } => {
            parse_non_negative_int_expr(limit, binds, "LIMIT").map(Some)
        }
    }
}

pub(crate) fn extract_offset(
    query: Option<&Query>,
    binds: Option<&HashMap<String, String>>,
) -> Result<Option<String>, AppError> {
    let Some(q) = query else {
        return Ok(None);
    };
    let Some(lc) = &q.limit_clause else {
        return Ok(None);
    };
    match lc {
        LimitClause::LimitOffset {
            offset: Some(off), ..
        } => parse_non_negative_int_expr(&off.value, binds, "OFFSET").map(Some),
        LimitClause::LimitOffset { offset: None, .. } => Ok(None),
        LimitClause::OffsetCommaLimit { offset, .. } => {
            parse_non_negative_int_expr(offset, binds, "OFFSET").map(Some)
        }
    }
}

pub(crate) fn parse_non_negative_int_expr(
    expr: &Expr,
    binds: Option<&HashMap<String, String>>,
    clause_name: &str,
) -> Result<String, AppError> {
    match expr {
        Expr::Value(v) => match &v.value {
            SqlValue::Number(n, _) => {
                let parsed: u64 = n.parse().map_err(|_| {
                    AppError::Validation(format!("{clause_name} must be a non-negative integer"))
                })?;
                Ok(parsed.to_string())
            }
            SqlValue::Placeholder(p) => {
                let name = p.trim_start_matches(':');
                if let Some(val) = binds.and_then(|b| b.get(name)) {
                    let parsed: u64 = val.parse().map_err(|_| {
                        AppError::Validation(format!(
                            "{clause_name} bind :{name} must be an integer"
                        ))
                    })?;
                    Ok(parsed.to_string())
                } else {
                    Err(AppError::Validation(format!(
                        "unresolved bind parameter {p} in {clause_name}"
                    )))
                }
            }
            other => Err(AppError::Validation(format!(
                "{clause_name} must be a non-negative integer, got: {other}"
            ))),
        },
        other => Err(AppError::Validation(format!(
            "{clause_name} must be a non-negative integer, got: {other}"
        ))),
    }
}

