//! Persist / load registration JSON as typed meta_* rows.

use libsql::{Connection, params};
use serde_json::{Value, json};

use crate::error::AppError;

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
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let description = opt_str(body, "description");
    let combine = str_field(body, "combine")?;
    let prefer = opt_str(body, "prefer");
    conn.execute(
        "INSERT INTO meta_views (name, description, target, combine, prefer) VALUES (?1,?2,?3,?4,?5)",
        params![name, description, target, combine, prefer],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(sources) = body.get("sources").and_then(|v| v.as_array()) {
        for s in sources {
            let (skind, sname, summary) = if let Some(n) = s.as_str() {
                ("table", n, false)
            } else {
                (
                    s.get("kind").and_then(|v| v.as_str()).unwrap_or("table"),
                    s.get("name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| AppError::Validation("source.name".into()))?,
                    s.get("summary").and_then(|v| v.as_bool()).unwrap_or(false),
                )
            };
            conn.execute(
                "INSERT INTO meta_view_sources (view_name, kind, source_name, summary) VALUES (?1,?2,?3,?4)",
                params![name, skind, sname, summary as i32],
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
            let on = j
                .get("on")
                .cloned()
                .unwrap_or(json!([]))
                .to_string();
            conn.execute(
                "INSERT INTO meta_view_joins (view_name, left_source, right_source, join_type, on_conditions_json)
                 VALUES (?1,?2,?3,?4,?5)",
                params![name, left, right, jtype, on],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }
    if let Some(cols) = body.get("columns").and_then(|v| v.as_array()) {
        for c in cols {
            let cname = str_field(c, "name")?;
            let mappings = c
                .get("mappings")
                .cloned()
                .unwrap_or(json!([]))
                .to_string();
            conn.execute(
                "INSERT INTO meta_view_columns (view_name, name, mappings_json) VALUES (?1,?2,?3)",
                params![name, cname, mappings],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        }
    }
    Ok(())
}

async fn load_view(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT description, target, combine, prefer FROM meta_views WHERE name = ?1",
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
    let prefer: Option<String> = row.get(3).ok();

    let mut sources = Vec::new();
    let mut srows = conn
        .query(
            "SELECT kind, source_name, summary FROM meta_view_sources WHERE view_name = ?1",
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
        let source_name: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
        let summary: i32 = r.get(2).map_err(|e| AppError::Internal(e.into()))?;
        let mut s = json!({ "kind": kind, "name": source_name });
        if summary != 0 {
            s["summary"] = json!(true);
        }
        sources.push(s);
    }

    let mut joins = Vec::new();
    let mut jrows = conn
        .query(
            "SELECT left_source, right_source, join_type, on_conditions_json FROM meta_view_joins WHERE view_name = ?1",
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
        joins.push(json!({
            "left": left,
            "right": right,
            "type": jtype,
            "on": serde_json::from_str::<Value>(&on_s).unwrap_or(json!([])),
        }));
    }

    let mut columns = Vec::new();
    let mut crows = conn
        .query(
            "SELECT name, mappings_json FROM meta_view_columns WHERE view_name = ?1 ORDER BY name",
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
    if let Some(p) = prefer {
        out["prefer"] = json!(p);
    }
    if !joins.is_empty() {
        out["joins"] = json!(joins);
    }
    Ok(out)
}

async fn insert_cube(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let target = str_field(body, "target")?;
    let view = str_field(body, "view")?;
    let description = str_field(body, "description")?;
    conn.execute(
        "INSERT INTO meta_cubes (name, target, view_name, description) VALUES (?1,?2,?3,?4)",
        params![name, target, view, description],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;

    if let Some(dims) = body.get("dimensions").and_then(|v| v.as_array()) {
        for d in dims {
            insert_cube_member(conn, "meta_cube_dimensions", name, d, true).await?;
        }
    }
    if let Some(facts) = body.get("facts").and_then(|v| v.as_array()) {
        for f in facts {
            insert_cube_member(conn, "meta_cube_facts", name, f, false).await?;
        }
    }
    Ok(())
}

async fn insert_cube_member(
    conn: &Connection,
    table: &str,
    cube: &str,
    item: &Value,
    is_dim: bool,
) -> Result<(), AppError> {
    let name = str_field(item, "name")?;
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
            "INSERT INTO meta_cube_dimensions (cube_name, name, type, column_ref, expression, encryption, as_code_json, role_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                cube,
                name,
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
        conn.execute(
            "INSERT INTO meta_cube_facts (cube_name, name, type, column_ref, expression, encryption, role_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                cube,
                name,
                ftype,
                column_ref,
                expression,
                encryption as i32,
                role
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
            "SELECT target, view_name, description FROM meta_cubes WHERE name = ?1",
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

    let mut dimensions = Vec::new();
    let mut drows = conn
        .query(
            "SELECT name, type, column_ref, expression, encryption, as_code_json, role_json
             FROM meta_cube_dimensions WHERE cube_name = ?1 ORDER BY name",
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
            "SELECT name, type, column_ref, expression, encryption, role_json
             FROM meta_cube_facts WHERE cube_name = ?1 ORDER BY name",
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

    Ok(json!({
        "kind": "cube",
        "name": name,
        "target": target,
        "view": view,
        "description": description,
        "dimensions": dimensions,
        "facts": facts,
    }))
}

fn load_member_dim(r: &libsql::Row) -> Result<Value, AppError> {
    let name: String = r.get(0).map_err(|e| AppError::Internal(e.into()))?;
    let dtype: Option<String> = r.get(1).ok();
    let column_ref: Option<String> = r.get(2).ok();
    let expression: Option<String> = r.get(3).ok();
    let encryption: i32 = r.get(4).map_err(|e| AppError::Internal(e.into()))?;
    let as_code_json: Option<String> = r.get(5).ok();
    let role_s: String = r.get(6).map_err(|e| AppError::Internal(e.into()))?;
    let mut out = json!({
        "name": name,
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
    let ftype: String = r.get(1).map_err(|e| AppError::Internal(e.into()))?;
    let column_ref: Option<String> = r.get(2).ok();
    let expression: Option<String> = r.get(3).ok();
    let encryption: i32 = r.get(4).map_err(|e| AppError::Internal(e.into()))?;
    let role_s: String = r.get(5).map_err(|e| AppError::Internal(e.into()))?;
    let mut out = json!({
        "name": name,
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
    Ok(out)
}

async fn insert_metric(conn: &Connection, body: &Value) -> Result<(), AppError> {
    let name = str_field(body, "name")?;
    let cube = str_field(body, "cube")?;
    let description = opt_str(body, "description");
    let expression = str_field(body, "expression")?;
    let aggregation = opt_str(body, "aggregation");
    let group_by_json = body.get("group_by").map(|v| v.to_string());
    let filter_expression = filter_expr(body.get("filter"));
    conn.execute(
        "INSERT INTO meta_metrics (name, cube_name, description, expression, aggregation, group_by_json, filter_expression, role_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            name,
            cube,
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

async fn load_metric(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT cube_name, description, expression, aggregation, group_by_json, filter_expression, role_json
             FROM meta_metrics WHERE name = ?1",
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
    let description: Option<String> = row.get(1).ok();
    let expression: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let aggregation: Option<String> = row.get(3).ok();
    let group_by_json: Option<String> = row.get(4).ok();
    let filter_expression: Option<String> = row.get(5).ok();
    let role_s: String = row.get(6).map_err(|e| AppError::Internal(e.into()))?;
    let mut out = json!({
        "kind": "metric",
        "name": name,
        "cube": cube,
        "expression": expression,
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
        "filter": filter_obj(filter_expression),
    });
    if let Some(d) = description {
        out["description"] = json!(d);
    }
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
    let description = opt_str(body, "description");
    let filter_expression = filter_expr(body.get("filter")).ok_or_else(|| {
        AppError::Validation("segment.filter.expression required".into())
    })?;
    conn.execute(
        "INSERT INTO meta_segments (name, cube_name, description, filter_expression, role_json)
         VALUES (?1,?2,?3,?4,?5)",
        params![name, cube, description, filter_expression, role_json(body)],
    )
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
    Ok(())
}

async fn load_segment(conn: &Connection, name: &str) -> Result<Value, AppError> {
    let mut rows = conn
        .query(
            "SELECT cube_name, description, filter_expression, role_json FROM meta_segments WHERE name = ?1",
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
    let description: Option<String> = row.get(1).ok();
    let filter_expression: String = row.get(2).map_err(|e| AppError::Internal(e.into()))?;
    let role_s: String = row.get(3).map_err(|e| AppError::Internal(e.into()))?;
    let mut out = json!({
        "kind": "segment",
        "name": name,
        "cube": cube,
        "filter": { "expression": filter_expression },
        "role": serde_json::from_str::<Value>(&role_s).unwrap_or(json!("all")),
    });
    if let Some(d) = description {
        out["description"] = json!(d);
    }
    Ok(out)
}

fn str_field<'a>(v: &'a Value, key: &str) -> Result<&'a str, AppError> {
    v.get(key)
        .and_then(|x| x.as_str())
        .ok_or_else(|| AppError::Validation(format!("{key} required")))
}

fn opt_str(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}
