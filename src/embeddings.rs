//! Embedding generation and semantic search for notes.
//!
//! This module provides:
//! - **Backends**: Generate embeddings via ONNX (local) or Ollama (server)
//! - **Cache**: Content-addressed storage of embeddings in redb (`.kbase/embeddings.redb`)
//! - **Store**: In-memory index for similarity search across note chunks
//! - **Chunking**: Split notes by headers or paragraphs for granular search
//!
//! Used by the LSP's semantic search and `kbase search` command.

use anyhow::{Context, Result};
use ndarray::{Array1, Array2, ArrayViewD};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use redb::{Database, TableDefinition};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use tokenizers::Tokenizer;

// redb table: hash (string) -> embedding bytes
const EMBEDDINGS_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("embeddings");

/// A chunk of text with its embedding
#[derive(Debug, Clone)]
pub struct Chunk {
    pub note_path: String,
    pub header_path: Vec<String>, // e.g., ["Recipe", "Ingredients"]
    pub text: String,
    pub embedding: Vec<f32>,
    pub line: u32, // 0-indexed line number where chunk starts
}

/// Trait for embedding backends (Ollama, Mistral, OpenAI, etc.)
pub trait EmbeddingBackend: Send + Sync {
    fn embed(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
    fn dimension(&self) -> usize;
}

/// Ollama backend using local Ollama server
pub struct OllamaBackend {
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct OllamaEmbedRequest<'a> {
    model: &'a str,
    input: &'a str,
}

#[derive(Deserialize)]
struct OllamaEmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaBackend {
    pub fn new(model: Option<&str>) -> Result<Self> {
        let base_url =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = model.unwrap_or("all-minilm").to_string();

        // Check if Ollama is running
        let health_url = format!("{}/api/tags", base_url);
        ureq::get(&health_url)
            .call()
            .context("Ollama not running. Start with: ollama serve")?;

        Ok(Self { base_url, model })
    }
}

impl EmbeddingBackend for OllamaBackend {
    fn embed(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/api/embed", self.base_url);
        let mut results = Vec::new();

        // TODO: Ollama's /api/embed supports batch input - could send all texts
        // in one request instead of looping. Would reduce HTTP overhead.
        for text in texts {
            let request = OllamaEmbedRequest {
                model: &self.model,
                input: text,
            };

            let response: OllamaEmbedResponse = ureq::post(&url)
                .send_json(&request)
                .context("Failed to call Ollama embed API")?
                .into_json()
                .context("Failed to parse Ollama response")?;

            match response.embeddings.into_iter().next() {
                Some(embedding) => results.push(embedding),
                None => anyhow::bail!("No embedding returned from Ollama"),
            }
        }

        Ok(results)
    }

    fn dimension(&self) -> usize {
        384 // all-minilm default
    }
}

/// ONNX backend using local model files
pub struct OnnxBackend {
    session: Session,
    tokenizer: Tokenizer,
}

impl OnnxBackend {
    /// Find model directory: checks ./models, vault/models, then ~/.cache/kbase/models
    pub fn find_model_dir(vault_path: &Path) -> Result<std::path::PathBuf> {
        let has_model = |dir: &Path| -> bool {
            dir.join("model.onnx").exists() && dir.join("tokenizer.json").exists()
        };

        // Current working directory: ./models
        let cwd_models = Path::new("models");
        if has_model(cwd_models) {
            return Ok(cwd_models.to_path_buf());
        }

        // Vault-local: models/ at vault root
        let vault_models = vault_path.join("models");
        if has_model(&vault_models) {
            return Ok(vault_models);
        }

        // Global fallback: $XDG_CACHE_HOME/kbase/models or ~/.cache/kbase/models
        let cache_dir = std::env::var_os("XDG_CACHE_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".cache")));

        match cache_dir {
            Some(cache) => {
                let global_dir = cache.join("kbase/models");
                if has_model(&global_dir) {
                    return Ok(global_dir);
                }
            }
            None => {}
        }

        // Neither found - show helpful error with XDG path
        let cache_path = std::env::var("XDG_CACHE_HOME")
            .unwrap_or_else(|_| std::env::var("HOME").unwrap_or_default() + "/.cache");

        anyhow::bail!(
            "Model files not found. Download with:\n  \
             mkdir -p {0}/kbase/models\n  \
             curl -L -o {0}/kbase/models/model.onnx https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/onnx/model.onnx\n  \
             curl -L -o {0}/kbase/models/tokenizer.json https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main/tokenizer.json",
            cache_path
        );
    }

    /// Create backend from model directory containing model.onnx and tokenizer.json
    pub fn new(model_dir: &Path) -> Result<Self> {
        tracing::info!("Loading ONNX model from: {}", model_dir.display());
        let model_path = model_dir.join("model.onnx");
        let tokenizer_path = model_dir.join("tokenizer.json");

        if !model_path.exists() {
            anyhow::bail!(
                "Model not found at {}",
                model_path.display()
            );
        }

        if !tokenizer_path.exists() {
            anyhow::bail!(
                "Tokenizer not found at {}",
                tokenizer_path.display()
            );
        }

        let session = Session::builder()
            .context("Failed to create ONNX session builder")?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .context("Failed to set optimization level")?
            .commit_from_file(model_path)
            .context("Failed to load ONNX model")?;

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

        Ok(Self { session, tokenizer })
    }

    /// Aggregate per-token embeddings into a single sentence embedding.
    ///
    /// The transformer outputs one 384-dim vector per token, but each vector already
    /// encodes context from the full sentence (via self-attention layers). Mean pooling
    /// averages these context-aware vectors, then L2-normalizes for cosine similarity.
    ///
    /// The `attention_mask` indicates real tokens (1) vs padding (0). Models expect
    /// fixed-length input, so short texts are padded. The mask ensures we only average
    /// real tokens: `["Hello", "world", PAD, PAD]` with mask `[1, 1, 0, 0]` averages
    /// only the first two embeddings.
    ///
    /// Alternative approach: use only the [CLS] token embedding, which is trained to
    /// capture sentence-level meaning. Mean pooling is often more robust for similarity.
    fn mean_pooling(embeddings: ArrayViewD<f32>, attention_mask: &[i64]) -> Array1<f32> {
        let shape = embeddings.shape();
        let seq_len = shape[1];
        let hidden_dim = shape[2];

        let mut pooled = Array1::zeros(hidden_dim);
        let mut total_weight = 0.0f32;

        for i in 0..seq_len {
            let weight = attention_mask[i] as f32;
            if weight > 0.0 {
                for j in 0..hidden_dim {
                    pooled[j] += embeddings[[0, i, j]] * weight;
                }
                total_weight += weight;
            }
        }

        if total_weight > 0.0 {
            pooled.mapv_inplace(|x| x / total_weight);
        }

        // L2 normalize
        let norm: f32 = pooled.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            pooled.mapv_inplace(|x| x / norm);
        }

        pooled
    }
}

impl EmbeddingBackend for OnnxBackend {
    fn embed(&mut self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());

        // Process texts individually rather than batching. ONNX supports batching, but it
        // requires padding all texts to the longest one. With diverse chunk sizes (10 vs 500
        // tokens), padding overhead can outweigh batching benefits on CPU.
        for text in texts {
            let encoding = self
                .tokenizer
                .encode(*text, true)
                .map_err(|e| anyhow::anyhow!("Tokenization failed: {}", e))?;

            // Tokenizer returns u32, but ONNX models expect i64 (PyTorch convention)
            let input_ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
            let attention_mask: Vec<i64> = encoding.get_attention_mask().iter().map(|&x| x as i64).collect();
            let token_type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&x| x as i64).collect();

            let seq_len = input_ids.len();

            let input_ids_arr = Array2::from_shape_vec((1, seq_len), input_ids)?;
            let attention_mask_arr = Array2::from_shape_vec((1, seq_len), attention_mask.clone())?;
            let token_type_ids_arr = Array2::from_shape_vec((1, seq_len), token_type_ids)?;

            let outputs = self.session.run(ort::inputs![
                "input_ids" => TensorRef::from_array_view(input_ids_arr.view())?,
                "attention_mask" => TensorRef::from_array_view(attention_mask_arr.view())?,
                "token_type_ids" => TensorRef::from_array_view(token_type_ids_arr.view())?,
            ])?;

            // Get the last_hidden_state output (shape: [1, seq_len, 384])
            let embeddings: ArrayViewD<f32> = outputs[0]
                .try_extract_array()
                .context("Failed to extract embeddings")?;

            let pooled = Self::mean_pooling(embeddings, &attention_mask);

            results.push(pooled.to_vec());
        }

        Ok(results)
    }

    fn dimension(&self) -> usize {
        384 // all-MiniLM-L6-v2
    }
}

/// Content-addressable embedding cache using redb
pub struct EmbeddingCache {
    db: Database,
}

impl EmbeddingCache {
    /// Open or create cache at the given path
    pub fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        match path.parent() {
            Some(parent) => std::fs::create_dir_all(parent).context("Failed to create cache directory")?,
            None => {}  // no parent directory needed (e.g., path is just a filename)
        }
        let db = Database::create(path).context("Failed to open embedding cache")?;
        Ok(Self { db })
    }

    /// Compute content hash for a text
    pub fn content_hash(text: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.as_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Get embedding from cache, returns None if not cached
    pub fn get(&self, text: &str) -> Option<Vec<f32>> {
        let hash = Self::content_hash(text);
        let read_txn = self.db.begin_read().ok()?;
        let table = read_txn.open_table(EMBEDDINGS_TABLE).ok()?;
        let value = table.get(hash.as_str()).ok()??;
        // Deserialize bytes back to f32 (4 bytes each, little-endian)
        let bytes = value.value();
        Some(
            bytes
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
                .collect(),
        )
    }

    /// Store embedding in cache
    pub fn put(&self, text: &str, embedding: &[f32]) -> Result<()> {
        let hash = Self::content_hash(text);
        // Serialize f32 to bytes (4 bytes each, little-endian) for redb storage
        let bytes: Vec<u8> = embedding.iter().flat_map(|f| f.to_le_bytes()).collect();
        let write_txn = self.db.begin_write().context("Failed to begin write transaction")?;
        {
            let mut table = write_txn
                .open_table(EMBEDDINGS_TABLE)
                .context("Failed to open embeddings table")?;
            table
                .insert(hash.as_str(), bytes.as_slice())
                .context("Failed to insert embedding")?;
        }
        write_txn.commit().context("Failed to commit transaction")?;
        Ok(())
    }

    /// Get embedding, computing and caching if needed
    pub fn get_or_compute<B: EmbeddingBackend + ?Sized>(
        &self,
        text: &str,
        backend: &mut B,
    ) -> Result<Vec<f32>> {
        match self.get(text) {
            Some(embedding) => return Ok(embedding),
            None => {}  // continue to compute
        }

        // Not cached, compute and store
        let embeddings = backend.embed(&[text])?;
        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No embedding returned"))?;

        self.put(text, &embedding)?;
        Ok(embedding)
    }

    /// Batch get or compute embeddings
    pub fn get_or_compute_batch<B: EmbeddingBackend + ?Sized>(
        &self,
        texts: &[&str],
        backend: &mut B,
    ) -> Result<Vec<Vec<f32>>> {
        let mut results = Vec::with_capacity(texts.len());
        let mut to_compute: Vec<(usize, &str)> = Vec::new();

        // Check cache first
        for (i, text) in texts.iter().enumerate() {
            match self.get(text) {
                Some(embedding) => results.push((i, embedding)),
                None => to_compute.push((i, text)),
            }
        }

        // Compute missing embeddings
        if !to_compute.is_empty() {
            let texts_to_embed: Vec<&str> = to_compute.iter().map(|(_, t)| *t).collect();
            let computed = backend.embed(&texts_to_embed)?;

            for ((i, text), embedding) in to_compute.into_iter().zip(computed) {
                self.put(text, &embedding)?;
                results.push((i, embedding));
            }
        }

        // Sort by original index
        results.sort_by_key(|(i, _)| *i);
        Ok(results.into_iter().map(|(_, e)| e).collect())
    }
}

/// In-memory embedding store
pub struct EmbeddingStore {
    chunks: Vec<Chunk>,
}

impl EmbeddingStore {
    pub fn new() -> Self {
        Self { chunks: Vec::new() }
    }

    pub fn add_chunk(&mut self, chunk: Chunk) {
        self.chunks.push(chunk);
    }

    pub fn chunks(&self) -> &[Chunk] {
        &self.chunks
    }

    /// Find similar chunks by cosine similarity
    pub fn find_similar(&self, query_embedding: &[f32], limit: usize) -> Vec<(&Chunk, f32)> {
        let mut scored: Vec<(&Chunk, f32)> = self
            .chunks
            .iter()
            .map(|chunk| (chunk, cosine_similarity(query_embedding, &chunk.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }

    /// Find similar chunks, excluding a specific note
    pub fn find_similar_excluding(
        &self,
        query_embedding: &[f32],
        exclude_path: &str,
        limit: usize,
    ) -> Vec<(&Chunk, f32)> {
        let mut scored: Vec<(&Chunk, f32)> = self
            .chunks
            .iter()
            .filter(|chunk| chunk.note_path != exclude_path)
            .map(|chunk| (chunk, cosine_similarity(query_embedding, &chunk.embedding)))
            .collect();

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(limit);
        scored
    }

    /// Get chunks for a specific note
    pub fn chunks_for_note(&self, note_path: &str) -> Vec<&Chunk> {
        self.chunks
            .iter()
            .filter(|c| c.note_path == note_path)
            .collect()
    }

    /// Group chunks by note path
    pub fn chunks_by_note(&self) -> HashMap<&str, Vec<&Chunk>> {
        let mut map: HashMap<&str, Vec<&Chunk>> = HashMap::new();
        for chunk in &self.chunks {
            map.entry(&chunk.note_path).or_default().push(chunk);
        }
        map
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Chunk level configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChunkLevel {
    #[default]
    None,       // Full note
    H1,         // Split on #
    H2,         // Split on ##
    H3,         // Split on ###
    Paragraph,  // Split on blank lines
}

impl ChunkLevel {
    pub fn from_str(s: &str) -> Self {
        match s {
            "none" | "full" => ChunkLevel::None,
            "#" | "h1" => ChunkLevel::H1,
            "##" | "h2" => ChunkLevel::H2,
            "###" | "h3" => ChunkLevel::H3,
            "paragraph" | "para" => ChunkLevel::Paragraph,
            _ => ChunkLevel::None,
        }
    }
}

/// Split markdown content into chunks based on level
/// Returns (header_path, text, line_number) for each chunk
pub fn split_into_chunks(content: &str, level: ChunkLevel) -> Vec<(Vec<String>, String, u32)> {
    match level {
        ChunkLevel::None => vec![(vec![], content.to_string(), 0)],
        ChunkLevel::H1 => split_by_header(content, 1),
        ChunkLevel::H2 => split_by_header(content, 2),
        ChunkLevel::H3 => split_by_header(content, 3),
        ChunkLevel::Paragraph => split_by_paragraph(content),
    }
}

fn split_by_header(content: &str, level: usize) -> Vec<(Vec<String>, String, u32)> {
    let mut chunks = Vec::new();
    let mut current_headers: Vec<String> = Vec::new();
    let mut current_text = String::new();
    let mut current_start_line: u32 = 0;
    let mut line_number: u32 = 0;

    for line in content.lines() {
        let trimmed = line.trim_start();

        // Check if this line is a header at or above our level
        if trimmed.starts_with('#') {
            let header_level = trimmed.chars().take_while(|c| *c == '#').count();

            if header_level <= level {
                // Save previous chunk if non-empty
                if !current_text.trim().is_empty() {
                    chunks.push((current_headers.clone(), current_text.trim().to_string(), current_start_line));
                }

                // Update header path
                let header_text = trimmed[header_level..].trim().to_string();

                // Truncate headers to current level and add new one
                current_headers.truncate(header_level.saturating_sub(1));
                current_headers.push(header_text.clone());

                // Start new chunk with header included in text for better embeddings
                current_text = format!("{}\n", header_text);
                current_start_line = line_number;
                line_number += 1;
                continue;
            }
        }

        current_text.push_str(line);
        current_text.push('\n');
        line_number += 1;
    }

    // Don't forget the last chunk
    if !current_text.trim().is_empty() {
        chunks.push((current_headers, current_text.trim().to_string(), current_start_line));
    }

    // If no chunks were created, return the whole content
    if chunks.is_empty() {
        chunks.push((vec![], content.to_string(), 0));
    }

    chunks
}

fn split_by_paragraph(content: &str) -> Vec<(Vec<String>, String, u32)> {
    let mut chunks = Vec::new();
    let mut current_line: u32 = 0;

    for part in content.split("\n\n") {
        let trimmed = part.trim();
        if !trimmed.is_empty() {
            chunks.push((vec![], trimmed.to_string(), current_line));
        }
        // Count lines in this part plus the blank line separator
        current_line += part.matches('\n').count() as u32 + 2;
    }

    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b)).abs() < 0.001);
    }

    #[test]
    fn split_by_h2() {
        let content = "# Title\n\nIntro\n\n## Section 1\n\nContent 1\n\n## Section 2\n\nContent 2";
        let chunks = split_by_header(content, 2);
        assert_eq!(chunks.len(), 3);
        // Check line numbers
        assert_eq!(chunks[0].2, 0); // # Title at line 0
        assert_eq!(chunks[1].2, 4); // ## Section 1 at line 4
        assert_eq!(chunks[2].2, 8); // ## Section 2 at line 8
    }

    #[test]
    fn split_by_paragraph_basic() {
        let content = "Para 1\n\nPara 2\n\nPara 3";
        let chunks = split_by_paragraph(content);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].2, 0); // Para 1 at line 0
    }
}
