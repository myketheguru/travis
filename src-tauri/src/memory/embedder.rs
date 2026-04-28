use std::sync::OnceLock;

use anyhow::{anyhow, Context};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

static EMBEDDER: OnceLock<TextEmbedding> = OnceLock::new();

fn embedder() -> anyhow::Result<&'static TextEmbedding> {
    if let Some(e) = EMBEDDER.get() {
        return Ok(e);
    }
    // Initialize once (downloads ~130MB on first call). Subsequent calls reuse.
    let model = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::BGESmallENV15).with_show_download_progress(false),
    )
    .context("failed to initialize fastembed BGESmallENV15 model")?;
    // OnceLock::set returns Err if already set by a racing thread; either is fine.
    let _ = EMBEDDER.set(model);
    EMBEDDER
        .get()
        .ok_or_else(|| anyhow!("embedder OnceLock unexpectedly empty after set"))
}

/// Embed a batch of texts. Each returned vector has 384 f32 dims (bge-small).
pub fn embed(texts: &[&str]) -> anyhow::Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let e = embedder()?;
    // fastembed expects owned strings.
    let owned: Vec<String> = texts.iter().map(|t| (*t).to_string()).collect();
    let vectors = e
        .embed(owned, None)
        .context("fastembed embed() failed")?;
    Ok(vectors)
}

/// Embed a single text, returning its 384-dim f32 vector.
pub fn embed_one(text: &str) -> anyhow::Result<Vec<f32>> {
    let mut out = embed(&[text])?;
    out.pop()
        .ok_or_else(|| anyhow!("embedder returned no vectors"))
}

/// Encode an f32 slice as little-endian bytes for SQLite BLOB storage.
pub fn vec_to_bytes(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Decode little-endian f32 bytes back into a vector. Trailing bytes (if any) are ignored.
pub fn bytes_to_vec(b: &[u8]) -> Vec<f32> {
    let n = b.len() / 4;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let off = i * 4;
        let arr = [b[off], b[off + 1], b[off + 2], b[off + 3]];
        out.push(f32::from_le_bytes(arr));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bytes() {
        let v = vec![0.0_f32, 1.5, -2.25, std::f32::consts::PI];
        let b = vec_to_bytes(&v);
        assert_eq!(b.len(), v.len() * 4);
        let back = bytes_to_vec(&b);
        assert_eq!(v, back);
    }
}
