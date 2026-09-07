use chrono::Utc;
use tiktoken_rs::cl100k_base;
use trans4mers_domain::document::DocChunk;
use uuid::Uuid;

pub struct DocumentChunker;

impl DocumentChunker {
    /// Chunks a document based on file extension and semantic structure.
    pub fn chunk(
        project_id: &str,
        file_path: &str,
        content: &str,
        content_hash: &str,
    ) -> Vec<DocChunk> {
        let ext = std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "md" | "markdown" | "mdx" => {
                Self::chunk_markdown(project_id, file_path, content, content_hash)
            }
            "rs" | "py" | "js" | "jsx" | "ts" | "tsx" | "go" | "c" | "cpp" | "h" | "hpp"
            | "java" | "json" | "yaml" | "yml" | "toml" | "sql" | "sh" | "ps1" => {
                Self::chunk_code(project_id, file_path, content, content_hash, &ext)
            }
            _ => Self::chunk_text(project_id, file_path, content, content_hash),
        }
    }

    /// Estimates token count using cl100k_base, fallback to chars/4.
    pub fn estimate_tokens(text: &str) -> usize {
        if let Ok(bpe) = cl100k_base() {
            bpe.encode_with_special_tokens(text).len()
        } else {
            text.chars().count().saturating_div(4).max(1)
        }
    }

    fn chunk_markdown(
        project_id: &str,
        file_path: &str,
        content: &str,
        content_hash: &str,
    ) -> Vec<DocChunk> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut current_chunk_lines = Vec::new();
        let mut start_line = 1;
        let mut ord = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let is_heading = line.starts_with("# ")
                || line.starts_with("## ")
                || line.starts_with("### ")
                || line.starts_with("#### ");

            // If heading encountered and accumulated lines exceed minimum threshold
            if is_heading && !current_chunk_lines.is_empty() {
                let text = current_chunk_lines.join("\n");
                let tokens = Self::estimate_tokens(&text);
                if tokens >= 80 {
                    chunks.push(DocChunk {
                        chunk_id: Uuid::new_v4().to_string(),
                        project_id: project_id.to_string(),
                        file_path: file_path.to_string(),
                        content_hash: content_hash.to_string(),
                        ord,
                        line_start: start_line,
                        line_end: line_num.saturating_sub(1),
                        text,
                        kind: "markdown".to_string(),
                        token_estimate: tokens,
                        created_at: Utc::now(),
                        embedding: None,
                    });
                    ord += 1;
                    current_chunk_lines.clear();
                    start_line = line_num;
                }
            }

            current_chunk_lines.push(*line);

            // Hard ceiling to avoid monstrous chunks
            let current_text = current_chunk_lines.join("\n");
            if Self::estimate_tokens(&current_text) > 800 {
                let tokens = Self::estimate_tokens(&current_text);
                chunks.push(DocChunk {
                    chunk_id: Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    file_path: file_path.to_string(),
                    content_hash: content_hash.to_string(),
                    ord,
                    line_start: start_line,
                    line_end: line_num,
                    text: current_text,
                    kind: "markdown".to_string(),
                    token_estimate: tokens,
                    created_at: Utc::now(),
                    embedding: None,
                });
                ord += 1;
                current_chunk_lines.clear();
                start_line = line_num + 1;
            }
        }

        if !current_chunk_lines.is_empty() {
            let text = current_chunk_lines.join("\n");
            let tokens = Self::estimate_tokens(&text);
            chunks.push(DocChunk {
                chunk_id: Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                file_path: file_path.to_string(),
                content_hash: content_hash.to_string(),
                ord,
                line_start: start_line,
                line_end: lines.len(),
                text,
                kind: "markdown".to_string(),
                token_estimate: tokens,
                created_at: Utc::now(),
                embedding: None,
            });
        }

        chunks
    }

    fn chunk_code(
        project_id: &str,
        file_path: &str,
        content: &str,
        content_hash: &str,
        _ext: &str,
    ) -> Vec<DocChunk> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut current_chunk_lines = Vec::new();
        let mut start_line = 1;
        let mut ord = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let trimmed = line.trim_start();
            let is_boundary = trimmed.starts_with("fn ")
                || trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub(crate) fn ")
                || trimmed.starts_with("async fn ")
                || trimmed.starts_with("pub async fn ")
                || trimmed.starts_with("def ")
                || trimmed.starts_with("class ")
                || trimmed.starts_with("async def ")
                || trimmed.starts_with("function ")
                || trimmed.starts_with("export function ")
                || trimmed.starts_with("export default function ")
                || trimmed.starts_with("struct ")
                || trimmed.starts_with("pub struct ")
                || trimmed.starts_with("enum ")
                || trimmed.starts_with("pub enum ")
                || trimmed.starts_with("impl ")
                || trimmed.starts_with("trait ")
                || trimmed.starts_with("pub trait ");

            if is_boundary && !current_chunk_lines.is_empty() {
                let text = current_chunk_lines.join("\n");
                let tokens = Self::estimate_tokens(&text);
                if tokens >= 60 {
                    chunks.push(DocChunk {
                        chunk_id: Uuid::new_v4().to_string(),
                        project_id: project_id.to_string(),
                        file_path: file_path.to_string(),
                        content_hash: content_hash.to_string(),
                        ord,
                        line_start: start_line,
                        line_end: line_num.saturating_sub(1),
                        text,
                        kind: "code".to_string(),
                        token_estimate: tokens,
                        created_at: Utc::now(),
                        embedding: None,
                    });
                    ord += 1;
                    current_chunk_lines.clear();
                    start_line = line_num;
                }
            }

            current_chunk_lines.push(*line);

            let current_text = current_chunk_lines.join("\n");
            if Self::estimate_tokens(&current_text) > 750 {
                let tokens = Self::estimate_tokens(&current_text);
                chunks.push(DocChunk {
                    chunk_id: Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    file_path: file_path.to_string(),
                    content_hash: content_hash.to_string(),
                    ord,
                    line_start: start_line,
                    line_end: line_num,
                    text: current_text,
                    kind: "code".to_string(),
                    token_estimate: tokens,
                    created_at: Utc::now(),
                    embedding: None,
                });
                ord += 1;
                current_chunk_lines.clear();
                start_line = line_num + 1;
            }
        }

        if !current_chunk_lines.is_empty() {
            let text = current_chunk_lines.join("\n");
            let tokens = Self::estimate_tokens(&text);
            chunks.push(DocChunk {
                chunk_id: Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                file_path: file_path.to_string(),
                content_hash: content_hash.to_string(),
                ord,
                line_start: start_line,
                line_end: lines.len(),
                text,
                kind: "code".to_string(),
                token_estimate: tokens,
                created_at: Utc::now(),
                embedding: None,
            });
        }

        chunks
    }

    fn chunk_text(
        project_id: &str,
        file_path: &str,
        content: &str,
        content_hash: &str,
    ) -> Vec<DocChunk> {
        let lines: Vec<&str> = content.lines().collect();
        if lines.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut current_chunk_lines = Vec::new();
        let mut start_line = 1;
        let mut ord = 0;

        for (idx, line) in lines.iter().enumerate() {
            let line_num = idx + 1;
            let is_blank = line.trim().is_empty();

            if is_blank && !current_chunk_lines.is_empty() {
                let text = current_chunk_lines.join("\n");
                let tokens = Self::estimate_tokens(&text);
                if tokens >= 100 {
                    chunks.push(DocChunk {
                        chunk_id: Uuid::new_v4().to_string(),
                        project_id: project_id.to_string(),
                        file_path: file_path.to_string(),
                        content_hash: content_hash.to_string(),
                        ord,
                        line_start: start_line,
                        line_end: line_num.saturating_sub(1),
                        text,
                        kind: "text".to_string(),
                        token_estimate: tokens,
                        created_at: Utc::now(),
                        embedding: None,
                    });
                    ord += 1;
                    current_chunk_lines.clear();
                    start_line = line_num;
                }
            }

            current_chunk_lines.push(*line);

            let current_text = current_chunk_lines.join("\n");
            if Self::estimate_tokens(&current_text) > 600 {
                let tokens = Self::estimate_tokens(&current_text);
                chunks.push(DocChunk {
                    chunk_id: Uuid::new_v4().to_string(),
                    project_id: project_id.to_string(),
                    file_path: file_path.to_string(),
                    content_hash: content_hash.to_string(),
                    ord,
                    line_start: start_line,
                    line_end: line_num,
                    text: current_text,
                    kind: "text".to_string(),
                    token_estimate: tokens,
                    created_at: Utc::now(),
                    embedding: None,
                });
                ord += 1;
                current_chunk_lines.clear();
                start_line = line_num + 1;
            }
        }

        if !current_chunk_lines.is_empty() {
            let text = current_chunk_lines.join("\n");
            let tokens = Self::estimate_tokens(&text);
            chunks.push(DocChunk {
                chunk_id: Uuid::new_v4().to_string(),
                project_id: project_id.to_string(),
                file_path: file_path.to_string(),
                content_hash: content_hash.to_string(),
                ord,
                line_start: start_line,
                line_end: lines.len(),
                text,
                kind: "text".to_string(),
                token_estimate: tokens,
                created_at: Utc::now(),
                embedding: None,
            });
        }

        chunks
    }
}
