//! Persist / load registration JSON as typed meta_* rows.

use std::collections::{HashMap, HashSet, VecDeque};

use libsql::{params, Connection};
use serde_json::{json, Value};

use crate::error::AppError;

/// Join views: hub (max join degree) at `sources[0]`, then remaining sources by hop
/// depth from the hub (depth-1, then depth-2). Siblings keep their original relative
/// order. Shared-key `mappings` follow that same rank. Union/minus and identity views
/// (`joins: []`) are unchanged.
pub(crate) fn normalize_join_hub_first(view: &Value) -> Result<Value, AppError> {
    if view.get("combine").and_then(|v| v.as_str()) != Some("join") {
        return Ok(view.clone());
    }
    let Some(joins) = view.get("joins").and_then(|v| v.as_array()) else {
        return Ok(view.clone());
    };
    if joins.is_empty() {
        return Ok(view.clone());
    }
    let Some(sources) = view.get("sources").and_then(|v| v.as_array()) else {
        return Ok(view.clone());
    };
    if sources.is_empty() {
        return Ok(view.clone());
    }

    let mut degree: HashMap<String, usize> = HashMap::new();
    let mut adj: HashMap<String, Vec<String>> = HashMap::new();
    for j in joins {
        let Some(l) = j.get("left").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(r) = j.get("right").and_then(|v| v.as_str()) else {
            continue;
        };
        *degree.entry(l.to_string()).or_default() += 1;
        *degree.entry(r.to_string()).or_default() += 1;
        adj.entry(l.to_string()).or_default().push(r.to_string());
        adj.entry(r.to_string()).or_default().push(l.to_string());
    }
    let aliases: Vec<String> = sources
        .iter()
        .map(view_source_alias)
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(str::to_string)
        .collect();
    let max_deg = aliases
        .iter()
        .map(|a| degree.get(a).copied().unwrap_or(0))
        .max()
        .unwrap_or(0);
    if max_deg == 0 {
        return Ok(view.clone());
    }
    let hub_idx = aliases
        .iter()
        .position(|a| degree.get(a).copied().unwrap_or(0) == max_deg)
        .unwrap_or(0);
    let hub = aliases[hub_idx].clone();
    let orig_idx: HashMap<&str, usize> = aliases
        .iter()
        .enumerate()
        .map(|(i, a)| (a.as_str(), i))
        .collect();
    for nbs in adj.values_mut() {
        nbs.sort_by_key(|n| orig_idx.get(n.as_str()).copied().unwrap_or(usize::MAX));
        nbs.dedup();
    }

    let mut order: Vec<String> = Vec::with_capacity(aliases.len());
    let mut seen = HashSet::from([hub.clone()]);
    let mut q = VecDeque::from([hub.clone()]);
    while let Some(cur) = q.pop_front() {
        order.push(cur.clone());
        if let Some(nbs) = adj.get(&cur) {
            for nb in nbs {
                if seen.insert(nb.clone()) {
                    q.push_back(nb.clone());
                }
            }
        }
    }
    for a in &aliases {
        if seen.insert(a.clone()) {
            order.push(a.clone());
        }
    }

    let by_alias: HashMap<&str, &Value> = aliases
        .iter()
        .zip(sources.iter())
        .map(|(a, s)| (a.as_str(), s))
        .collect();
    let reordered: Vec<Value> = order
        .iter()
        .filter_map(|a| by_alias.get(a.as_str()).copied().cloned())
        .collect();
    let rank: HashMap<&str, usize> = order
        .iter()
        .enumerate()
        .map(|(i, a)| (a.as_str(), i))
        .collect();

    let mut out = view.clone();
    out["sources"] = json!(reordered);
    if let Some(cols) = out.get_mut("columns").and_then(|v| v.as_array_mut()) {
        for col in cols {
            if let Some(maps) = col.get_mut("mappings").and_then(|v| v.as_array_mut()) {
                if maps.len() > 1 {
                    maps.sort_by_key(|m| {
                        m.get("source")
                            .and_then(|v| v.as_str())
                            .and_then(|s| rank.get(s).copied())
                            .unwrap_or(usize::MAX)
                    });
                }
            }
        }
    }
    Ok(out)
}

fn view_source_alias(src: &Value) -> Result<&str, AppError> {
    if let Some(s) = src.as_str() {
        return Ok(s);
    }
    src.get("alias")
        .and_then(|v| v.as_str())
        .or_else(|| src.get("table").and_then(|v| v.as_str()))
        .or_else(|| src.get("name").and_then(|v| v.as_str()))
        .ok_or_else(|| AppError::Validation("source alias/table missing".into()))
}

async fn last_insert_id(conn: &Connection) -> Result<i64, AppError> {
    let mut rows = conn
        .query("SELECT last_insert_rowid()", ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("last_insert_rowid empty")))?;
    row.get(0).map_err(|e| AppError::Internal(e.into()))
}

async fn view_id_by_name(conn: &Connection, name: &str) -> Result<i64, AppError> {
    let mut rows = conn
        .query("SELECT id FROM meta_views WHERE name = ?1", params![name])
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("view '{name}' not found")))?;
    row.get(0).map_err(|e| AppError::Internal(e.into()))
}

async fn cube_id_by_name(conn: &Connection, name: &str) -> Result<i64, AppError> {
    let mut rows = conn
        .query("SELECT id FROM meta_cubes WHERE name = ?1", params![name])
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("cube '{name}' not found")))?;
    row.get(0).map_err(|e| AppError::Internal(e.into()))
}

async fn delete_view_children(conn: &Connection, view_id: i64) -> Result<(), AppError> {
    for sql in [
        "DELETE FROM meta_view_columns WHERE view_id = ?1",
        "DELETE FROM meta_view_joins WHERE view_id = ?1",
        "DELETE FROM meta_view_sources WHERE view_id = ?1",
    ] {
        conn.execute(sql, params![view_id])
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

async fn delete_cube_children(conn: &Connection, cube_id: i64) -> Result<(), AppError> {
    for sql in [
        "DELETE FROM meta_cube_choices WHERE cube_id = ?1",
        "DELETE FROM meta_cube_dimensions WHERE cube_id = ?1",
        "DELETE FROM meta_cube_facts WHERE cube_id = ?1",
    ] {
        conn.execute(sql, params![cube_id])
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

pub async fn delete_kind(conn: &Connection, kind: &str, name: &str) -> Result<u64, AppError> {
    let sql = match kind {
        "table" => "DELETE FROM meta_tables WHERE name = ?1",
        "view" => "DELETE FROM meta_views WHERE name = ?1",
        "cube" => "DELETE FROM meta_cubes WHERE name = ?1",
        "metric" => "DELETE FROM meta_metrics WHERE name = ?1",
        "segment" => "DELETE FROM meta_segments WHERE name = ?1",
        _ => return Err(AppError::Validation(format!("unknown kind {kind}"))),
    };
    conn.execute(sql, params![name])
        .await
        .map_err(|e| AppError::Internal(e.into()))
}

pub async fn insert_document(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let kind = body
        .get("kind")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("kind required".into()))?;
    match kind {
        "table" => insert_table(conn, body).await,
        "view" => insert_view(conn, body).await,
        "cube" => insert_cube(conn, body).await,
        "metric" => insert_metric(conn, body).await,
        "segment" => insert_segment(conn, body).await,
        _ => Err(AppError::Validation(format!("unknown kind {kind}"))),
    }
}

/// Keep the parent row (stable id) and replace unique `name` plus children.
/// Tables still delete+insert because `name` is the PK.
pub async fn replace_document(
    conn: &Connection,
    kind: &str,
    existing_name: &str,
    body: &Value,
) -> Result<(), AppError> {
    match kind {
        "table" => {
            delete_kind(conn, kind, existing_name).await?;
            insert_document(conn, body).await
        }
        "view" => replace_view(conn, existing_name, body).await,
        "cube" => replace_cube(conn, existing_name, body).await,
        "metric" => replace_metric(conn, existing_name, body).await,
        "segment" => replace_segment(conn, existing_name, body).await,
        _ => Err(AppError::Validation(format!("unknown kind {kind}"))),
    }
}

pub async fn load_document(conn: &Connection, kind: &str, name: &str) -> Result<Value, AppError> {
    match kind {
        "table" => load_table(conn, name).await,
        "view" => load_view(conn, name).await,
        "cube" => load_cube(conn, name).await,
        "metric" => load_metric(conn, name).await,
        "segment" => load_segment(conn, name).await,
        _ => Err(AppError::Validation(format!("unknown kind {kind}"))),
    }
}

pub async fn list_all(conn: &Connection) -> Result<Vec<Value>, AppError> {
    let mut out = Vec::new();
    for (kind, sql) in [
        ("table", "SELECT name FROM meta_tables ORDER BY name"),
        ("view", "SELECT name FROM meta_views ORDER BY name"),
        ("cube", "SELECT name FROM meta_cubes ORDER BY name"),
        ("metric", "SELECT name FROM meta_metrics ORDER BY name"),
        ("segment", "SELECT name FROM meta_segments ORDER BY name"),
    ] {
        let mut rows = conn
            .query(sql, ())
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| AppError::Internal(e.into()))?
        {
            let name: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
            out.push(load_document(conn, kind, &name).await?);
        }
    }
    Ok(out)
}

pub async fn list_kind(conn: &Connection, kind: &str) -> Result<Vec<Value>, AppError> {
    let sql = match kind {
        "table" => "SELECT name FROM meta_tables ORDER BY name",
        "view" => "SELECT name FROM meta_views ORDER BY name",
        "cube" => "SELECT name FROM meta_cubes ORDER BY name",
        "metric" => "SELECT name FROM meta_metrics ORDER BY name",
        "segment" => "SELECT name FROM meta_segments ORDER BY name",
        _ => return Err(AppError::Validation(format!("unknown kind {kind}"))),
    };
    let mut rows = conn
        .query(sql, ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let name: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
        out.push(load_document(conn, kind, &name).await?);
    }
    Ok(out)
}

fn role_json(body: &Value) -> String {
    body.get("role")
        .cloned()
        .unwrap_or(json!("all"))
        .to_string()
}

fn filter_expr(v: Option<&Value>) -> Option<String> {
    v.and_then(|f| {
        if f.is_null() {
            None
        } else {
            f.get("expression")
                .and_then(|e| e.as_str())
                .map(str::to_string)
        }
    })
}

fn filter_obj(expr: Option<String>) -> Value {
    match expr {
        Some(e) if !e.is_empty() => json!({ "expression": e }),
        _ => Value::Null,
    }
}

async fn insert_table(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let db = str_field(body, "db")?;
    let schema = str_field(body, "schema")?;
    let table = str_field(body, "table")?;
    let description = opt_str(body, "description");
    let pk_json = body.get("pk").map(|v| v.to_string());
    let fk_json = body.get("fk").map(|v| v.to_string());
    let scope = filter_expr(body.get("scope_filter"));
    let default_f = filter_expr(body.get("default_filter"));
    conn.execute(
        "INSERT INTO meta_tables (name, description, target, db, schema, table_name, pk_json, fk_json, scope_filter, default_filter, role_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            name,
            description,
            target,
            db,
            schema,
            table,
            pk_json,
            fk_json,
            scope,
            default_f,
            role_json(body)
        ],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(cols) = body.get("columns").and_then(|v| v.as_array()) {
        for c in cols {
            let cname = str_field(c, "name")?;
            let ctype = str_field(c, "type")?;
            let nullable = c.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
            conn.execute(
                "INSERT INTO meta_table_columns (table_name, name, type, nullable) VALUES (?1,?2,?3,?4)",
                params![name, cname, ctype, nullable as i32],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }
    if let Some(pfs) = body.get("permission_filters").and_then(|v| v.as_array()) {
        for pf in pfs {
            let role = str_field(pf, "role")?;
            let expr = str_field(pf, "expression")?;
            conn.execute(
                "INSERT INTO meta_table_permission_filters (table_name, role, expression) VALUES (?1,?2,?3)",
                params![name, role, expr],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }
    Ok(())
}

async fn load_table(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT description, target, db, schema, table_name, pk_json, fk_json, scope_filter, default_filter, role_json
             FROM meta_tables WHERE name = ?1",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("table/{name}")))?;
    let description: Option<String> = row.get(0).ok();
    let target: String = row.get(1).map_err(|e| AppError::Internal(e.into()))?;
    let db: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let schema: String = row.get(3).map_err(|e| AppError::Internal(e.into()))?;
    let table: String = row.get(4).map_err(|e| AppError::Internal(e.into()))?;
    let pk_json: Option<String> = row.get(5).ok();
    let fk_json: Option<String> = row.get(6).ok();
    let scope: Option<String> = row.get(7).ok();
    let default_f: Option<String> = row.get(8).ok();
    let role_s: String = row.get(9).map_err(|e| AppError::Internal(e.into()))?;

    let mut columns = Vec::new();
    let mut crows = conn
        .query(
            "SELECT name, type, nullable FROM meta_table_columns WHERE table_name = ?1 ORDER BY name",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = crows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let cname: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
        let ctype: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
        let nullable: i32 = r.get(2).map_err(|e| AppError::Internal(e.into()))?;
        columns.push(json!({
            "name": cname,
            "type": ctype,
            "nullable": nullable != 0,
        }));
    }

    let mut pfs = Vec::new();
    let mut prows = conn
        .query(
            "SELECT role, expression FROM meta_table_permission_filters WHERE table_name = ?1",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = prows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let role: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
        let expression: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
        pfs.push(json!({ "role": role, "expression": expression }));
    }

    let mut out = json!({
        "kind": "table",
        "name": name,
        "target": target,
        "db": db,
        "schema": schema,
        "table": table,
        "columns": columns,
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
    });
    if let Some(d) = description {
        out["description"] = json!(d);
    }
    if let Some(pk) = pk_json {
        out["pk"] = serde_json::from_str(&pk).unwrap_or(Value::Null);
    }
    if let Some(fk) = fk_json {
        out["fk"] = serde_json::from_str(&fk).unwrap_or(Value::Null);
    }
    if !pfs.is_empty() {
        out["permission_filters"] = json!(pfs);
    }
    out["scope_filter"] = filter_obj(scope);
    out["default_filter"] = filter_obj(default_f);
    Ok(out)
}

async fn insert_view(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let normalized = normalize_join_hub_first(body)?;
    let body = &normalized;
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let description = opt_str(body, "description");
    let combine = str_field(body, "combine")?;
    conn.execute(
        "INSERT INTO meta_views (name, description, target, combine) VALUES (?1,?2,?3,?4)",
        params![name, description, target, combine],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let view_id = last_insert_id(conn).await?;
    insert_view_children(conn, view_id, body).await
}

async fn insert_view_children(
    conn: &Connection,
    view_id: i64,
    body: &Value,
) -> Result<(), AppError> {
    if let Some(sources) = body.get("sources").and_then(|v| v.as_array()) {
        for s in sources {
            let (skind, catalog, alias) = if let Some(n) = s.as_str() {
                ("table", n, n)
            } else {
                let catalog = s
                    .get("table")
                    .and_then(|v| v.as_str())
                    .or_else(|| s.get("name").and_then(|v| v.as_str()))
                    .ok_or_else(|| AppError::Validation("source.table".into()))?;
                let alias = s.get("alias").and_then(|v| v.as_str()).unwrap_or(catalog);
                (
                    s.get("kind").and_then(|v| v.as_str()).unwrap_or("table"),
                    catalog,
                    alias,
                )
            };
            conn.execute(
                "INSERT INTO meta_view_sources (view_id, kind, source_name, catalog_name, summary) VALUES (?1,?2,?3,?4,0)",
                params![view_id, skind, alias, catalog],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }
    if let Some(joins) = body.get("joins").and_then(|v| v.as_array()) {
        for j in joins {
            let left = str_field(j, "left")?;
            let right = str_field(j, "right")?;
            let jtype = j.get("type").and_then(|v| v.as_str()).unwrap_or("inner");
            let on = j.get("on").cloned().unwrap_or(json!([])).to_string();
            let relationship = opt_str(j, "relationship");
            if let Some(rel) = relationship.as_deref() {
                if !matches!(rel, "many_to_one" | "one_to_one" | "one_to_many") {
                    return Err(AppError::Validation(format!(
                        "viewJoin.relationship must be many_to_one, one_to_one, or one_to_many, got: {rel}"
                    )));
                }
            }
            conn.execute(
                "INSERT INTO meta_view_joins (view_id, left_source, right_source, join_type, on_conditions_json, relationship)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![view_id, left, right, jtype, on, relationship],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }
    if let Some(cols) = body.get("columns").and_then(|v| v.as_array()) {
        for c in cols {
            let cname = str_field(c, "name")?;
            let mappings = c.get("mappings").cloned().unwrap_or(json!([])).to_string();
            conn.execute(
                "INSERT INTO meta_view_columns (view_id, name, mappings_json) VALUES (?1,?2,?3)",
                params![view_id, cname, mappings],
            )
            .await
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("UNIQUE constraint failed") && msg.contains("meta_view_columns") {
                    AppError::Validation(format!("duplicate column {cname}"))
                } else {
                    AppError::Internal(e.into())
                }
            })?;
        }
    }
    Ok(())
}

pub async fn replace_view(
    conn: &Connection,
    existing_name: &str,
    body: &Value,
) -> Result<(), AppError> {
    let normalized = normalize_join_hub_first(body)?;
    let body = &normalized;
    let view_id = view_id_by_name(conn, existing_name).await?;
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let description = opt_str(body, "description");
    let combine = str_field(body, "combine")?;
    conn.execute(
        "UPDATE meta_views SET name = ?1, description = ?2, target = ?3, combine = ?4 WHERE id = ?5",
        params![name, description, target, combine, view_id],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    delete_view_children(conn, view_id).await?;
    insert_view_children(conn, view_id, body).await
}

async fn load_view(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT description, target, combine FROM meta_views WHERE name = ?1",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("view/{name}")))?;
    let description: Option<String> = row.get(0).ok();
    let target: String = row.get(1).map_err(|e| AppError::Internal(e.into()))?;
    let combine: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;

    let mut sources = Vec::new();
    let mut srows = conn
        .query(
            "SELECT kind, source_name, catalog_name FROM meta_view_sources WHERE view_id = (SELECT id FROM meta_views WHERE name = ?1)",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = srows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let kind: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
        let alias: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
        let catalog: String = r
            .get::<String>(2)
            .ok()
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| alias.clone());
        let mut s = json!({ "kind": kind, "table": catalog });
        if alias != catalog {
            s["alias"] = json!(alias);
        }
        sources.push(s);
    }

    let mut joins = Vec::new();
    let mut jrows = conn
        .query(
            "SELECT left_source, right_source, join_type, on_conditions_json, relationship FROM meta_view_joins WHERE view_id = (SELECT id FROM meta_views WHERE name = ?1)",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = jrows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let left: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
        let right: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
        let jtype: String = r.get(2).map_err(|e| AppError::Internal(e.into()))?;
        let on_s: String = r.get(3).map_err(|e| AppError::Internal(e.into()))?;
        let relationship: Option<String> = r.get(4).ok();
        let mut join = json!({
            "left": left,
            "right": right,
            "type": jtype,
            "on": serde_json::from_str::<Value>(&on_s).unwrap_or(json!([])),
        });
        if let Some(rel) = relationship.filter(|s| !s.is_empty()) {
            join["relationship"] = json!(rel);
        }
        joins.push(join);
    }

    let mut columns = Vec::new();
    let mut crows = conn
        .query(
            "SELECT name, mappings_json FROM meta_view_columns WHERE view_id = (SELECT id FROM meta_views WHERE name = ?1) ORDER BY name",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = crows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let cname: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
        let mappings: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
        columns.push(json!({
            "name": cname,
            "mappings": serde_json::from_str::<Value>(&mappings).unwrap_or(json!([])),
        }));
    }

    let mut out = json!({
        "kind": "view",
        "name": name,
        "target": target,
        "combine": combine,
        "sources": sources,
        "columns": columns,
    });
    if let Some(d) = description {
        out["description"] = json!(d);
    }
    if !joins.is_empty() {
        out["joins"] = json!(joins);
    }
    normalize_join_hub_first(&out)
}

async fn insert_cube(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let view = str_field(body, "view")?;
    let description = str_field(body, "description")?;
    let grain_json = body
        .get("grain")
        .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        .map(|v| v.to_string());
    let prefer = opt_str(body, "prefer");
    let view_id = view_id_by_name(conn, view).await?;
    conn.execute(
        "INSERT INTO meta_cubes (name, target, view_id, description, role_json, grain_json, prefer) VALUES (?1,?2,?3,?4,?5,?6,?7)",
        params![name, target, view_id, description, role_json(body), grain_json, prefer],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    let cube_id = last_insert_id(conn).await?;
    insert_cube_children(conn, cube_id, body).await
}

async fn insert_cube_children(
    conn: &Connection,
    cube_id: i64,
    body: &Value,
) -> Result<(), AppError> {
    if let Some(choices) = body.get("choice").and_then(|v| v.as_array()) {
        for (pos, item) in choices.iter().enumerate() {
            let cview = str_field(item, "view")?;
            let cview_id = view_id_by_name(conn, cview).await?;
            let summary = item
                .get("summary")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            conn.execute(
                "INSERT INTO meta_cube_choices (cube_id, pos, view_id, summary) VALUES (?1,?2,?3,?4)",
                params![cube_id, pos as i64, cview_id, summary as i32],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }

    if let Some(dims) = body.get("dimensions").and_then(|v| v.as_array()) {
        for d in dims {
            insert_cube_member(conn, "meta_cube_dimensions", cube_id, d, true).await?;
        }
    }
    if let Some(facts) = body.get("facts").and_then(|v| v.as_array()) {
        for f in facts {
            insert_cube_member(conn, "meta_cube_facts", cube_id, f, false).await?;
        }
    }
    Ok(())
}

pub async fn replace_cube(
    conn: &Connection,
    existing_name: &str,
    body: &Value,
) -> Result<(), AppError> {
    let cube_id = cube_id_by_name(conn, existing_name).await?;
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let view = str_field(body, "view")?;
    let view_id = view_id_by_name(conn, view).await?;
    let description = str_field(body, "description")?;
    let grain_json = body
        .get("grain")
        .filter(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        .map(|v| v.to_string());
    let prefer = opt_str(body, "prefer");
    conn.execute(
        "UPDATE meta_cubes SET name = ?1, target = ?2, view_id = ?3, description = ?4, role_json = ?5, grain_json = ?6, prefer = ?7 WHERE id = ?8",
        params![name, target, view_id, description, role_json(body), grain_json, prefer, cube_id],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    delete_cube_children(conn, cube_id).await?;
    insert_cube_children(conn, cube_id, body).await
}

async fn insert_cube_member(
    conn: &Connection,
    table: &str,
    cube_id: i64,
    item: &Value,
    is_dim: bool,
) -> Result<(), AppError> {
    let name = str_field(item, "name")?;
    let description = str_field(item, "description")?;
    let column_ref = opt_str(item, "column");
    let expression = opt_str(item, "expression");
    let encryption = item
        .get("encryption")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let role = role_json(item);
    if is_dim {
        let dtype = opt_str(item, "type");
        let as_code_json = item
            .get("as_code")
            .filter(|v| !v.is_null())
            .map(|v| serde_json::to_string(v).unwrap_or_default());
        conn.execute(
            "INSERT INTO meta_cube_dimensions (cube_id, name, description, type, column_ref, expression, encryption, as_code_json, role_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                cube_id,
                name,
                description,
                dtype,
                column_ref,
                expression,
                encryption as i32,
                as_code_json,
                role
            ],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    } else {
        let ftype = str_field(item, "type")?;
        let additive = item
            .get("additive")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        conn.execute(
            "INSERT INTO meta_cube_facts (cube_id, name, description, type, column_ref, expression, encryption, role_json, additive)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                cube_id,
                name,
                description,
                ftype,
                column_ref,
                expression,
                encryption as i32,
                role,
                additive as i32
            ],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    let _ = table;
    Ok(())
}

async fn load_cube(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT c.target, v.name, c.description, c.role_json, c.grain_json, c.prefer
             FROM meta_cubes c JOIN meta_views v ON v.id = c.view_id WHERE c.name = ?1",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("cube/{name}")))?;
    let target: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
    let view: String = row.get(1).map_err(|e| AppError::Internal(e.into()))?;
    let description: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let role_s: String = row.get(3).unwrap_or_else(|_| "\"all\"".to_string());
    let grain_s: Option<String> = row.get(4).ok();
    let prefer: Option<String> = row.get(5).ok();

    let mut dimensions = Vec::new();
    let mut drows = conn
        .query(
            "SELECT name, description, type, column_ref, expression, encryption, as_code_json, role_json
             FROM meta_cube_dimensions WHERE cube_id = (SELECT id FROM meta_cubes WHERE name = ?1) ORDER BY name",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = drows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        dimensions.push(load_member_dim(&r)?);
    }

    let mut facts = Vec::new();
    let mut frows = conn
        .query(
            "SELECT name, description, type, column_ref, expression, encryption, role_json, additive
             FROM meta_cube_facts WHERE cube_id = (SELECT id FROM meta_cubes WHERE name = ?1) ORDER BY name",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = frows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        facts.push(load_member_fact(&r)?);
    }

    let mut out = json!({
        "kind": "cube",
        "name": name,
        "target": target,
        "view": view,
        "description": description,
        "dimensions": dimensions,
        "facts": facts,
    });
    let role_val = serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all"));
    if role_val != json!("all") {
        out["role"] = role_val;
    }
    if let Some(raw) = grain_s.filter(|s| !s.is_empty()) {
        if let Ok(grain) = serde_json::from_str::<Value>(&raw) {
            if grain.as_array().is_some_and(|a| !a.is_empty()) {
                out["grain"] = grain;
            }
        }
    }
    if let Some(p) = prefer.filter(|s| !s.is_empty()) {
        out["prefer"] = json!(p);
    }
    let mut choice = Vec::new();
    let mut chrows = conn
        .query(
            "SELECT v.name, ch.summary FROM meta_cube_choices ch JOIN meta_views v ON v.id = ch.view_id
             WHERE ch.cube_id = (SELECT id FROM meta_cubes WHERE name = ?1) ORDER BY ch.pos",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    while let Some(r) = chrows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let cview: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
        let summary: i32 = r.get(1).unwrap_or(1);
        let mut item = json!({ "view": cview });
        if summary != 0 {
            item["summary"] = json!(true);
        }
        choice.push(item);
    }
    if !choice.is_empty() {
        out["choice"] = json!(choice);
    }
    Ok(out)
}

fn load_member_dim(r: &libsql::Row) -> Result<Value, AppError> {
    let name: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
    let description: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
    let dtype: Option<String> = r.get(2).ok();
    let column_ref: Option<String> = r.get(3).ok();
    let expression: Option<String> = r.get(4).ok();
    let encryption: i32 = r.get(5).map_err(|e| AppError::Internal(e.into()))?;
    let as_code_json: Option<String> = r.get(6).ok();
    let role_s: String = r.get(7).map_err(|e| AppError::Internal(e.into()))?;
    let mut out = json!({
        "name": name,
        "description": description,
        "encryption": encryption != 0,
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
    });
    if let Some(t) = dtype {
        out["type"] = json!(t);
    }
    if let Some(c) = column_ref {
        out["column"] = json!(c);
    }
    if let Some(e) = expression {
        out["expression"] = json!(e);
    }
    if let Some(raw) = as_code_json.filter(|s| !s.is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(&raw) {
            out["as_code"] = v;
        }
    }
    Ok(out)
}

fn load_member_fact(r: &libsql::Row) -> Result<Value, AppError> {
    let name: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
    let description: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
    let ftype: String = r.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let column_ref: Option<String> = r.get(3).ok();
    let expression: Option<String> = r.get(4).ok();
    let encryption: i32 = r.get(5).map_err(|e| AppError::Internal(e.into()))?;
    let role_s: String = r.get(6).map_err(|e| AppError::Internal(e.into()))?;
    let additive: i32 = r.get(7).unwrap_or(1);
    let mut out = json!({
        "name": name,
        "description": description,
        "type": ftype,
        "encryption": encryption != 0,
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
    });
    if let Some(c) = column_ref {
        out["column"] = json!(c);
    }
    if let Some(e) = expression {
        out["expression"] = json!(e);
    }
    if additive == 0 {
        out["additive"] = json!(false);
    }
    Ok(out)
}

async fn insert_metric(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let cube = str_field(body, "cube")?;
    let cube_id = cube_id_by_name(conn, cube).await?;
    let description = str_field(body, "description")?;
    let expression = str_field(body, "expression")?;
    let aggregation = opt_str(body, "aggregation");
    let group_by_json = body.get("group_by").map(|v| v.to_string());
    let filter_expression = filter_expr(body.get("filter"));
    conn.execute(
        "INSERT INTO meta_metrics (name, cube_id, description, expression, aggregation, group_by_json, filter_expression, role_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            name,
            cube_id,
            description,
            expression,
            aggregation,
            group_by_json,
            filter_expression,
            role_json(body)
        ],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn replace_metric(
    conn: &Connection,
    existing_name: &str,
    body: &Value,
) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let cube = str_field(body, "cube")?;
    let cube_id = cube_id_by_name(conn, cube).await?;
    let description = str_field(body, "description")?;
    let expression = str_field(body, "expression")?;
    let aggregation = opt_str(body, "aggregation");
    let group_by_json = body.get("group_by").map(|v| v.to_string());
    let filter_expression = filter_expr(body.get("filter"));
    let n = conn
        .execute(
            "UPDATE meta_metrics SET name = ?1, cube_id = ?2, description = ?3, expression = ?4, aggregation = ?5, group_by_json = ?6, filter_expression = ?7, role_json = ?8 WHERE name = ?9",
            params![
                name,
                cube_id,
                description,
                expression,
                aggregation,
                group_by_json,
                filter_expression,
                role_json(body),
                existing_name
            ],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("metric/{existing_name}")));
    }
    Ok(())
}

async fn load_metric(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT c.name, m.description, m.expression, m.aggregation, m.group_by_json, m.filter_expression, m.role_json
             FROM meta_metrics m JOIN meta_cubes c ON c.id = m.cube_id WHERE m.name = ?1",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("metric/{name}")))?;
    let cube: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
    let description: String = row.get::<String>(1).unwrap_or_else(|_| name.to_string());
    let expression: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let aggregation: Option<String> = row.get(3).ok();
    let group_by_json: Option<String> = row.get(4).ok();
    let filter_expression: Option<String> = row.get(5).ok();
    let role_s: String = row.get(6).map_err(|e| AppError::Internal(e.into()))?;
    let mut out = json!({
        "kind": "metric",
        "name": name,
        "description": description,
        "cube": cube,
        "expression": expression,
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
        "filter": filter_obj(filter_expression),
    });
    if let Some(a) = aggregation {
        out["aggregation"] = json!(a);
    }
    if let Some(g) = group_by_json {
        out["group_by"] = serde_json::from_str(&g).unwrap_or(Value::Null);
    }
    Ok(out)
}

async fn insert_segment(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let cube = str_field(body, "cube")?;
    let cube_id = cube_id_by_name(conn, cube).await?;
    let description = str_field(body, "description")?;
    let filter_expression = filter_expr(body.get("filter"))
        .ok_or_else(|| AppError::Validation("segment.filter.expression required".into()))?;
    conn.execute(
        "INSERT INTO meta_segments (name, cube_id, description, filter_expression, role_json)
         VALUES (?1,?2,?3,?4,?5)",
        params![name, cube_id, description, filter_expression, role_json(body)],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn replace_segment(
    conn: &Connection,
    existing_name: &str,
    body: &Value,
) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let cube = str_field(body, "cube")?;
    let cube_id = cube_id_by_name(conn, cube).await?;
    let description = str_field(body, "description")?;
    let filter_expression = filter_expr(body.get("filter"))
        .ok_or_else(|| AppError::Validation("segment.filter.expression required".into()))?;
    let n = conn
        .execute(
            "UPDATE meta_segments SET name = ?1, cube_id = ?2, description = ?3, filter_expression = ?4, role_json = ?5 WHERE name = ?6",
            params![name, cube_id, description, filter_expression, role_json(body), existing_name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if n == 0 {
        return Err(AppError::NotFound(format!("segment/{existing_name}")));
    }
    Ok(())
}

async fn load_segment(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT c.name, s.description, s.filter_expression, s.role_json
             FROM meta_segments s JOIN meta_cubes c ON c.id = s.cube_id WHERE s.name = ?1",
            params![name],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let row = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
        .ok_or_else(|| AppError::NotFound(format!("segment/{name}")))?;
    let cube: String = row.get(0).map_err(|e| AppError::Internal(e.into()))?;
    let description: String = row.get::<String>(1).unwrap_or_else(|_| name.to_string());
    let filter_expression: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let role_s: String = row.get(3).map_err(|e| AppError::Internal(e.into()))?;
    Ok(json!({
        "kind": "segment",
        "name": name,
        "description": description,
        "cube": cube,
        "filter": { "expression": filter_expression },
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
    }))
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, AppError> {
    v.get(key)
        .and_then(|x| x.as_str())
        .ok_or_else(|| AppError::Validation(format!("{key} required")))
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}
