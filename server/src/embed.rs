use std::env;
use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;

use crate::error::AppError;

/// D14: embedding provider. API when configured; deterministic for tests/offline.
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError>;
    fn dimensions(&self) -> usize;
}

/// Build embedder from env: `SQL_MCP_EMBEDDING_URL` + `SQL_MCP_EMBEDDING_API_KEY` → OpenAI-compatible.
/// Otherwise HashEmbedder(dim from `SQL_MCP_EMBEDDING_DIM`, default 64).
pub fn from_env() -> Arc<dyn Embedder> {
    let dim = env::var("SQL_MCP_EMBEDDING_DIM")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64);
    match (
        env::var("SQL_MCP_EMBEDDING_URL").ok(),
        env::var("SQL_MCP_EMBEDDING_API_KEY").ok(),
    ) {
        (Some(url), Some(key)) if !url.is_empty() && !key.is_empty() => {
            let model = env::var("SQL_MCP_EMBEDDING_MODEL")
                .unwrap_or_else(|_| "text-embedding-3-small".into());
            Arc::new(OpenAiEmbedder::new(url, key, model, dim))
        }
        _ => Arc::new(HashEmbedder::new(dim)),
    }
}

pub struct OpenAiEmbedder {
    url: String,
    api_key: String,
    model: String,
    dim: usize,
    client: reqwest::Client,
}

impl OpenAiEmbedder {
    pub fn new(url: String, api_key: String, model: String, dim: usize) -> Self {
        Self {
            url,
            api_key,
            model,
            dim,
            client: reqwest::Client::new(),
        }
    }
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[async_trait]
impl Embedder for OpenAiEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        let body = serde_json::json!({
            "model": self.model,
            "input": texts,
        });
        let resp = self
            .client
            .post(&self.url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::Connection(e.into()))?;
        if !resp.status().is_success() {
            return Err(AppError::Connection(anyhow::anyhow!(
                "embedding HTTP {}",
                resp.status()
            )));
        }
        let parsed: EmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
        if parsed.data.len() != texts.len() {
            return Err(AppError::Internal(anyhow::anyhow!(
                "embedding count mismatch"
            )));
        }
        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }

    fn dimensions(&self) -> usize {
        self.dim
    }
}

/// Stable hash embedding for tests and when API is unset.
pub struct HashEmbedder {
    dim: usize,
}

impl HashEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(8) }
    }
}

#[async_trait]
impl Embedder for HashEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError> {
        Ok(texts.iter().map(|t| hash_vec(t, self.dim)).collect())
    }

    fn dimensions(&self) -> usize {
        self.dim
    }
}

fn hash_vec(text: &str, dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim];
    for (i, b) in text.bytes().enumerate() {
        let idx = (i + b as usize) % dim;
        out[idx] += (b as f32) / 255.0;
    }
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
    for v in &mut out {
        *v /= norm;
    }
    out
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-9 {
        0.0
    } else {
        dot / denom
    }
}

pub fn f32_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

pub fn blob_to_f32(blob: &[u8]) -> Vec<f32> {
    blob.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn similar_texts_closer() {
        let e = HashEmbedder::new(32);
        let a = e.embed(&[String::from("order_date sales")]).await.unwrap();
        let b = e.embed(&[String::from("order_date revenue")]).await.unwrap();
        let c = e.embed(&[String::from("zzzz unrelated")]).await.unwrap();
        assert!(cosine(&a[0], &b[0]) > cosine(&a[0], &c[0]));
    }
}
