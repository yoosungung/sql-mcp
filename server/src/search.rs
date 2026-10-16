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
    pub doc_kind: String,
    pub doc_name: String,
    pub field_name: String,
    pub field_role: String,
    pub text: String,
    pub score: f32,
    pub mode: String,
}

pub async fn rebuild_index(
    conn: &Connection,
    docs: &[Value],
    embedder: Option<&dyn Embedder>,
) -> Result<(), AppError> {
    conn.execute("DELETE FROM column_index_fts", ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    conn.execute("DELETE FROM column_index", ())
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    let mut rows = Vec::new();
    for doc in docs {
        rows.extend(extract_index_rows(doc));
    }

    let embeddings = if let Some(embedder) = embedder {
        let texts: Vec<String> = rows.iter().map(|r| r.text.clone()).collect();
        if texts.is_empty() {
            vec![]
        } else {
            embedder.embed(&texts).await?
        }
    } else {
        vec![]
    };

    for (i, row) in rows.iter().enumerate() {
        let emb = embeddings.get(i).map(|v| f32_to_blob(v));
        conn.execute(
            "INSERT INTO column_index (doc_kind, doc_name, field_name, field_role, text, embedding)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                row.doc_kind.as_str(),
                row.doc_name.as_str(),
                row.field_name.as_str(),
                row.field_role.as_str(),
                row.text.as_str(),
                emb
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

struct IndexRow {
    doc_kind: String,
    doc_name: String,
    field_name: String,
    field_role: String,
    text: String,
}

fn name_desc_text(name: &str, doc: &Value) -> String {
    match doc.get("description").and_then(|v| v.as_str()) {
        Some(d) if !d.is_empty() => format!("{name} {d}"),
        _ => name.to_string(),
    }
}

/// D12: cube/dim/fact/metric/segment only — name (+ description); dim/fact add type.
fn extract_index_rows(doc: &Value) -> Vec<IndexRow> {
    let kind = doc.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    let name = doc.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let mut rows = Vec::new();
    match kind {
        "cube" => {
            rows.push(IndexRow {
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
                        let dtype = item.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        let text = if dtype.is_empty() {
                            fname.to_string()
                        } else {
                            format!("{fname} {dtype}")
                        };
                        rows.push(IndexRow {
                            doc_kind: kind.into(),
                            doc_name: name.into(),
                            field_name: fname.into(),
                            field_role: role.into(),
                            text,
                        });
                    }
                }
            }
        }
        "metric" | "segment" => {
            rows.push(IndexRow {
                doc_kind: kind.into(),
                doc_name: name.into(),
                field_name: name.into(),
                field_role: kind.into(),
                text: name_desc_text(name, doc),
            });
        }
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
    let mode = normalize_mode(mode, embedder.is_some());
    match mode.as_str() {
        "keyword" => keyword_search(conn, query, limit).await,
        "vector" => vector_search(conn, query, limit, embedder).await,
        "hybrid" => {
            let kw = keyword_search(conn, query, limit * 2).await?;
            let vec = if embedder.is_some() {
                vector_search(conn, query, limit * 2, embedder).await?
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

async fn keyword_search(
    conn: &Arc<Mutex<Connection>>,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>, AppError> {
    let fts_query = sanitize_fts_query(query);
    if fts_query.is_empty() {
        return Ok(vec![]);
    }
    let conn = conn.lock().await;
    let mut rows = conn
        .query(
            r#"
            SELECT c.doc_kind, c.doc_name, c.field_name, c.field_role, c.text,
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
        .map_err(|e| AppError::Internal(e.into()))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let rank: f64 = row.get(5).unwrap_or(0.0);
        // bm25: lower is better → convert to positive score
        let score = 1.0 / (1.0 + rank as f32);
        out.push(SearchHit {
            doc_kind: row.get(0).map_err(|e| AppError::Internal(e.into()))?,
            doc_name: row.get(1).map_err(|e| AppError::Internal(e.into()))?,
            field_name: row.get(2).map_err(|e| AppError::Internal(e.into()))?,
            field_role: row.get(3).map_err(|e| AppError::Internal(e.into()))?,
            text: row.get(4).map_err(|e| AppError::Internal(e.into()))?,
            score,
            mode: "keyword".into(),
        });
    }
    Ok(out)
}

async fn vector_search(
    conn: &Arc<Mutex<Connection>>,
    query: &str,
    limit: usize,
    embedder: Option<&dyn Embedder>,
) -> Result<Vec<SearchHit>, AppError> {
    let embedder = embedder.ok_or_else(|| {
        AppError::Validation("vector search requires embedding provider".into())
    })?;
    let qv = embedder.embed(&[query.to_string()]).await?.remove(0);
    let conn = conn.lock().await;
    let mut rows = conn
        .query(
            "SELECT doc_kind, doc_name, field_name, field_role, text, embedding FROM column_index WHERE embedding IS NOT NULL",
            (),
        )
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let mut scored = Vec::new();
    while let Some(row) = rows
        .next()
        .await
        .map_err(|e| AppError::Internal(e.into()))?
    {
        let blob: Vec<u8> = row.get(5).map_err(|e| AppError::Internal(e.into()))?;
        let emb = blob_to_f32(&blob);
        let score = cosine(&qv, &emb);
        scored.push(SearchHit {
            doc_kind: row.get(0).map_err(|e| AppError::Internal(e.into()))?,
            doc_name: row.get(1).map_err(|e| AppError::Internal(e.into()))?,
            field_name: row.get(2).map_err(|e| AppError::Internal(e.into()))?,
            field_role: row.get(3).map_err(|e| AppError::Internal(e.into()))?,
            text: row.get(4).map_err(|e| AppError::Internal(e.into()))?,
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

pub fn hits_to_json(hits: &[SearchHit]) -> Value {
    json!({
        "items": hits.iter().map(|h| json!({
            "doc_kind": h.doc_kind,
            "doc_name": h.doc_name,
            "field_name": h.field_name,
            "field_role": h.field_role,
            "text": h.text,
            "score": h.score,
            "mode": h.mode,
        })).collect::<Vec<_>>()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::embed::HashEmbedder;
    use crate::semantic::SemanticStore;
    use std::sync::Arc;

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
    async fn keyword_and_hybrid_find_dimension() {
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
                .any(|h| h.field_name == "region_code" && h.field_role == "dimension"),
            "hits={hits:?}"
        );
        assert!(hits.iter().any(|h| h.text.contains("code")));
        let hybrid = store.search_columns("region", "hybrid", 10).await.unwrap();
        assert!(!hybrid.is_empty());
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
        let rows = extract_index_rows(&metric);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].field_role, "metric");
        assert_eq!(rows[0].text, "revenue");
        assert!(!rows[0].text.contains("sum"));

        let cube: Value =
            serde_json::from_str(include_str!("../../semantic/examples/cubes/sales.json")).unwrap();
        let rows = extract_index_rows(&cube);
        assert!(rows.iter().any(|r| r.field_role == "document" && r.text.contains("매출")));
        let dim = rows
            .iter()
            .find(|r| r.field_name == "region_code")
            .expect("dim");
        assert_eq!(dim.text, "region_code code");
        let fact = rows
            .iter()
            .find(|r| r.field_name == "sale_amount")
            .expect("fact");
        assert_eq!(fact.text, "sale_amount amount");
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
