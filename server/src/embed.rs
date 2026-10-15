use async_trait::async_trait;

use crate::error::AppError;

/// D14: embedding provider. API when configured; deterministic for tests/offline.
#[async_trait]
pub trait Embedder: Send + Sync {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, AppError>;
    fn dimensions(&self) -> usize;
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
