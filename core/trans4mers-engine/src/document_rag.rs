use crate::app_state::AppState;
use crate::document_chunker::DocumentChunker;
use chrono::Utc;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::document::{DocChunk, DocIngestState, DocSearchResult};
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::ids::ProjectId;
use trans4mers_storage::filesystem::sha256_hex;
use trans4mers_storage::lancedb_store::LanceDbStore;
use trans4mers_storage::repos::{document_repo, project_repo, settings_repo};
use trans4mers_storage::vector_store::{SqliteVecStore, VectorRecord, VectorStore};

pub struct DocumentRagEngine;

impl DocumentRagEngine {
    /// Ingests workspace documents into the RAG pipeline.
    /// If `paths` is None, scans the entire workspace respecting standard ignore rules.
    pub async fn ingest_project(
        app_state: &AppState,
        project_id: &ProjectId,
        paths: Option<Vec<String>>,
    ) -> Result<usize, Trans4mersError> {
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        // 1. Resolve workspace root
        let mut workspace_path_buf = None;
        let _ = app_state.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) = project_repo::get_project(conn, project_id) {
                workspace_path_buf = Some(PathBuf::from(proj.workspace_path));
            }
            Ok(())
        });

        let workspace_root = match workspace_path_buf {
            Some(p) => p,
            None => {
                return Err(Trans4mersError::Internal(
                    "Workspace root not found".to_string(),
                ));
            }
        };

        // 2. Discover files to ingest
        let target_files: Vec<PathBuf> = if let Some(explicit_paths) = paths {
            explicit_paths
                .into_iter()
                .map(|p| workspace_root.join(p))
                .filter(|p| p.is_file())
                .collect()
        } else {
            Self::walk_directory(&workspace_root)
        };

        let provider = app_state.provider_registry.get_default().ok();
        let mut total_chunks_ingested = 0;

        for abs_path in target_files {
            let rel_path = match abs_path.strip_prefix(&workspace_root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => continue,
            };

            // Skip files larger than 2MB
            if let Ok(meta) = std::fs::metadata(&abs_path)
                && meta.len() > 2 * 1024 * 1024
            {
                continue;
            }

            // Read file text
            let content = match std::fs::read_to_string(&abs_path) {
                Ok(c) => c,
                Err(_) => continue, // Ignore binary or unreadable files
            };

            if content.trim().is_empty() {
                continue;
            }

            let hash = sha256_hex(content.as_bytes());

            let proj_id_str = project_id.to_string();

            // Check if file is already indexed with identical hash
            let is_unchanged = db
                .with_read_conn(|conn| {
                    if let Ok(Some(state)) =
                        document_repo::get_ingest_state(conn, &proj_id_str, &rel_path)
                        && state.content_hash == hash
                        && state.chunk_count > 0
                    {
                        return Ok(true);
                    }
                    Ok(false)
                })
                .unwrap_or(false);

            if is_unchanged {
                continue;
            }

            // Chunk document
            let mut chunks = DocumentChunker::chunk(&proj_id_str, &rel_path, &content, &hash);
            if chunks.is_empty() {
                continue;
            }

            // Compute vector embeddings if provider is active
            let mut embeddings: Vec<(String, Vec<f32>)> = Vec::new();
            if let Some(ref p) = provider {
                let model_config = ModelConfig::default();
                for c in &mut chunks {
                    if let Ok(emb) = p.embed(&c.text, &model_config).await {
                        c.embedding = Some(emb.clone());
                        embeddings.push((c.chunk_id.clone(), emb));
                    }
                }
            }

            if embeddings.is_empty() {
                tracing::debug!(
                    file = %rel_path,
                    "Vector embeddings omitted (provider absent or offline); chunk indexed via FTS5 BM25"
                );
            }

            let chunk_count = chunks.len();
            let now = Utc::now();

            let vector_backend = db
                .with_read_conn(settings_repo::get_vector_backend)
                .unwrap_or_else(|_| "sqlite-vec".to_string());

            // Atomically write chunks, embeddings, and ingest state
            db.with_write_tx(|tx| {
                document_repo::delete_chunks_by_file(tx, &proj_id_str, &rel_path)?;
                document_repo::insert_chunk_batch(tx, &chunks)?;

                if !embeddings.is_empty() {
                    let vec_store = SqliteVecStore::new();
                    for (chunk_id, emb) in &embeddings {
                        let _ = vec_store.upsert_embedding(tx, "vec_doc_chunks", chunk_id, emb);
                    }
                }

                let state = DocIngestState {
                    project_id: project_id.to_string(),
                    file_path: rel_path.clone(),
                    content_hash: hash,
                    chunk_count,
                    updated_at: now,
                };
                document_repo::upsert_ingest_state(tx, &state)?;

                Ok(())
            })?;

            if vector_backend == "lancedb" && !embeddings.is_empty() {
                let lance_dir = workspace_root
                    .join(".trans4mers")
                    .join("lancedb")
                    .join(&proj_id_str);
                let lance_store = LanceDbStore::new(lance_dir);
                let records: Vec<VectorRecord> = embeddings
                    .iter()
                    .map(|(id, emb)| VectorRecord {
                        id: id.clone(),
                        vector: emb.clone(),
                        metadata: None,
                    })
                    .collect();
                let _ = lance_store.batch_upsert("doc_chunks", &records).await;
            }

            total_chunks_ingested += chunk_count;
        }

        info!(project_id = %project_id, count = total_chunks_ingested, "Indexed document chunks in RAG pipeline");
        Ok(total_chunks_ingested)
    }

    /// Performs hybrid search (BM25 sparse via SQLite FTS5 + sqlite-vec dense) fused with Reciprocal Rank Fusion (RRF k=60).
    /// If dense embeddings are unavailable, gracefully degrades to pure SQLite FTS5 BM25 search without errors.
    pub async fn search(
        app_state: &AppState,
        project_id: &ProjectId,
        query: &str,
        limit: usize,
        file_pattern: Option<&str>,
    ) -> Result<Vec<DocSearchResult>, Trans4mersError> {
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        let limit = limit.clamp(1, 50);
        let proj_str = project_id.to_string();

        let mut workspace_path_buf = None;
        let _ = app_state.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) = project_repo::get_project(conn, project_id) {
                workspace_path_buf = Some(PathBuf::from(proj.workspace_path));
            }
            Ok(())
        });

        // 1. Dense retrieval via active vector backend (sqlite-vec or LanceDB)
        let mut dense_ranked: Vec<String> = Vec::new();
        if let Ok(provider) = app_state.provider_registry.get_default() {
            let model_config = ModelConfig::default();
            if let Ok(query_emb) = provider.embed(query, &model_config).await {
                let vector_backend = db
                    .with_read_conn(settings_repo::get_vector_backend)
                    .unwrap_or_else(|_| "sqlite-vec".to_string());

                if vector_backend == "lancedb"
                    && let Some(w_root) = workspace_path_buf.as_ref()
                {
                    let lance_dir = w_root.join(".trans4mers").join("lancedb").join(&proj_str);
                    let lance_store = LanceDbStore::new(lance_dir);
                    if let Ok(matches) = lance_store
                        .search_similar("doc_chunks", &query_emb, limit * 2, None, None)
                        .await
                    {
                        dense_ranked = matches.into_iter().map(|m| m.id).collect();
                    }
                } else {
                    let dense_res = db.with_read_conn(|conn| {
                        let vec_store = SqliteVecStore::new();
                        vec_store.search_similar(
                            conn,
                            "vec_doc_chunks",
                            &query_emb,
                            (limit * 2) as u32,
                        )
                    });
                    if let Ok(ids) = dense_res {
                        dense_ranked = ids;
                    }
                }
            } else {
                tracing::info!(
                    "Embedding unavailable for document search query; proceeding with FTS5 BM25"
                );
            }
        }

        // 2. Sparse retrieval via SQLite FTS5 BM25
        let sanitized = trans4mers_storage::fts5::sanitize_fts5_prefix_query(query);
        let mut fts5_chunk_map: HashMap<String, DocChunk> = HashMap::new();
        let mut sparse_ranked: Vec<String> = Vec::new();

        if !sanitized.is_empty() {
            let sparse_res = db.with_read_conn(|conn| {
                document_repo::search_chunks_fts5(
                    conn,
                    &proj_str,
                    &sanitized,
                    file_pattern,
                    limit * 2,
                )
            });

            if let Ok(fts_matches) = sparse_res {
                for (chunk, _bm25_score) in fts_matches {
                    let id = chunk.chunk_id.clone();
                    fts5_chunk_map.insert(id.clone(), chunk);
                    sparse_ranked.push(id);
                }
            }
        }

        // 3. Reciprocal Rank Fusion (RRF with k = 60)
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();
        for (rank, chunk_id) in dense_ranked.iter().enumerate() {
            let score = 1.0 / (60.0 + rank as f32);
            *rrf_scores.entry(chunk_id.clone()).or_insert(0.0) += score;
        }

        for (rank, chunk_id) in sparse_ranked.iter().enumerate() {
            let score = 1.0 / (60.0 + rank as f32);
            *rrf_scores.entry(chunk_id.clone()).or_insert(0.0) += score;
        }

        // Fallback: if both dense and FTS5 prefix returned empty, try exact sanitize
        if rrf_scores.is_empty() && !query.trim().is_empty() {
            let exact_sanitized = trans4mers_storage::fts5::sanitize_fts5_query(query);
            if !exact_sanitized.is_empty() && exact_sanitized != sanitized {
                let fallback_res = db.with_read_conn(|conn| {
                    document_repo::search_chunks_fts5(
                        conn,
                        &proj_str,
                        &exact_sanitized,
                        file_pattern,
                        limit,
                    )
                });
                if let Ok(matches) = fallback_res {
                    for (rank, (chunk, _)) in matches.into_iter().enumerate() {
                        let id = chunk.chunk_id.clone();
                        fts5_chunk_map.insert(id.clone(), chunk);
                        rrf_scores.insert(id, 1.0 / (60.0 + rank as f32));
                    }
                }
            }
        }

        let mut sorted_candidates: Vec<(String, f32)> = rrf_scores.into_iter().collect();
        sorted_candidates
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_candidates: Vec<(String, f32)> =
            sorted_candidates.into_iter().take(limit).collect();

        // 4. Resolve chunks into DocSearchResult
        let mut results = Vec::new();
        let mut missing_ids = Vec::new();

        for (chunk_id, score) in &top_candidates {
            if let Some(chunk) = fts5_chunk_map.get(chunk_id) {
                let snippet = if chunk.text.chars().count() > 300 {
                    let truncated: String = chunk.text.chars().take(300).collect();
                    format!("{}...", truncated)
                } else {
                    chunk.text.clone()
                };

                results.push(DocSearchResult {
                    chunk_id: chunk.chunk_id.clone(),
                    file_path: chunk.file_path.clone(),
                    line_start: chunk.line_start,
                    line_end: chunk.line_end,
                    snippet,
                    kind: chunk.kind.clone(),
                    score: *score,
                });
            } else {
                missing_ids.push((chunk_id.clone(), *score));
            }
        }

        if !missing_ids.is_empty() {
            db.with_read_conn(|conn| {
                for (chunk_id, score) in missing_ids {
                    if let Ok(Some(chunk)) = document_repo::get_chunk_by_id(conn, &chunk_id) {
                        let snippet = if chunk.text.chars().count() > 300 {
                            let truncated: String = chunk.text.chars().take(300).collect();
                            format!("{}...", truncated)
                        } else {
                            chunk.text.clone()
                        };

                        results.push(DocSearchResult {
                            chunk_id,
                            file_path: chunk.file_path,
                            line_start: chunk.line_start,
                            line_end: chunk.line_end,
                            snippet,
                            kind: chunk.kind,
                            score,
                        });
                    }
                }
                Ok(())
            })?;
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(results)
    }

    /// Lists all indexed files in the project.
    pub fn list_ingested_files(
        app_state: &AppState,
        project_id: &ProjectId,
    ) -> Result<Vec<DocIngestState>, Trans4mersError> {
        let db = app_state
            .get_project_db(project_id)
            .ok_or_else(|| Trans4mersError::Database("Project database not found".to_string()))?;

        let proj_str = project_id.to_string();
        db.with_read_conn(|conn| document_repo::list_ingest_states(conn, &proj_str))
    }

    /// Recursively walks workspace directory, filtering ignored paths.
    fn walk_directory(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let mut stack = vec![root.to_path_buf()];

        while let Some(dir) = stack.pop() {
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = entry.file_name().to_string_lossy().to_string();

                // Standard ignore filters
                if file_name.starts_with('.')
                    || file_name == "node_modules"
                    || file_name == "target"
                    || file_name == "dist"
                    || file_name == "build"
                    || file_name == "vendor"
                    || file_name == ".trans4mers"
                    || file_name == ".next"
                {
                    continue;
                }

                if path.is_dir() {
                    stack.push(path);
                } else if path.is_file() {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();
                    // Exclude common binary files
                    let is_binary = matches!(
                        ext.as_str(),
                        "png"
                            | "jpg"
                            | "jpeg"
                            | "gif"
                            | "ico"
                            | "svg"
                            | "webp"
                            | "exe"
                            | "dll"
                            | "so"
                            | "dylib"
                            | "bin"
                            | "wasm"
                            | "zip"
                            | "tar"
                            | "gz"
                            | "7z"
                            | "rar"
                            | "pdf"
                            | "doc"
                            | "docx"
                            | "xls"
                            | "xlsx"
                            | "db"
                            | "sqlite"
                            | "sqlite3"
                    );

                    if !is_binary {
                        files.push(path);
                    }
                }
            }
        }

        files
    }
}
