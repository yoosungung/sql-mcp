use std::collections::HashMap;
use std::sync::Arc;

use libsql::{Connection, params};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::embed::{Embedder, blob_to_f32, cosine, f32_to_blob};
use crate::error::AppError;

const RRF_K: f32 = 60.0;

#[derive(Debug, Clone)]
pub struct SearchHit {
    pub target: String,
    pub doc_kind: String,
    pub doc_name: String,
    pub field_name: String,
    pub field_role: String,
    pub text: String,
    pub score: f32,
    pub mode: String,
}

#[derive(Debug, Clone)]
pub struct IndexRow {
    pub target: String,
    pub doc_kind: String,
    pub doc_name: String,
    pub field_name: String,
    pub field_role: String,
    pub text: String,
}

/// Prepare index rows and incrementally compute embeddings only for rows whose text changed.
pub async fn prepare_index_rows(
    conn: &Connection,
    docs: &[Value],
    embedder: Option<&dyn Embedder>,
) -> Result<Vec<(IndexRow, Option<Vec<u8>>)>, AppError> {
    let cube_targets: HashMap<String, String> = docs
        .iter()
        .filter(|d| d.get("kind").and_then(|v| v.as_str()) == Some("cube"))
        .filter_map(|d| {
            let name = d.get("name")?.as_str()?.to_string();
            let target = d
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("dw")
                .to_string();
            Some((name, target))
        })
        .collect();

    let mut rows = Vec::new();
    for doc in docs {
        rows.extend(extract_index_rows_with_targets(doc, &cube_targets));
    }

    if let Some(embedder) = embedder {
        // Load existing embeddings keyed by (doc_kind, doc_name, field_name, field_role, text)
        let mut existing: HashMap<(String, String, String, String, String), Vec<u8>> =
            HashMap::new();
        if let Ok(mut q) = conn
            .query(
                "SELECT doc_kind, doc_name, field_name, field_role, text, embedding FROM column_index WHERE embedding IS NOT NULL",
                (),
            )
            .await
        {
            while let Ok(Some(r)) = q.next().await {
                if let (Ok(k), Ok(n), Ok(f), Ok(fr), Ok(t), Ok(b)) = (
                    r.get::<String>(0),
                    r.get::<String>(1),
                    r.get::<String>(2),
                    r.get::<String>(3),
                    r.get::<String>(4),
                    r.get::<Vec<u8>>(5),
                ) {
                    if !b.is_empty() {
                        existing.insert((k, n, f, fr, t), b);
                    }
                }
            }
        }

        let mut missing_indices = Vec::new();
        let mut missing_texts = Vec::new();
        let mut out: Vec<(IndexRow, Option<Vec<u8>>)> = Vec::with_capacity(rows.len());

        for (i, row) in rows.into_iter().enumerate() {
            let key = (
                row.doc_kind.clone(),
                row.doc_name.clone(),
                row.field_name.clone(),
                row.field_role.clone(),
                row.text.clone(),
            );
            if let Some(blob) = existing.get(&key) {
                out.push((row, Some(blob.clone())));
            } else {
                missing_indices.push(i);
                missing_texts.push(row.text.clone());
                out.push((row, None));
            }
        }

        if !missing_texts.is_empty() {
            let new_embs = embedder.embed(&missing_texts).await?;
            for (idx, emb) in missing_indices.into_iter().zip(new_embs.into_iter()) {
                out[idx].1 = Some(f32_to_blob(&emb));
            }
        }
        Ok(out)
    } else {
        Ok(rows.into_iter().map(|r| (r, None)).collect())
    }
}

/// Write prepared index rows into `column_index` and `column_index_fts` (caller manages transaction).
pub async fn write_prepared_index(
    conn: &Connection,
    prepared: &[(IndexRow, Option<Vec<u8>>)],
) -> Result<(), AppError> {
    conn.execute("DELETE FROM column_index_fts", ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    conn.execute("DELETE FROM column_index", ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    for (row, emb) in prepared {
        conn.execute(
            "INSERT INTO column_index (target, doc_kind, doc_name, field_name, field_role, text, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.target.as_str(),
                row.doc_kind.as_str(),
                row.doc_name.as_str(),
                row.field_name.as_str(),
                row.field_role.as_str(),
                row.text.as_str(),
                emb.clone()
            ],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

        let mut q = conn
            .query(
                "SELECT id FROM column_index WHERE doc_kind=?1 AND doc_name=?2 AND field_name=?3 AND field_role=?4",
                params![
                    row.doc_kind.as_str(),
                    row.doc_name.as_str(),
                    row.field_name.as_str(),
                    row.field_role.as_str()
                ],
            )
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        let id: i64 = q
            .next()
            .await
            .map_err(|e| AppError::Internal(e.into()))?
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("missing index row")))?
            .get(0)
            .map_err(|e| AppError::Internal(e.into()))?;
        let fts_text = augment_fts_text(&row.text);
        conn.execute(
            "INSERT INTO column_index_fts (text, row_id) VALUES (?1, ?2)",
            params![fts_text.as_str(), id],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    }
    Ok(())
}

pub async fn rebuild_index(
    conn: &Connection,
    docs: &[Value],
    embedder: Option<&dyn Embedder>,
) -> Result<(), AppError> {
    let prepared = prepare_index_rows(conn, docs, embedder).await?;
    write_prepared_index(conn, &prepared).await
}

fn name_desc_text(name: &str, doc: &Value) -> String {
    match doc.get("description").and_then(|v| v.as_str()) {
        Some(d) if !d.is_empty() => format!("{name} {d}"),
        _ => name.to_string(),
    }
}

/// D12/D26: cube/dim/fact — name + description; dim/fact also append type.
fn member_index_text(item: &Value) -> String {
    let name = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let with_desc = name_desc_text(name, item);
    match item.get("type").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
        Some(dtype) => format!("{with_desc} {dtype}"),
        None => with_desc,
    }
}

#[cfg(test)]
fn extract_index_rows(doc: &Value) -> Vec<IndexRow> {
    extract_index_rows_with_targets(doc, &HashMap::new())
}

fn extract_index_rows_with_targets(
    doc: &Value,
    _cube_targets: &HashMap<String, String>,
) -> Vec<IndexRow> {
    let kind = doc.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let name = doc.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let mut rows = Vec::new();
    match kind {
        "cube" => {
            let target = doc
                .get("target")
                .and_then(|v| v.as_str())
                .unwrap_or("dw")
                .to_string();
            rows.push(IndexRow {
                target: target.clone(),
                doc_kind: kind.into(),
                doc_name: name.into(),
                field_name: name.into(),
                field_role: "document".into(),
                text: name_desc_text(name, doc),
            });
            for (role, key) in [("dimension", "dimensions"), ("fact", "facts")] {
                if let Some(items) = doc.get(key).and_then(|v| v.as_array()) {
                    for item in items {
                        let fname = item.get("name").and_then(|v| v.as_str()).unwrap_or("");
                        rows.push(IndexRow {
                            target: target.clone(),
                            doc_kind: kind.into(),
                            doc_name: name.into(),
                            field_name: fname.into(),
                            field_role: role.into(),
                            text: member_index_text(item),
                        });
                    }
                }
            }
        }
        // D26: metric/segment are describe_table.examples only — not search columns.
        "metric" | "segment" => {}
        _ => {}
    }
    rows
}

pub async fn search_columns(
    conn: &Arc<Mutex<Connection>>,
    query: &str,
    mode: &str,
    limit: usize,
    embedder: Option<&dyn Embedder>,
) -> Result<Vec<SearchHit>, AppError> {
    search_columns_filtered(conn, query, None, mode, limit, embedder).await
}

pub async fn search_columns_filtered(
    conn: &Arc<Mutex<Connection>>,
    query: &str,
    target: Option<&str>,
    mode: &str,
    limit: usize,
    embedder: Option<&dyn Embedder>,
) -> Result<Vec<SearchHit>, AppError> {
    let mode = normalize_mode(mode, embedder.is_some());
    match mode.as_str() {
        "keyword" => keyword_search(conn, query, target, limit).await,
        "vector" => vector_search(conn, query, target, limit, embedder).await,
        "hybrid" => {
            let kw = keyword_search(conn, query, target, limit * 2).await?;
            let vec = if embedder.is_some() {
                vector_search(conn, query, target, limit * 2, embedder).await?
            } else {
                vec![]
            };
            Ok(rrf_fuse(kw, vec, limit))
        }
        other => Err(AppError::Validation(format!("unknown search mode: {other}"))),
    }
}

fn normalize_mode(mode: &str, has_embedder: bool) -> String {
    let m = if mode.is_empty() { "hybrid" } else { mode };
    if matches!(m, "vector" | "hybrid") && !has_embedder {
        "keyword".into()
    } else {
        m.into()
    }
}

/// Convert SQLite FTS5 `bm25()` rank (negative float; more negative = better match) into a
/// monotonically increasing score in `(0.0, 1.0]`.
pub fn bm25_rank_to_score(rank: f64) -> f32 {
    if !rank.is_finite() {
        return 0.0;
    }
    if rank < 0.0 {
        let pos = (-rank) as f32;
        pos / (1.0 + pos)
    } else {
        1.0 / (1.0 + rank as f32)
    }
}

async fn keyword_search(
    conn: &Arc<Mutex<Connection>>,
    query: &str,
    target: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, AppError> {
    let fts_query = sanitize_fts_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }
    let conn = conn.lock().await;
    let mut rows = if let Some(t) = target {
        conn.query(
            r#"
            SELECT c.target, c.doc_kind, c.doc_name, c.field_name, c.field_role, c.text,
                   bm25(column_index_fts) AS rank
            FROM column_index_fts
            JOIN column_index c ON c.id = column_index_fts.row_id
            WHERE column_index_fts MATCH ?1 AND c.target = ?2
            ORDER BY rank
            LIMIT ?3
            "#,
            params![fts_query, t, limit as i64],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        conn.query(
            r#"
            SELECT c.target, c.doc_kind, c.doc_name, c.field_name, c.field_role, c.text,
                   bm25(column_index_fts) AS rank
            FROM column_index_fts
            JOIN column_index c ON c.id = column_index_fts.row_id
            WHERE column_index_fts MATCH ?1
            ORDER BY rank
            LIMIT ?2
            "#,
            params![fts_query, limit as i64],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let rank: f64 = row.get(6).unwrap_or(0.0);
        let score = bm25_rank_to_score(rank);
        out.push(SearchHit {
            target: row.get(0).map_err(|e| AppError::Internal(e.into()))?,
            doc_kind: row.get(1).map_err(|e| AppError::Internal(e.into()))?,
            doc_name: row.get(2).map_err(|e| AppError::Internal(e.into()))?,
            field_name: row.get(3).map_err(|e| AppError::Internal(e.into()))?,
            field_role: row.get(4).map_err(|e| AppError::Internal(e.into()))?,
            text: row.get(5).map_err(|e| AppError::Internal(e.into()))?,
            score,
            mode: "keyword".into(),
        });
    }
    Ok(out)
}

async fn vector_search(
    conn: &Arc<Mutex<Connection>>,
    query: &str,
    target: Option<&str>,
    limit: usize,
    embedder: Option<&dyn Embedder>,
) -> Result<Vec<SearchHit>, AppError> {
    let embedder = embedder.ok_or_else(|| {
        AppError::Validation("vector search requires embedding provider".into())
    })?;
    let qv = embedder.embed(&[query.to_string()]).await?.remove(0);
    let conn = conn.lock().await;
    let mut rows = if let Some(t) = target {
        conn.query(
            "SELECT target, doc_kind, doc_name, field_name, field_role, text, embedding FROM column_index WHERE embedding IS NOT NULL AND target = ?1",
            params![t],
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    } else {
        conn.query(
            "SELECT target, doc_kind, doc_name, field_name, field_role, text, embedding FROM column_index WHERE embedding IS NOT NULL",
            (),
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    };
    let mut scored = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let blob: Vec<u8> = row.get(6).map_err(|e| AppError::Internal(e.into()))?;
        let emb = blob_to_f32(&blob);
        let score = cosine(&qv, &emb);
        scored.push(SearchHit {
            target: row.get(0).map_err(|e| AppError::Internal(e.into()))?,
            doc_kind: row.get(1).map_err(|e| AppError::Internal(e.into()))?,
            doc_name: row.get(2).map_err(|e| AppError::Internal(e.into()))?,
            field_name: row.get(3).map_err(|e| AppError::Internal(e.into()))?,
            field_role: row.get(4).map_err(|e| AppError::Internal(e.into()))?,
            text: row.get(5).map_err(|e| AppError::Internal(e.into()))?,
            score,
            mode: "vector".into(),
        });
    }
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit);
    Ok(scored)
}

fn rrf_fuse(a: Vec<SearchHit>, b: Vec<SearchHit>, limit: usize) -> Vec<SearchHit> {
    let mut scores: HashMap<String, (SearchHit, f32)> = HashMap::new();
    for (rank, hit) in a.into_iter().enumerate() {
        let key = hit_key(&hit);
        let add = 1.0 / (RRF_K + rank as f32 + 1.0);
        scores
            .entry(key)
            .and_modify(|(_, s)| *s += add)
            .or_insert((hit, add));
    }
    for (rank, hit) in b.into_iter().enumerate() {
        let key = hit_key(&hit);
        let add = 1.0 / (RRF_K + rank as f32 + 1.0);
        scores
            .entry(key)
            .and_modify(|(h, s)| {
                *s += add;
                h.mode = "hybrid".into();
            })
            .or_insert_with(|| {
                let mut h = hit;
                h.mode = "hybrid".into();
                (h, add)
            });
    }
    let mut out: Vec<_> = scores
        .into_values()
        .map(|(mut hit, score)| {
            hit.score = score;
            hit.mode = "hybrid".into();
            hit
        })
        .collect();
    out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit);
    out
}

fn hit_key(h: &SearchHit) -> String {
    format!(
        "{}:{}:{}:{}",
        h.doc_kind, h.doc_name, h.field_name, h.field_role
    )
}

/// Keep ASCII ident chars and Unicode letters (incl. Hangul) for FTS tokens (D14).
fn sanitize_fts_query(q: &str) -> String {
    q.split_whitespace()
        .map(|t| {
            t.chars()
                .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || c.is_alphabetic())
                .collect::<String>()
        })
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Append character bigrams over non-ASCII letter runs for better Korean BM25 (D14).
fn augment_fts_text(text: &str) -> String {
    let mut out = text.to_string();
    let mut run = String::new();
    let flush = |run: &mut String, out: &mut String| {
        if run.chars().count() >= 2 {
            let chars: Vec<char> = run.chars().collect();
            for w in chars.windows(2) {
                out.push(' ');
                out.push(w[0]);
                out.push(w[1]);
            }
        }
        run.clear();
    };
    for c in text.chars() {
        if c.is_alphabetic() && !c.is_ascii() {
            run.push(c);
        } else {
            flush(&mut run, &mut out);
        }
    }
    flush(&mut run, &mut out);
    out
}

/// DW agent surface: `{ database, table, kind, name, description, score }`.
/// `field_role=document` → `kind=table`. Metric/segment index rows use `doc_name` as cube (`table`).
pub fn hits_to_dw_json(hits: &[SearchHit]) -> Value {
    json!({
        "items": hits.iter().map(hit_to_dw_item).collect::<Vec<_>>()
    })
}

fn hit_to_dw_item(h: &SearchHit) -> Value {
    let (kind, table, name) = match h.field_role.as_str() {
        "document" => ("table", h.doc_name.as_str(), h.doc_name.as_str()),
        role => (role, h.doc_name.as_str(), h.field_name.as_str()),
    };
    json!({
        "database": h.target,
        "table": table,
        "kind": kind,
        "name": name,
        "description": description_from_index_text(name, &h.text),
        "score": h.score,
    })
}

fn description_from_index_text(name: &str, text: &str) -> String {
    let prefix = format!("{name} ");
    if let Some(rest) = text.strip_prefix(&prefix) {
        rest.to_string()
    } else if text == name {
        String::new()
    } else {
        text.to_string()
    }
}

/// Keep hit if the referenced cube/member is visible to `caller` role.
pub fn hit_visible_for_role(hit: &SearchHit, docs: &[Value], caller: Option<&str>) -> bool {
    use crate::compile::check_role_access;
    match hit.field_role.as_str() {
        "document" => {
            let Some(cube) = find_doc(docs, "cube", &hit.doc_name) else {
                return false;
            };
            check_role_access(cube.get("role"), caller).is_ok()
        }
        "dimension" | "fact" => {
            let Some(cube) = find_doc(docs, "cube", &hit.doc_name) else {
                return false;
            };
            if check_role_access(cube.get("role"), caller).is_err() {
                return false;
            }
            let key = if hit.field_role == "dimension" {
                "dimensions"
            } else {
                "facts"
            };
            let Some(items) = cube.get(key).and_then(|v| v.as_array()) else {
                return false;
            };
            items.iter().any(|m| {
                m.get("name").and_then(|v| v.as_str()) == Some(hit.field_name.as_str())
                    && check_role_access(m.get("role"), caller).is_ok()
            })
        }
        "metric" | "segment" => {
            let Some(doc) = find_doc(docs, &hit.field_role, &hit.field_name) else {
                return false;
            };
            let cube_name = doc.get("cube").and_then(|v| v.as_str()).unwrap_or("");
            if !cube_name.is_empty() {
                if let Some(cube) = find_doc(docs, "cube", cube_name) {
                    if check_role_access(cube.get("role"), caller).is_err() {
                        return false;
                    }
                }
            }
            check_role_access(doc.get("role"), caller).is_ok()
        }
        _ => true,
    }
}

fn find_doc<'a>(docs: &'a [Value], kind: &str, name: &str) -> Option<&'a Value> {
    docs.iter().find(|d| {
        d.get("kind").and_then(|v| v.as_str()) == Some(kind)
            && d.get("name").and_then(|v| v.as_str()) == Some(name)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::HashEmbedder;
    use crate::semantic::SemanticStore;
    use std::sync::Arc;

    #[test]
    fn bm25_negative_ranks_produce_positive_monotonic_scores() {
        let s_strong = bm25_rank_to_score(-12.5);
        let s_medium = bm25_rank_to_score(-1.0);
        let s_weak = bm25_rank_to_score(-0.2);
        assert!(s_strong > 0.0 && s_strong <= 1.0);
        assert!(s_medium > 0.0 && s_medium.is_finite());
        assert!(s_weak > 0.0);
        assert!(s_strong > s_medium && s_medium > s_weak);
    }

    #[tokio::test]
    async fn table_columns_are_not_indexed() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        let customer: Value =
            serde_json::from_str(include_str!("../../semantic/examples/tables/customer.json"))
                .unwrap();
        store.create(customer).await.unwrap();
        let hits = store.search_columns("region", "keyword", 10).await.unwrap();
        assert!(hits.is_empty(), "hits={hits:?}");
    }

    #[tokio::test]
    async fn keyword_and_hybrid_find_dimension_and_filter_by_target() {
        let embedder = Arc::new(HashEmbedder::new(16));
        let store = SemanticStore::open(":memory:")
            .await
            .unwrap()
            .with_embedder(embedder.clone());
        for path in [
            include_str!("../../semantic/examples/tables/customer.json"),
            include_str!("../../semantic/examples/tables/region_codes.json"),
            include_str!("../../semantic/examples/tables/status_codes.json"),
            include_str!("../../semantic/examples/tables/orders.json"),
            include_str!("../../semantic/examples/views/orders_customer.json"),
            include_str!("../../semantic/examples/cubes/sales.json"),
        ] {
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
        let hits = store.search_columns("region", "keyword", 10).await.unwrap();
        assert!(
            hits.iter()
                .any(|h| h.field_name == "region_code" && h.field_role == "dimension" && h.target == "dw"),
            "hits={hits:?}"
        );
        assert!(hits.iter().all(|h| h.score > 0.0 && h.score.is_finite()));
        assert!(hits.iter().any(|h| h.text.contains("code")));
        let hybrid = store.search_columns("region", "hybrid", 10).await.unwrap();
        assert!(!hybrid.is_empty());

        let filtered_ok = store
            .search_columns_for_target("region", Some("dw"), "hybrid", 10)
            .await
            .unwrap();
        assert!(!filtered_ok.is_empty());
        let filtered_empty = store
            .search_columns_for_target("region", Some("other_target"), "hybrid", 10)
            .await
            .unwrap();
        assert!(filtered_empty.is_empty());
    }

    #[tokio::test]
    async fn without_embedder_hybrid_falls_back_to_keyword() {
        let store = SemanticStore::open(":memory:").await.unwrap();
        for path in [
            include_str!("../../semantic/examples/tables/customer.json"),
            include_str!("../../semantic/examples/tables/region_codes.json"),
            include_str!("../../semantic/examples/tables/status_codes.json"),
            include_str!("../../semantic/examples/tables/orders.json"),
            include_str!("../../semantic/examples/views/orders_customer.json"),
            include_str!("../../semantic/examples/cubes/sales.json"),
        ] {
            store
                .create(serde_json::from_str(path).unwrap())
                .await
                .unwrap();
        }
        let hits = store.search_columns("region", "hybrid", 10).await.unwrap();
        assert!(
            hits.iter().any(|h| h.field_name == "region_code"),
            "hits={hits:?}"
        );
        assert!(hits.iter().all(|h| h.mode == "keyword"));
    }

    #[test]
    fn extract_skips_expression_and_table() {
        let table: Value =
            serde_json::from_str(include_str!("../../semantic/examples/tables/customer.json"))
                .unwrap();
        assert!(extract_index_rows(&table).is_empty());

        let metric: Value =
            serde_json::from_str(include_str!("../../semantic/examples/metrics/revenue.json"))
                .unwrap();
        assert!(
            extract_index_rows(&metric).is_empty(),
            "D26: metric/segment are not search columns"
        );
        let table_hit = hit_to_dw_item(&SearchHit {
            target: "dw".into(),
            doc_kind: "cube".into(),
            doc_name: "sales".into(),
            field_name: "sales".into(),
            field_role: "document".into(),
            text: "sales 주문과 고객을 묶은 매출.".into(),
            score: 0.5,
            mode: "keyword".into(),
        });
        assert_eq!(table_hit["kind"], "table");
        assert_eq!(table_hit["table"], "sales");
        assert_eq!(table_hit["name"], "sales");

        let cube: Value =
            serde_json::from_str(include_str!("../../semantic/examples/cubes/sales.json")).unwrap();
        let rows = extract_index_rows(&cube);
        assert!(rows.iter().any(|r| r.field_role == "document" && r.text.contains("매출")));
        let dim = rows
            .iter()
            .find(|r| r.field_name == "region_code")
            .expect("dim");
        assert_eq!(dim.text, "region_code 판매 지역 코드. code");
        let fact = rows
            .iter()
            .find(|r| r.field_name == "sale_amount")
            .expect("fact");
        assert_eq!(fact.text, "sale_amount 판매 금액(원 통화 전). amount");
    }

    #[test]
    fn sanitize_keeps_hangul() {
        assert_eq!(sanitize_fts_query("매출 지역"), "매출 지역");
        assert_eq!(sanitize_fts_query("region_code"), "region_code");
    }

    #[test]
    fn augment_adds_hangul_bigrams() {
        let t = augment_fts_text("주문일자");
        assert!(t.contains("주문일자"));
        assert!(t.contains("주문"));
        assert!(t.contains("문일"));
        assert!(t.contains("일자"));
    }
}
