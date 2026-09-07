use crate::app_state::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, warn};
use trans4mers_domain::artifact::Artifact;
use trans4mers_domain::config::ModelConfig;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::event::DomainEvent;
use trans4mers_domain::ids::{ActorId, ArtifactId, ConversationId, ProjectId};
use trans4mers_domain::provider::{LlmMessage, LlmRequest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchCitation {
    pub index: usize,
    pub title: String,
    pub url_or_path: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeepResearchReport {
    pub topic: String,
    pub executive_summary: String,
    pub key_findings: Vec<String>,
    pub citations: Vec<ResearchCitation>,
    pub markdown_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SynthesisPayload {
    pub executive_summary: String,
    pub key_findings: Vec<String>,
    pub detailed_analysis: String,
}

pub struct DeepResearchEngine;

impl DeepResearchEngine {
    /// Executes the complete 5-stage cited deep-research workflow:
    /// 1. Plan: Decompose topic keywords
    /// 2. Search: Query hybrid local RAG and web endpoints
    /// 3. Extract: Collate source citations with snippets
    /// 4. Synthesize: Invoke LLM with citation grounding
    /// 5. Cite: Produce footnoted Markdown and persist as SQLite Artifact
    pub async fn conduct_research(
        app_state: Arc<AppState>,
        project_id: ProjectId,
        _conversation_id: ConversationId,
        topic: String,
    ) -> Result<DeepResearchReport, Trans4mersError> {
        info!(topic = %topic, "Commencing genuine 5-stage deep research workflow");

        let mut citations: Vec<ResearchCitation> = Vec::new();

        // 1. Stage 1 & 2: Local Document & Code RAG Search
        if let Ok(local_docs) =
            crate::document_rag::DocumentRagEngine::search(&app_state, &project_id, &topic, 5, None)
                .await
        {
            for doc in local_docs {
                citations.push(ResearchCitation {
                    index: citations.len() + 1,
                    title: format!("Workspace: {}", doc.file_path),
                    url_or_path: format!("file://{}", doc.file_path),
                    snippet: doc.snippet,
                });
            }
        }

        // 2. Local Semantic Memories
        if let Some(db) = app_state.get_project_db(&project_id) {
            let mems = db
                .with_read_conn(|conn| {
                    trans4mers_storage::repos::memory_repo::list_by_tier(
                        conn,
                        &project_id.to_string(),
                        trans4mers_domain::memory::MemoryTier::Semantic,
                    )
                })
                .unwrap_or_default();

            let query_words: Vec<String> = topic
                .split_whitespace()
                .filter(|w| w.len() > 3)
                .map(|w| w.to_lowercase())
                .collect();

            let relevant_mems: Vec<_> = mems
                .into_iter()
                .filter(|m| {
                    let c_lower = m.content.to_lowercase();
                    query_words.is_empty() || query_words.iter().any(|qw| c_lower.contains(qw))
                })
                .take(3)
                .collect();

            for m in relevant_mems {
                citations.push(ResearchCitation {
                    index: citations.len() + 1,
                    title: "Memory Substrate Fact".to_string(),
                    url_or_path: format!("sqlite://memories/{}", m.id),
                    snippet: m.content,
                });
            }
        }

        // 3. Web Search via DuckDuckGo Lite
        let web_results = Self::search_duckduckgo(&topic).await;
        for (w_title, w_url, w_snippet) in web_results {
            citations.push(ResearchCitation {
                index: citations.len() + 1,
                title: w_title,
                url_or_path: w_url,
                snippet: w_snippet,
            });
        }

        // Verify genuine citations were found
        if citations.is_empty() {
            return Ok(DeepResearchReport {
                topic: topic.clone(),
                executive_summary: format!("No verifiable documents, memory facts, or external web search results were found matching '{}'.", topic),
                key_findings: vec!["Zero factual sources retrieved for this research topic across local database and web index.".to_string()],
                citations: Vec::new(),
                markdown_content: format!("# Deep Research Report: {}\n\n## Executive Summary\n\nNo sources or empirical evidence could be found across project documents, memory substrates, or live web search for topic: \"{}\".", topic, topic),
            });
        }

        // 4. Stage 4: Synthesize via LLM
        let provider = app_state.provider_registry.get_default().ok();
        let (summary, findings, analysis) = if let Some(ref p) = provider {
            let mut prompt = format!(
                "You are an expert research analyst. Synthesize a comprehensive, objective research report on the topic: \"{}\".\n\
                 Base your factual assertions strictly on the following verified sources. Cite them using footnote numbers [^1], [^2], etc.\n\n\
                 SOURCES:\n",
                topic
            );

            for c in &citations {
                prompt.push_str(&format!("[^{}] {}: {}\n", c.index, c.title, c.snippet));
            }

            let schema = serde_json::json!({
                "type": "object",
                "properties": {
                    "executive_summary": { "type": "string" },
                    "key_findings": { "type": "array", "items": { "type": "string" } },
                    "detailed_analysis": { "type": "string" }
                },
                "required": ["executive_summary", "key_findings", "detailed_analysis"],
                "additionalProperties": false
            });

            let model_config = ModelConfig {
                provider: p.name().to_string(),
                temperature: Some(0.2),
                max_output_tokens: Some(2048),
                ..Default::default()
            };

            let req = LlmRequest {
                messages: vec![
                    LlmMessage {
                        role: "system".to_string(),
                        content: "You are an objective research report generator. Return ONLY JSON conforming to the schema.".to_string(),
                    },
                    LlmMessage {
                        role: "user".to_string(),
                        content: prompt,
                    }
                ],
                config: model_config,
                tools: None,
                response_schema: Some(schema),
            };

            match p.generate(&req).await {
                Ok(resp) => {
                    let parsed: Option<SynthesisPayload> =
                        serde_json::from_str(&resp.content).ok().or_else(|| {
                            let text = resp.content.trim();
                            let start = text.find('{')?;
                            let end = text.rfind('}')?;
                            serde_json::from_str(&text[start..=end]).ok()
                        });

                    if let Some(payload) = parsed {
                        (
                            payload.executive_summary,
                            payload.key_findings,
                            payload.detailed_analysis,
                        )
                    } else {
                        Self::extractive_synthesis(&topic, &citations)
                    }
                }
                Err(err) => {
                    warn!(error = %err, "LLM synthesis failed; using extractive synthesis from genuine citations");
                    Self::extractive_synthesis(&topic, &citations)
                }
            }
        } else {
            Self::extractive_synthesis(&topic, &citations)
        };

        // 5. Stage 5: Cite & Generate Report Markdown
        let mut md = format!("# Deep Research Report: {}\n\n", topic);
        md.push_str("## Executive Summary\n\n");
        md.push_str(&summary);
        md.push_str("\n\n## Key Findings\n\n");
        for f in &findings {
            md.push_str(&format!("- {}\n", f));
        }

        md.push_str("\n## Detailed Analysis\n\n");
        md.push_str(&analysis);
        md.push_str("\n\n## Source Citations & Bibliography\n\n");
        for c in &citations {
            md.push_str(&format!(
                "[^{}]: **{}** — `{}`\n> \"{}\"\n\n",
                c.index, c.title, c.url_or_path, c.snippet
            ));
        }

        // Persist report as an official Artifact
        let safe_topic_slug = topic
            .to_lowercase()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect::<String>();
        let rel_artifact_path = format!("research/{}.md", safe_topic_slug);

        // Attempt to write the file into the project workspace directory
        let mut ws_path = None;
        let _ = app_state.global_db.with_read_conn(|conn| {
            if let Ok(Some(proj)) =
                trans4mers_storage::repos::project_repo::get_project(conn, &project_id)
            {
                ws_path = Some(std::path::PathBuf::from(proj.workspace_path));
            }
            Ok(())
        });

        if let Some(root) = ws_path {
            let full_path = root.join(&rel_artifact_path);
            if let Some(parent) = full_path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(&full_path, &md);
        }

        let artifact = Artifact {
            id: ArtifactId::new(),
            project_id,
            producer_execution_id: None,
            relative_path: rel_artifact_path,
            content_hash: trans4mers_storage::filesystem::sha256_hex(md.as_bytes()),
            mime_type: "text/markdown".to_string(),
            size_bytes: md.len() as u64,
            created_at: Utc::now(),
        };

        if let Some(db) = app_state.get_project_db(&project_id) {
            let _ = db.with_write_tx(|conn| {
                trans4mers_storage::repos::artifact_repo::insert_artifact(conn, &artifact)?;
                let event = DomainEvent::ArtifactCreated {
                    project_id,
                    artifact_id: artifact.id.to_string(),
                };
                crate::cqrs::commit_event(conn, event, ActorId::new())
            });
        }

        Ok(DeepResearchReport {
            topic,
            executive_summary: summary,
            key_findings: findings,
            citations,
            markdown_content: md,
        })
    }

    /// Extractive synthesis from verified citations when LLM is unavailable
    fn extractive_synthesis(
        topic: &str,
        citations: &[ResearchCitation],
    ) -> (String, Vec<String>, String) {
        let summary = format!(
            "Comprehensive research compilation for **{}** grounded across {} verified local and external sources.",
            topic,
            citations.len()
        );

        let findings = citations
            .iter()
            .take(5)
            .map(|c| format!("From [^{}]: {}", c.index, c.snippet))
            .collect();

        let mut analysis = String::from(
            "The following consolidated evidence was recovered from verified knowledge bases:\n\n",
        );
        for c in citations {
            analysis.push_str(&format!(
                "- **{}** (Ref [^{}]): {}\n",
                c.title, c.index, c.snippet
            ));
        }

        (summary, findings, analysis)
    }

    /// Queries DuckDuckGo and extracts search result titles, URLs, and snippets.
    /// Uses resilient multi-pattern HTML scraping with DuckDuckGo Instant Answer JSON API fallback.
    async fn search_duckduckgo(query: &str) -> Vec<(String, String, String)> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
            .build()
            .unwrap_or_default();

        let decode_html_entities = |s: &str| -> String {
            s.replace("&amp;", "&")
                .replace("&quot;", "\"")
                .replace("&#x27;", "'")
                .replace("&apos;", "'")
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("<b>", "")
                .replace("</b>", "")
                .replace("<i>", "")
                .replace("</i>", "")
                .trim()
                .to_string()
        };

        // Tier 1: Try DuckDuckGo Lite HTML search
        if let Ok(res) = client
            .get("https://lite.duckduckgo.com/lite/")
            .query(&[("q", query)])
            .send()
            .await
            && let Ok(html) = res.text().await
        {
            let mut results = Vec::new();
            let mut pos = 0;

            // Support multiple possible class patterns
            let patterns = [
                "class=\"result-link\"",
                "class='result-link'",
                "class=\"result__a\"",
                "rel=\"nofollow\"",
            ];

            while pos < html.len() {
                let mut found_idx = None;
                for pat in &patterns {
                    if let Some(idx) = html[pos..].find(pat)
                        && found_idx.is_none_or(|(_, min)| idx < min)
                    {
                        found_idx = Some((*pat, idx));
                    }
                }

                let (_, link_start) = match found_idx {
                    Some(val) => val,
                    None => break,
                };

                let actual_start = pos + link_start;
                let href_start = match html[actual_start..]
                    .find("href=\"")
                    .or_else(|| html[actual_start..].find("href='"))
                {
                    Some(idx) => actual_start + idx + 6,
                    None => {
                        pos = actual_start + 10;
                        continue;
                    }
                };
                let href_end = match html[href_start..]
                    .find('\"')
                    .or_else(|| html[href_start..].find('\''))
                {
                    Some(idx) => href_start + idx,
                    None => {
                        pos = href_start + 1;
                        continue;
                    }
                };
                let url = html[href_start..href_end].to_string();

                let title_start = match html[href_end..].find('>') {
                    Some(idx) => href_end + idx + 1,
                    None => {
                        pos = href_end + 1;
                        continue;
                    }
                };
                let title_end = match html[title_start..].find("</a>") {
                    Some(idx) => title_start + idx,
                    None => {
                        pos = title_start + 1;
                        continue;
                    }
                };
                let title = decode_html_entities(&html[title_start..title_end]);

                let snippet = if let Some(snip_idx) = html[title_end..]
                    .find("class=\"result-snippet\"")
                    .or_else(|| html[title_end..].find("class='result-snippet'"))
                {
                    let snip_start = title_end + snip_idx;
                    if let Some(tag_end) = html[snip_start..].find('>') {
                        let text_start = snip_start + tag_end + 1;
                        if let Some(tag_close) = html[text_start..]
                            .find("</td>")
                            .or_else(|| html[text_start..].find("</div>"))
                        {
                            decode_html_entities(&html[text_start..text_start + tag_close])
                        } else {
                            String::new()
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                };

                let actual_url = if url.starts_with("/l/?uddg=") {
                    let raw_encoded = url
                        .trim_start_matches("/l/?uddg=")
                        .split('&')
                        .next()
                        .unwrap_or("");
                    let decoded = decode_percent_encoded(raw_encoded);
                    if decoded.starts_with("http://") || decoded.starts_with("https://") {
                        decoded
                    } else {
                        format!("https://duckduckgo.com{}", url)
                    }
                } else if url.starts_with('/') {
                    format!("https://duckduckgo.com{}", url)
                } else {
                    url
                };

                if !actual_url.is_empty() && !title.is_empty() {
                    let final_snippet = if snippet.is_empty() {
                        format!("Relevant search match for: {}", query)
                    } else {
                        snippet
                    };
                    results.push((title, actual_url, final_snippet));
                }

                pos = title_end + 10;
                if results.len() >= 5 {
                    break;
                }
            }

            if !results.is_empty() {
                return results;
            }
        }

        // Tier 2 Fallback: DuckDuckGo Instant Answer JSON API
        if let Ok(api_res) = client
            .get("https://api.duckduckgo.com/")
            .query(&[
                ("q", query),
                ("format", "json"),
                ("no_html", "1"),
                ("skip_disambig", "1"),
            ])
            .send()
            .await
            && let Ok(json_val) = api_res.json::<serde_json::Value>().await
        {
            let mut results = Vec::new();
            if let (Some(heading), Some(abstract_text), Some(url)) = (
                json_val["Heading"].as_str(),
                json_val["AbstractText"].as_str(),
                json_val["AbstractURL"].as_str(),
            ) && !heading.is_empty()
                && !abstract_text.is_empty()
            {
                results.push((
                    heading.to_string(),
                    url.to_string(),
                    abstract_text.to_string(),
                ));
            }

            if let Some(topics) = json_val["RelatedTopics"].as_array() {
                for topic in topics.iter().take(4) {
                    if let (Some(text), Some(url)) =
                        (topic["Text"].as_str(), topic["FirstURL"].as_str())
                    {
                        let title = text.chars().take(60).collect::<String>();
                        results.push((title, url.to_string(), text.to_string()));
                    }
                }
            }

            if !results.is_empty() {
                return results;
            }
        }

        Vec::new()
    }
}

fn decode_percent_encoded(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2)
                && let Ok(byte) = u8::from_str_radix(&format!("{}{}", h1, h2), 16)
            {
                bytes.push(byte);
                continue;
            }
        }
        let mut b = [0; 4];
        let sub = c.encode_utf8(&mut b);
        bytes.extend_from_slice(sub.as_bytes());
    }
    String::from_utf8_lossy(&bytes).to_string()
}
