//! Semantic SQL → physical SQL (spec 6-step pipeline entry).
//! Free physical SELECT is not supported — input must reference cube(s).
//! Same MCP `target` cubes may JOIN / UNION (·EXCEPT/INTERSECT).

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sqlparser::ast::{
    BinaryOperator, Expr, GroupByExpr, Ident, Join, JoinConstraint, JoinOperator, LimitClause,
    OrderByExpr, OrderByKind, Query, Select, SelectItem, SetExpr, SetOperator, SetQuantifier,
    Statement, TableFactor, Value as SqlValue,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::compile::{
    CubeProjection, CubeQuerySpec, apply_binds, check_role_access, compile_cube_query, quote_ident,
};
use crate::dialect::DialectProfile;
use crate::error::AppError;
use crate::select_guard::assert_select_only;

/// Compile semantic SQL to physical SELECT. `role` applies table permission_filters.
/// Defaults to Postgres dialect emit (ANSI quotes).
pub fn compile_semantic_sql(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
) -> Result<String, AppError> {
    compile_semantic_sql_with_binds(docs, sql, role, None, DialectProfile::Postgres)
}

/// All FROM cube names in a semantic SELECT (including JOIN/UNION arms).
pub fn cube_names_from_semantic_sql(sql: &str) -> Result<Vec<String>, AppError> {
    let query = parse_query(sql)?;
    let mut names = Vec::new();
    collect_cubes_from_set_expr(query.body.as_ref(), &mut names)?;
    Ok(names)
}

/// First FROM cube name (compat helper).
pub fn cube_name_from_semantic_sql(sql: &str) -> Result<String, AppError> {
    cube_names_from_semantic_sql(sql)?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Validation("semantic SQL has no FROM cube".into()))
}

pub fn compile_semantic_sql_with_binds(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
    dialect: DialectProfile,
) -> Result<String, AppError> {
    let query = parse_query(sql)?;
    let mut cubes = Vec::new();
    collect_cubes_from_set_expr(query.body.as_ref(), &mut cubes)?;
    if cubes.is_empty() {
        return Err(AppError::Validation(
            "semantic SQL must reference at least one cube".into(),
        ));
    }
    assert_same_cube_target(docs, &cubes)?;
    for c in &cubes {
        let cube = find_doc(docs, "cube", c)?;
        check_role_access(cube.get("role"), role)?;
    }
    // Steps 1–7: engine-neutral canonical SQL (ANSI "…" · date_trunc).
    let physical = compile_query(docs, &query, role, binds)?;
    assert_select_only(&physical)?;
    // Step 8 (D21): date_trunc rewrite + quote adapt.
    dialect.emit_physical_sql(&physical)
}

fn parse_query(sql: &str) -> Result<Query, AppError> {
    assert_select_only(sql)?;
    let dialect = GenericDialect {};
    let stmts =
        Parser::parse_sql(&dialect, sql).map_err(|e| AppError::SqlParse(e.to_string()))?;
    let Statement::Query(query) = stmts
        .into_iter()
        .next()
        .ok_or_else(|| AppError::NotSelect)?
    else {
        return Err(AppError::NotSelect);
    };
    Ok(*query)
}

fn compile_query(
    docs: &[Value],
    query: &Query,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    match query.body.as_ref() {
        SetExpr::Select(select) => compile_select(docs, select, Some(query), role, binds),
        SetExpr::Query(inner) => compile_query(docs, inner, role, binds),
        other => {
            let mut sql = compile_set_expr(docs, other, role, binds)?;
            let order_by = extract_order_by(Some(query), None, &[])?;
            if !order_by.is_empty() {
                sql.push_str(" ORDER BY ");
                sql.push_str(&order_by.join(", "));
            }
            if let Some(lim) = extract_limit(Some(query), binds)? {
                sql.push_str(" LIMIT ");
                sql.push_str(&lim);
            }
            if let Some(off) = extract_offset(Some(query), binds)? {
                sql.push_str(" OFFSET ");
                sql.push_str(&off);
            }
            Ok(sql)
        }
    }
}

fn compile_set_expr(
    docs: &[Value],
    expr: &SetExpr,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    match expr {
        SetExpr::Select(select) => compile_select(docs, select, None, role, binds),
        SetExpr::Query(q) => compile_query(docs, q, role, binds),
        SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
        } => {
            match op {
                SetOperator::Union | SetOperator::Except | SetOperator::Intersect => {}
                SetOperator::Minus => {}
            }
            let l = compile_set_expr(docs, left, role, binds)?;
            let r = compile_set_expr(docs, right, role, binds)?;
            let quant = match set_quantifier {
                SetQuantifier::All => " ALL",
                SetQuantifier::Distinct => " DISTINCT",
                SetQuantifier::None => "",
                other => {
                    return Err(AppError::Validation(format!(
                        "unsupported set quantifier: {other}"
                    )));
                }
            };
            Ok(format!("({l}) {op}{quant} ({r})"))
        }
        _ => Err(AppError::Validation(
            "semantic SQL must be SELECT or set operation of SELECTs".into(),
        )),
    }
}

fn compile_select(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SELECT must have exactly one FROM item (joins go on that item)".into(),
        ));
    }
    let tw = &select.from[0];
    if tw.joins.is_empty() {
        compile_single_cube(docs, select, query, role, binds)
    } else {
        compile_cube_join(docs, select, query, role, binds)
    }
}

fn compile_single_cube(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let (cube_alias, cube_name) = relation_alias_cube(&select.from[0].relation)?;
    let (projections, metrics, dims) =
        classify_projection_for_cube(select, docs, &cube_name, Some(&cube_alias), true)?;
    if metrics.is_empty() {
        return Err(AppError::Validation(
            "semantic SELECT must project at least one metric".into(),
        ));
    }

    let (segments, where_exprs) =
        extract_where_for_cube(select, docs, &cube_name, Some(&cube_alias), true, role)?;
    let (explicit_group_by, aggregation) =
        extract_group_by_for_cube(select, &cube_name, &cube_alias)?;
    let having = extract_having_for_cube(select, &cube_name, &cube_alias)?;
    let order_by = extract_order_by(query, Some((&cube_alias, &cube_name)), &[])?;
    let limit = extract_limit(query, binds)?;
    let offset = extract_offset(query, binds)?;

    let spec = CubeQuerySpec {
        projections,
        metrics,
        dimensions: dims,
        segments,
        where_exprs,
        explicit_group_by,
        aggregation,
        having,
        order_by,
        limit,
        offset,
    };

    compile_cube_query(docs, &cube_name, &spec, role, binds)
}

fn compile_cube_join(
    docs: &[Value],
    select: &Select,
    query: Option<&Query>,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let tw = &select.from[0];
    let mut sides: Vec<(String, String)> = Vec::new(); // (alias, cube_name)
    let (alias0, cube0) = relation_alias_cube(&tw.relation)?;
    sides.push((alias0, cube0));
    for j in &tw.joins {
        let (alias, cube) = relation_alias_cube(&j.relation)?;
        sides.push((alias, cube));
    }

    // Validate and rewrite JOIN ON clauses first, recording dimensions needed from each side.
    let mut on_dims_per_side: HashMap<String, Vec<String>> = HashMap::new();
    let mut rewritten_joins: Vec<(String, &'static str, String)> = Vec::new();
    for join in &tw.joins {
        let (alias, _) = relation_alias_cube(&join.relation)?;
        let join_kw = join_keyword(&join.join_operator)?;
        let on_sql = rewrite_and_validate_join_on(
            docs,
            &sides,
            join,
            role,
            binds,
            &mut on_dims_per_side,
        )?;
        rewritten_joins.push((alias, join_kw, on_sql));
    }

    let mut subqueries: HashMap<String, String> = HashMap::new();
    for (alias, cube_name) in &sides {
        let (_projs, metrics, mut dims) =
            classify_projection_for_cube(select, docs, cube_name, Some(alias), false)?;
        if metrics.is_empty() {
            return Err(AppError::Validation(format!(
                "joined cube {cube_name} (alias {alias}) must project at least one metric"
            )));
        }
        // If dimensions were explicitly projected for this side, ensure any JOIN ON dimensions
        // for this side are also included so the outer ON clause can reference them.
        if !dims.is_empty() {
            if let Some(on_dims) = on_dims_per_side.get(alias) {
                for od in on_dims {
                    if !dims.iter().any(|d| d == od) {
                        dims.push(od.clone());
                    }
                }
            }
        }
        let (segments, where_exprs) =
            extract_where_for_cube(select, docs, cube_name, Some(alias), false, role)?;
        let mut sub_projs = Vec::new();
        for d in &dims {
            sub_projs.push(CubeProjection {
                member: d.clone(),
                alias: d.clone(),
            });
        }
        for m in &metrics {
            sub_projs.push(CubeProjection {
                member: m.clone(),
                alias: m.clone(),
            });
        }
        let spec = CubeQuerySpec {
            projections: if dims.is_empty() {
                Vec::new()
            } else {
                sub_projs
            },
            metrics,
            dimensions: dims,
            segments,
            where_exprs,
            explicit_group_by: None,
            aggregation: None,
            having: None,
            order_by: Vec::new(),
            limit: None,
            offset: None,
        };
        let physical = compile_cube_query(docs, cube_name, &spec, role, binds)?;
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

    let mut proj_parts = Vec::new();
    for item in &select.projection {
        let (alias_opt, member, out_alias) = projection_member(item)?;
        let (side_alias, _cube) =
            resolve_member_side(docs, &sides, alias_opt.as_deref(), &member)?;
        let qside = quote_ident(&side_alias)?;
        let qmem = quote_ident(&member)?;
        let qout = quote_ident(&out_alias)?;
        proj_parts.push(format!("{qside}.{qmem} AS {qout}"));
    }
    if proj_parts.is_empty() {
        return Err(AppError::Validation(
            "JOIN SELECT needs at least one projection".into(),
        ));
    }

    let mut sql = format!("SELECT {} FROM {}", proj_parts.join(", "), from_sql);
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

fn join_keyword(op: &JoinOperator) -> Result<&'static str, AppError> {
    match op {
        JoinOperator::Join(_) | JoinOperator::Inner(_) => Ok("INNER JOIN"),
        JoinOperator::Left(_) | JoinOperator::LeftOuter(_) => Ok("LEFT JOIN"),
        JoinOperator::Right(_) | JoinOperator::RightOuter(_) => Ok("RIGHT JOIN"),
        JoinOperator::FullOuter(_) => Ok("FULL JOIN"),
        JoinOperator::CrossJoin => Err(AppError::Validation(
            "CROSS JOIN of cubes is not supported".into(),
        )),
        _ => Err(AppError::Validation(
            "unsupported cube join type".into(),
        )),
    }
}

fn rewrite_and_validate_join_on(
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

fn rewrite_join_on_expr(
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
                let l =
                    rewrite_join_on_operand(docs, sides, left, role, binds, on_dims_per_side)?;
                let r =
                    rewrite_join_on_operand(docs, sides, right, role, binds, on_dims_per_side)?;
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

fn rewrite_join_on_operand(
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
                return Err(AppError::Validation(format!(
                    "unknown semantic member {mem} on cube {cube_name}"
                )));
            }
            validate_join_member(docs, &cube_name, mem, role)?;
            record_on_dim(docs, &cube_name, &side_alias, mem, on_dims_per_side)?;
            Ok(format!(
                "{}.{}",
                quote_ident(&side_alias)?,
                quote_ident(mem)?
            ))
        }
        Expr::Value(_) => Ok( apply_binds(&expr.to_string(), binds) ),
        other => Err(AppError::Validation(format!(
            "unsupported operand in cube JOIN ON: {other}"
        ))),
    }
}

fn validate_join_member(
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

fn record_on_dim(
    docs: &[Value],
    cube_name: &str,
    side_alias: &str,
    member: &str,
    on_dims_per_side: &mut HashMap<String, Vec<String>>,
) -> Result<(), AppError> {
    let cube = find_doc(docs, "cube", cube_name)?;
    if let Some(dims) = cube.get("dimensions").and_then(|v| v.as_array()) {
        if dims
            .iter()
            .any(|d| d.get("name").and_then(|v| v.as_str()) == Some(member))
        {
            let entry = on_dims_per_side.entry(side_alias.to_string()).or_default();
            if !entry.iter().any(|x| x == member) {
                entry.push(member.to_string());
            }
        }
    }
    Ok(())
}

fn projection_member(item: &SelectItem) -> Result<(Option<String>, String, String), AppError> {
    match item {
        SelectItem::UnnamedExpr(Expr::Identifier(id)) => {
            Ok((None, id.value.clone(), id.value.clone()))
        }
        SelectItem::UnnamedExpr(Expr::CompoundIdentifier(parts)) if parts.len() == 2 => Ok((
            Some(parts[0].value.clone()),
            parts[1].value.clone(),
            parts[1].value.clone(),
        )),
        SelectItem::ExprWithAlias {
            expr: Expr::Identifier(id),
            alias,
        } => Ok((None, id.value.clone(), alias.value.clone())),
        SelectItem::ExprWithAlias {
            expr: Expr::CompoundIdentifier(parts),
            alias,
        } if parts.len() == 2 => Ok((
            Some(parts[0].value.clone()),
            parts[1].value.clone(),
            alias.value.clone(),
        )),
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => Err(
            AppError::Validation("SELECT * not allowed in semantic SQL".into()),
        ),
        _ => Err(AppError::Validation(
            "semantic projection must be [cube.]member identifiers".into(),
        )),
    }
}

fn resolve_member_side(
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

fn member_on_cube(docs: &[Value], cube_name: &str, member: &str) -> Result<bool, AppError> {
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

fn classify_projection_for_cube(
    select: &Select,
    docs: &[Value],
    cube_name: &str,
    only_alias: Option<&str>,
    is_single_cube: bool,
) -> Result<(Vec<CubeProjection>, Vec<String>, Vec<String>), AppError> {
    let metric_names: HashSet<String> = docs
        .iter()
        .filter(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("metric")
                && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .filter_map(|d| d.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let cube = find_doc(docs, "cube", cube_name)?;
    let mut dim_names = HashSet::new();
    for key in ["dimensions", "facts"] {
        if let Some(items) = cube.get(key).and_then(|v| v.as_array()) {
            for item in items {
                if let Some(n) = item.get("name").and_then(|v| v.as_str()) {
                    dim_names.insert(n.to_string());
                }
            }
        }
    }

    let mut projs = Vec::new();
    let mut metrics = Vec::new();
    let mut dims = Vec::new();
    for item in &select.projection {
        let (qual, member, out_alias) = projection_member(item)?;
        if let Some(only) = only_alias {
            match &qual {
                Some(q) if q != only && q != cube_name => {
                    if is_single_cube {
                        return Err(AppError::Validation(format!(
                            "qualifier {q} does not match FROM cube {cube_name}"
                        )));
                    }
                    continue;
                }
                None if !is_single_cube => {
                    if !metric_names.contains(&member) && !dim_names.contains(&member) {
                        continue;
                    }
                }
                _ => {}
            }
        }
        if metric_names.contains(&member) {
            if !metrics.iter().any(|m| m == &member) {
                metrics.push(member.clone());
            }
            projs.push(CubeProjection {
                member,
                alias: out_alias,
            });
        } else if dim_names.contains(&member) {
            if !dims.iter().any(|d| d == &member) {
                dims.push(member.clone());
            }
            projs.push(CubeProjection {
                member,
                alias: out_alias,
            });
        } else if is_single_cube || qual.is_some() {
            return Err(AppError::Validation(format!(
                "unknown semantic member: {member}"
            )));
        }
    }
    Ok((projs, metrics, dims))
}

fn extract_where_for_cube(
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
    let seg_map: HashMap<String, &Value> = docs
        .iter()
        .filter(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("segment")
                && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .filter_map(|d| {
            d.get("name")
                .and_then(|v| v.as_str())
                .map(|n| (n.to_string(), d))
        })
        .collect();

    let mut conjuncts = Vec::new();
    split_and_conjuncts(selection, &mut conjuncts);

    let mut segments = Vec::new();
    let mut where_exprs = Vec::new();

    for conj in conjuncts {
        if let Some(seg_name) = match_segment_ident(conj, &seg_map, only_alias, cube_name) {
            if !segments.iter().any(|s| s == &seg_name) {
                segments.push(seg_name);
            }
            continue;
        }
        if !is_single_cube && !expr_belongs_to_cube(conj, docs, cube_name, only_alias)? {
            continue;
        }
        let rewritten =
            expand_segments_and_strip_qualifier(conj, &seg_map, cube_name, only_alias, is_single_cube, role)?;
        where_exprs.push(rewritten.to_string());
    }

    Ok((segments, where_exprs))
}

fn split_and_conjuncts<'a>(expr: &'a Expr, out: &mut Vec<&'a Expr>) {
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

fn match_segment_ident(
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

fn expr_belongs_to_cube(
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
    for d in docs {
        if d.get("kind").and_then(|v| v.as_str()) == Some("segment")
            && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        {
            if let Some(n) = d.get("name").and_then(|v| v.as_str()) {
                known.insert(n.to_string());
            }
        }
    }
    Ok(!bare.is_empty() && bare.iter().all(|b| known.contains(b)))
}

fn collect_expr_quals_and_idents(expr: &Expr, quals: &mut Vec<String>, bare: &mut Vec<String>) {
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
        Expr::UnaryOp { expr, .. } | Expr::Nested(expr) | Expr::IsNull(expr) | Expr::IsNotNull(expr) => {
            collect_expr_quals_and_idents(expr, quals, bare);
        }
        Expr::Between { expr, low, high, .. } => {
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
        _ => {}
    }
}

fn expand_segments_and_strip_qualifier(
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

fn rewrite_where_ast(
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
        _ => Ok(()),
    }
}

fn parse_cond_fragment(sql: &str) -> Result<Expr, AppError> {
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

fn extract_group_by_for_cube(
    select: &Select,
    cube_name: &str,
    cube_alias: &str,
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
                        dims.push(extract_cube_ident(item, cube_name, cube_alias)?);
                    }
                }
            }
            Expr::Cube(groups) => {
                agg_mode = Some("cube".to_string());
                for g in groups {
                    for item in g {
                        dims.push(extract_cube_ident(item, cube_name, cube_alias)?);
                    }
                }
            }
            Expr::GroupingSets(groups) => {
                agg_mode = Some("grouping_sets".to_string());
                for g in groups {
                    for item in g {
                        dims.push(extract_cube_ident(item, cube_name, cube_alias)?);
                    }
                }
            }
            other => {
                dims.push(extract_cube_ident(other, cube_name, cube_alias)?);
            }
        }
    }
    Ok((Some(dims), agg_mode))
}

fn extract_cube_ident(
    expr: &Expr,
    cube_name: &str,
    cube_alias: &str,
) -> Result<String, AppError> {
    match expr {
        Expr::Identifier(id) => Ok(id.value.clone()),
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = &parts[0].value;
            let n = &parts[1].value;
            if q != cube_name && q != cube_alias {
                return Err(AppError::Validation(format!(
                    "qualifier {q} does not match cube {cube_name}"
                )));
            }
            Ok(n.clone())
        }
        Expr::Nested(inner) => extract_cube_ident(inner, cube_name, cube_alias),
        other => Err(AppError::Validation(format!(
            "unsupported GROUP BY expression: {other}"
        ))),
    }
}

fn extract_having_for_cube(
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

fn extract_order_by(
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
    let mut out = Vec::new();
    for OrderByExpr { expr, options, .. } in exprs {
        let target = match expr {
            Expr::Identifier(id) => quote_ident(&id.value)?,
            Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
                let qname = &parts[0].value;
                let col = &parts[1].value;
                if let Some((alias, cname)) = single_cube {
                    if qname != alias && qname != cname {
                        return Err(AppError::Validation(format!(
                            "ORDER BY qualifier {qname} does not match cube {cname}"
                        )));
                    }
                    quote_ident(col)?
                } else if let Some((side_alias, _)) = join_sides
                    .iter()
                    .find(|(a, c)| a == qname || c == qname)
                {
                    format!("{}.{}", quote_ident(side_alias)?, quote_ident(col)?)
                } else {
                    return Err(AppError::Validation(format!(
                        "unknown ORDER BY qualifier: {qname}"
                    )));
                }
            }
            other => {
                return Err(AppError::Validation(format!(
                    "ORDER BY must reference a semantic column or alias, got: {other}"
                )));
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

fn extract_limit(
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

fn extract_offset(
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

fn parse_non_negative_int_expr(
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

fn collect_cubes_from_set_expr(expr: &SetExpr, out: &mut Vec<String>) -> Result<(), AppError> {
    match expr {
        SetExpr::Select(select) => collect_cubes_from_select(select, out),
        SetExpr::Query(q) => collect_cubes_from_set_expr(q.body.as_ref(), out),
        SetExpr::SetOperation { left, right, .. } => {
            collect_cubes_from_set_expr(left, out)?;
            collect_cubes_from_set_expr(right, out)
        }
        _ => Err(AppError::Validation(
            "semantic SQL must be SELECT or set operation of SELECTs".into(),
        )),
    }
}

fn collect_cubes_from_select(select: &Select, out: &mut Vec<String>) -> Result<(), AppError> {
    if select.from.len() != 1 {
        return Err(AppError::Validation(
            "semantic SELECT must have exactly one FROM item".into(),
        ));
    }
    push_cube_unique(out, &relation_cube_name(&select.from[0].relation)?);
    for j in &select.from[0].joins {
        push_cube_unique(out, &relation_cube_name(&j.relation)?);
    }
    Ok(())
}

fn push_cube_unique(out: &mut Vec<String>, name: &str) {
    if !out.iter().any(|x| x == name) {
        out.push(name.to_string());
    }
}

fn relation_cube_name(rel: &TableFactor) -> Result<String, AppError> {
    Ok(relation_alias_cube(rel)?.1)
}

fn relation_alias_cube(rel: &TableFactor) -> Result<(String, String), AppError> {
    match rel {
        TableFactor::Table { name, alias, .. } => {
            if name.0.len() != 1 {
                return Err(AppError::Validation(
                    "physical schema.table FROM is not allowed; use cube name only".into(),
                ));
            }
            let cube = name.0[0].to_string();
            let al = alias
                .as_ref()
                .map(|a| a.name.value.clone())
                .unwrap_or_else(|| cube.clone());
            Ok((al, cube))
        }
        _ => Err(AppError::Validation(
            "semantic FROM must be a cube identifier".into(),
        )),
    }
}

fn assert_same_cube_target(docs: &[Value], cubes: &[String]) -> Result<(), AppError> {
    let mut target: Option<&str> = None;
    for name in cubes {
        let cube = find_doc(docs, "cube", name)?;
        let t = cube
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or_else(|| AppError::Validation(format!("cube {name}: target missing")))?;
        match target {
            None => target = Some(t),
            Some(prev) if prev == t => {}
            Some(prev) => {
                return Err(AppError::Validation(format!(
                    "cube JOIN/UNION requires same target; {name} has {t}, others have {prev}"
                )));
            }
        }
    }
    Ok(())
}

fn find_doc<'a>(docs: &'a [Value], kind: &str, name: &str) -> Result<&'a Value, AppError> {
    docs.iter()
        .find(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some(kind)
                && d.get("name").and_then(|v| v.as_str()) == Some(name)
        })
        .ok_or_else(|| AppError::NotFound(format!("{kind}/{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn example_docs() -> Vec<Value> {
        [
            include_str!("../../semantic/examples/tables/customer.json"),
            include_str!("../../semantic/examples/tables/orders.json"),
            include_str!("../../semantic/examples/tables/orders_month.json"),
            include_str!("../../semantic/examples/tables/region_codes.json"),
            include_str!("../../semantic/examples/tables/status_codes.json"),
            include_str!("../../semantic/examples/views/orders_customer.json"),
            include_str!("../../semantic/examples/cubes/sales.json"),
            include_str!("../../semantic/examples/metrics/revenue.json"),
            include_str!("../../semantic/examples/segments/paid.json"),
        ]
        .into_iter()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect()
    }

    fn docs_with_second_cube(target: &str) -> Vec<Value> {
        let mut docs = example_docs();
        // Second cube same view/target for JOIN/UNION tests.
        let mut sales_b: Value = docs
            .iter()
            .find(|d| d.get("name").and_then(|v| v.as_str()) == Some("sales"))
            .cloned()
            .unwrap();
        sales_b["name"] = json!("sales_b");
        sales_b["target"] = json!(target);
        docs.push(sales_b);
        docs.push(json!({
            "kind": "metric",
            "name": "revenue_b",
            "description": "sales_b 지역·일자별 판매 금액 합계.",
            "cube": "sales_b",
            "expression": "sum(sale_amount)",
            "aggregation": "group",
            "group_by": ["region_code", "order_date"],
            "filter": null,
            "role": "all"
        }));
        docs
    }

    #[test]
    fn compiles_revenue_with_segment() {
        let docs = example_docs();
        let sql = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales WHERE paid",
            Some("branch"),
        )
        .unwrap();
        assert!(sql.to_ascii_lowercase().contains("sum("));
        assert!(sql.contains("PAID") || sql.contains("paid") || sql.contains("'PAID'"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn compiles_union_all_same_target() {
        let docs = docs_with_second_cube("dw");
        let sql = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales UNION ALL SELECT revenue_b FROM sales_b",
            None,
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("UNION ALL"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_union_different_target() {
        let mut docs = docs_with_second_cube("other");
        // sales stays dw; sales_b is other
        let err = compile_semantic_sql(
            &docs,
            "SELECT revenue FROM sales UNION ALL SELECT revenue_b FROM sales_b",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{err:?}");
        let _ = &mut docs;
    }

    #[test]
    fn compiles_join_same_target() {
        let docs = docs_with_second_cube("dw");
        let sql = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sales_b.revenue_b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            None,
        )
        .unwrap();
        let u = sql.to_ascii_uppercase();
        assert!(u.contains("JOIN"));
        assert!(u.contains("REGION_CODE"));
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn rejects_join_different_target() {
        let docs = docs_with_second_cube("other");
        let err = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sales_b.revenue_b \
             FROM sales JOIN sales_b ON sales.region_code = sales_b.region_code",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{err:?}");
    }

    #[test]
    fn rejects_physical_from() {
        let docs = example_docs();
        let err = compile_semantic_sql(
            &docs,
            "SELECT order_id FROM sales.orders",
            None,
        )
        .unwrap_err();
        assert!(matches!(
            err,
            AppError::Validation(_) | AppError::NotFound(_)
        ));
    }

    #[test]
    fn rejects_bare_physical_select() {
        let docs = example_docs();
        assert!(compile_semantic_sql(&docs, "SELECT 1", None).is_err());
    }

    #[test]
    fn mysql_dialect_adapts_identifier_quotes() {
        let docs = example_docs();
        let sql = compile_semantic_sql_with_binds(
            &docs,
            "SELECT revenue FROM sales WHERE paid",
            Some("branch"),
            None,
            DialectProfile::Mysql,
        )
        .unwrap();
        assert!(sql.contains('`'), "expected MySQL backticks in {sql}");
        assert!(
            !sql.contains('"'),
            "canonical double quotes should be adapted away: {sql}"
        );
        assert!(sql.to_ascii_lowercase().contains("sum("));
    }

    #[test]
    fn compiles_multiple_metrics_with_distinct_filters_and_dynamic_group_by() {
        let mut docs = example_docs();
        docs.push(json!({
            "kind": "metric",
            "name": "paid_revenue",
            "description": "결제 완료 판매 금액.",
            "cube": "sales",
            "expression": "sum(sale_amount)",
            "aggregation": "group",
            "group_by": ["region_code", "order_date"],
            "filter": {"expression": "status_code = 'PAID'"},
            "role": "all"
        }));
        let mut binds = HashMap::new();
        binds.insert("caller_region".into(), "KR".into());
        let sql = compile_semantic_sql_with_binds(
            &docs,
            "SELECT region_code AS reg, revenue AS total_rev, paid_revenue AS paid_rev \
             FROM sales \
             WHERE region_code = :caller_region \
             HAVING revenue > 100 \
             ORDER BY total_rev DESC \
             LIMIT 10 OFFSET 5",
            Some("branch"),
            Some(&binds),
            DialectProfile::Postgres,
        )
        .unwrap();
        let upper = sql.to_ascii_uppercase();
        assert!(upper.contains("CASE WHEN"), "expected CASE WHEN for distinct metric filters: {sql}");
        assert!(upper.contains("HAVING"), "expected HAVING clause: {sql}");
        assert!(upper.contains("ORDER BY \"TOTAL_REV\" DESC"), "expected ORDER BY total_rev DESC: {sql}");
        assert!(upper.contains("LIMIT 10"), "expected LIMIT 10: {sql}");
        assert!(upper.contains("OFFSET 5"), "expected OFFSET 5: {sql}");
        assert!(sql.contains("'KR'"), "expected bind :caller_region resolved to 'KR': {sql}");
        // Dynamic group_by: only region_code was projected, so order_date should not be in GROUP BY
        assert!(!sql.contains("order_date\" AS"), "order_date should not be projected: {sql}");
        assert_select_only(&sql).unwrap();
    }

    #[test]
    fn enforces_metric_segment_and_dimension_rbac_and_encryption_guards() {
        let mut docs = example_docs();
        docs.push(json!({
            "kind": "metric",
            "name": "finance_metric",
            "description": "재무 전용 지표.",
            "cube": "sales",
            "expression": "sum(sale_amount)",
            "aggregation": "group",
            "group_by": ["region_code"],
            "filter": null,
            "role": ["finance"]
        }));
        // Restricted metric rejected for branch role
        assert!(matches!(
            compile_semantic_sql(&docs, "SELECT finance_metric FROM sales", Some("branch")),
            Err(AppError::Forbidden(_))
        ));
        // Allowed for finance role
        assert!(compile_semantic_sql(&docs, "SELECT finance_metric FROM sales", Some("finance")).is_ok());

        // Mark customer_id as encrypted in sales cube and verify WHERE filter is rejected
        for doc in &mut docs {
            if doc.get("kind").and_then(|v| v.as_str()) == Some("cube")
                && doc.get("name").and_then(|v| v.as_str()) == Some("sales")
            {
                if let Some(dims) = doc.get_mut("dimensions").and_then(|v| v.as_array_mut()) {
                    for d in dims {
                        if d.get("name").and_then(|v| v.as_str()) == Some("customer_id") {
                            d["encryption"] = json!(true);
                        }
                    }
                }
            }
        }
        assert!(matches!(
            compile_semantic_sql(
                &docs,
                "SELECT revenue FROM sales WHERE customer_id = 'C1'",
                Some("branch")
            ),
            Err(AppError::Validation(_))
        ));
    }

    #[test]
    fn rejects_unknown_member_in_join_on() {
        let docs = docs_with_second_cube("dw");
        let err = compile_semantic_sql(
            &docs,
            "SELECT sales.revenue, sales_b.revenue_b \
             FROM sales JOIN sales_b ON sales.nonexistent = sales_b.region_code",
            None,
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)), "{err:?}");
    }
}
