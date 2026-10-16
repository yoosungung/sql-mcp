//! Semantic SQL → physical SQL (spec 6-step pipeline entry).
//! Free physical SELECT is not supported — input must reference cube(s).
//! Same MCP `target` cubes may JOIN / UNION (·EXCEPT/INTERSECT).

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use sqlparser::ast::{
    Expr, Join, JoinConstraint, JoinOperator, Query, Select, SelectItem, SetExpr, SetOperator,
    SetQuantifier, Statement, TableFactor,
};
use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

use crate::compile::{compile_metric_select_with_binds, quote_ident};
use crate::error::AppError;
use crate::select_guard::assert_select_only;

/// Compile semantic SQL to physical SELECT. `role` applies table permission_filters.
pub fn compile_semantic_sql(
    docs: &[Value],
    sql: &str,
    role: Option<&str>,
) -> Result<String, AppError> {
    compile_semantic_sql_with_binds(docs, sql, role, None)
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
    let physical = compile_set_expr(docs, query.body.as_ref(), role, binds)?;
    assert_select_only(&physical)?;
    Ok(physical)
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

fn compile_set_expr(
    docs: &[Value],
    expr: &SetExpr,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    match expr {
        SetExpr::Select(select) => compile_select(docs, select, role, binds),
        SetExpr::Query(q) => compile_set_expr(docs, q.body.as_ref(), role, binds),
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
        compile_single_cube(docs, select, role, binds)
    } else {
        compile_cube_join(docs, select, role, binds)
    }
}

fn compile_single_cube(
    docs: &[Value],
    select: &Select,
    role: Option<&str>,
    binds: Option<&HashMap<String, String>>,
) -> Result<String, AppError> {
    let cube_name = relation_cube_name(&select.from[0].relation)?;
    let (metrics, dims) = classify_projection_for_cube(select, docs, &cube_name, None)?;
    let segments = extract_segments_for_cube(select, docs, &cube_name, None)?;

    if metrics.is_empty() {
        return Err(AppError::Validation(
            "semantic SELECT must project at least one metric".into(),
        ));
    }
    if metrics.len() > 1 {
        return Err(AppError::Validation(
            "multiple metrics in one SELECT not yet supported".into(),
        ));
    }

    let metric_name = &metrics[0];
    let _is_semi_additive = is_semi_additive_metric(docs, metric_name)?;

    let seg_refs: Vec<&str> = segments.iter().map(String::as_str).collect();
    let physical =
        compile_metric_select_with_binds(docs, metric_name, &seg_refs, role, binds)?;

    if !dims.is_empty() {
        let metric = find_doc(docs, "metric", metric_name)?;
        let gb: Vec<String> = metric
            .get("group_by")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        for d in &dims {
            if !gb.iter().any(|g| g == d) {
                return Err(AppError::Validation(format!(
                    "dimension {d} not in metric {metric_name} group_by"
                )));
            }
        }
    }

    Ok(physical)
}

fn compile_cube_join(
    docs: &[Value],
    select: &Select,
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

    let mut subqueries: Vec<(String, String)> = Vec::new(); // (alias, physical sql)
    for (alias, cube_name) in &sides {
        let (metrics, _dims) =
            classify_projection_for_cube(select, docs, cube_name, Some(alias))?;
        let segments = extract_segments_for_cube(select, docs, cube_name, Some(alias))?;
        if metrics.is_empty() {
            return Err(AppError::Validation(format!(
                "joined cube {cube_name} (alias {alias}) must project at least one metric"
            )));
        }
        if metrics.len() > 1 {
            return Err(AppError::Validation(format!(
                "joined cube {cube_name}: multiple metrics not yet supported"
            )));
        }
        let metric_name = &metrics[0];
        let seg_refs: Vec<&str> = segments.iter().map(String::as_str).collect();
        let physical =
            compile_metric_select_with_binds(docs, metric_name, &seg_refs, role, binds)?;
        subqueries.push((alias.clone(), physical));
    }

    let mut from_sql = String::new();
    let (a0, p0) = &subqueries[0];
    let qa0 = quote_ident(a0)?;
    from_sql.push_str(&format!("({p0}) AS {qa0}"));
    for (i, join) in tw.joins.iter().enumerate() {
        let (alias, phys) = &subqueries[i + 1];
        let qalias = quote_ident(alias)?;
        let join_kw = join_keyword(&join.join_operator)?;
        let on_sql = join_on_sql(join)?;
        from_sql.push_str(&format!(" {join_kw} ({phys}) AS {qalias} ON {on_sql}"));
    }

    let mut proj_parts = Vec::new();
    for item in &select.projection {
        let (alias_opt, member, out_alias) = projection_member(item)?;
        let (side_alias, _cube) = resolve_member_side(docs, &sides, alias_opt.as_deref(), &member)?;
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

    Ok(format!(
        "SELECT {} FROM {}",
        proj_parts.join(", "),
        from_sql
    ))
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

fn join_on_sql(join: &Join) -> Result<String, AppError> {
    match &join.join_operator {
        JoinOperator::Join(c)
        | JoinOperator::Inner(c)
        | JoinOperator::Left(c)
        | JoinOperator::LeftOuter(c)
        | JoinOperator::Right(c)
        | JoinOperator::RightOuter(c)
        | JoinOperator::FullOuter(c) => match c {
            JoinConstraint::On(expr) => Ok(format!("({})", expr)),
            JoinConstraint::Using(_) => Err(AppError::Validation(
                "USING join not supported for cubes; use ON".into(),
            )),
            JoinConstraint::Natural => Err(AppError::Validation(
                "NATURAL join not supported for cubes".into(),
            )),
            JoinConstraint::None => Err(AppError::Validation(
                "cube JOIN requires ON".into(),
            )),
        },
        _ => Err(AppError::Validation("cube JOIN requires ON".into())),
    }
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
) -> Result<(Vec<String>, Vec<String>), AppError> {
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

    let mut metrics = Vec::new();
    let mut dims = Vec::new();
    for item in &select.projection {
        let (qual, member, _) = projection_member(item)?;
        if let Some(only) = only_alias {
            match &qual {
                Some(q) if q != only && q != cube_name => continue,
                None => {
                    // bare name: include only if unique to this cube among join sides — caller
                    // already filtered via resolve; for compile side, include if on this cube.
                    if !metric_names.contains(&member) && !dim_names.contains(&member) {
                        continue;
                    }
                }
                _ => {}
            }
        } else if let Some(q) = &qual {
            if q != cube_name {
                return Err(AppError::Validation(format!(
                    "qualifier {q} does not match FROM cube {cube_name}"
                )));
            }
        }
        if metric_names.contains(&member) {
            if !metrics.iter().any(|m| m == &member) {
                metrics.push(member);
            }
        } else if dim_names.contains(&member) {
            if !dims.iter().any(|d| d == &member) {
                dims.push(member);
            }
        } else if only_alias.is_none() {
            return Err(AppError::Validation(format!(
                "unknown semantic member: {member}"
            )));
        }
    }
    Ok((metrics, dims))
}

fn extract_segments_for_cube(
    select: &Select,
    docs: &[Value],
    cube_name: &str,
    only_alias: Option<&str>,
) -> Result<Vec<String>, AppError> {
    let Some(selection) = &select.selection else {
        return Ok(vec![]);
    };
    let seg_names: HashSet<String> = docs
        .iter()
        .filter(|d| {
            d.get("kind").and_then(|v| v.as_str()) == Some("segment")
                && d.get("cube").and_then(|v| v.as_str()) == Some(cube_name)
        })
        .filter_map(|d| d.get("name").and_then(|v| v.as_str()).map(str::to_string))
        .collect();
    let mut found = Vec::new();
    collect_segment_idents(selection, &seg_names, only_alias, cube_name, &mut found);
    Ok(found)
}

fn collect_segment_idents(
    expr: &Expr,
    known: &HashSet<String>,
    only_alias: Option<&str>,
    cube_name: &str,
    out: &mut Vec<String>,
) {
    match expr {
        Expr::Identifier(id) => {
            if known.contains(&id.value) && !out.iter().any(|x| x == &id.value) {
                out.push(id.value.clone());
            }
        }
        Expr::CompoundIdentifier(parts) if parts.len() == 2 => {
            let q = &parts[0].value;
            let n = &parts[1].value;
            let ok_qual = match only_alias {
                Some(a) => q == a || q == cube_name,
                None => q == cube_name,
            };
            if ok_qual && known.contains(n) && !out.iter().any(|x| x == n) {
                out.push(n.clone());
            }
        }
        Expr::BinaryOp { left, right, .. } => {
            collect_segment_idents(left, known, only_alias, cube_name, out);
            collect_segment_idents(right, known, only_alias, cube_name, out);
        }
        Expr::UnaryOp { expr, .. } => {
            collect_segment_idents(expr, known, only_alias, cube_name, out)
        }
        Expr::Nested(e) => collect_segment_idents(e, known, only_alias, cube_name, out),
        _ => {}
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

fn is_semi_additive_metric(docs: &[Value], metric_name: &str) -> Result<bool, AppError> {
    let metric = find_doc(docs, "metric", metric_name)?;
    let expr = metric
        .get("expression")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    Ok(expr.contains("count(distinct")
        || expr.contains("avg(")
        || expr.contains("median("))
}

fn check_role_access(role_val: Option<&Value>, caller: Option<&str>) -> Result<(), AppError> {
    let Some(role_val) = role_val else {
        return Ok(());
    };
    if role_val.as_str() == Some("all") || role_val.is_null() {
        return Ok(());
    }
    let Some(caller) = caller else {
        return Err(AppError::Forbidden(
            "caller_role required for restricted cube".into(),
        ));
    };
    match role_val {
        Value::String(s) if s == "all" || s == caller => Ok(()),
        Value::Array(arr) if arr.iter().any(|v| v.as_str() == Some(caller)) => Ok(()),
        Value::String(s) if s == caller => Ok(()),
        _ => Err(AppError::Forbidden(format!(
            "role {caller} cannot access this cube"
        ))),
    }
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
}
