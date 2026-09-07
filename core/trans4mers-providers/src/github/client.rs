use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_domain::github::{GitHubComment, GitHubIssue, GitHubPrReviewSubmission};

pub struct GitHubClient {
    base_url: String,
    http_client: reqwest::Client,
}

impl Default for GitHubClient {
    fn default() -> Self {
        Self::new()
    }
}

impl GitHubClient {
    pub fn new() -> Self {
        Self::new_with_base_url("https://api.github.com".to_string())
    }

    pub fn new_with_base_url(base_url: String) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http_client,
        }
    }

    fn build_headers(&self, accept_header: &str, token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Trans4mers-AgentOS/1.0"),
        );
        if let Ok(accept_val) = HeaderValue::from_str(accept_header) {
            headers.insert(ACCEPT, accept_val);
        }
        if let Some(auth_val) = token
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .and_then(|tok| HeaderValue::from_str(&format!("Bearer {}", tok)).ok())
        {
            headers.insert(AUTHORIZATION, auth_val);
        }
        headers
    }

    /// Fetches a GitHub issue by repository and issue number, including its comments discussion thread.
    pub async fn fetch_issue(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        token: Option<&str>,
    ) -> Result<GitHubIssue, Trans4mersError> {
        let issue_url = format!(
            "{}/repos/{}/{}/issues/{}",
            self.base_url, owner, repo, number
        );
        let headers = self.build_headers("application/vnd.github.v3+json", token);

        let resp = self
            .http_client
            .get(&issue_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| Trans4mersError::Network(format!("Failed to connect to GitHub: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::Network(format!(
                "GitHub API error fetching issue #{}: HTTP {} - {}",
                number, status, err_body
            )));
        }

        let val: Value = resp.json().await.map_err(|e| {
            Trans4mersError::Serialization(format!("Failed to parse GitHub issue JSON: {}", e))
        })?;

        let title = val["title"].as_str().unwrap_or_default().to_string();
        let body = val["body"].as_str().map(|s| s.to_string());
        let state = val["state"].as_str().unwrap_or("open").to_string();
        let html_url = val["html_url"].as_str().unwrap_or_default().to_string();
        let author = val["user"]["login"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        let created_at = val["created_at"].as_str().unwrap_or_default().to_string();
        let updated_at = val["updated_at"].as_str().unwrap_or_default().to_string();

        let labels = val["labels"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|l| {
                        if let Some(s) = l.as_str() {
                            Some(s.to_string())
                        } else {
                            l["name"].as_str().map(|s| s.to_string())
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // Fetch discussion comments
        let comments = self
            .fetch_issue_comments(owner, repo, number, token)
            .await
            .unwrap_or_default();

        Ok(GitHubIssue {
            number,
            title,
            body,
            state,
            html_url,
            author,
            labels,
            comments,
            created_at,
            updated_at,
        })
    }

    /// Fetches all comments on a GitHub issue.
    pub async fn fetch_issue_comments(
        &self,
        owner: &str,
        repo: &str,
        number: u64,
        token: Option<&str>,
    ) -> Result<Vec<GitHubComment>, Trans4mersError> {
        let comments_url = format!(
            "{}/repos/{}/{}/issues/{}/comments",
            self.base_url, owner, repo, number
        );
        let headers = self.build_headers("application/vnd.github.v3+json", token);

        let resp = self
            .http_client
            .get(&comments_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                Trans4mersError::Network(format!("Failed to connect to GitHub comments API: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::Network(format!(
                "GitHub API error fetching issue comments: HTTP {} - {}",
                status, err_body
            )));
        }

        let arr: Vec<Value> = resp.json().await.map_err(|e| {
            Trans4mersError::Serialization(format!("Failed to parse GitHub comments JSON: {}", e))
        })?;

        let comments = arr
            .into_iter()
            .map(|c| GitHubComment {
                id: c["id"].as_u64().unwrap_or_default(),
                author: c["user"]["login"].as_str().unwrap_or("unknown").to_string(),
                body: c["body"].as_str().unwrap_or_default().to_string(),
                created_at: c["created_at"].as_str().unwrap_or_default().to_string(),
            })
            .collect();

        Ok(comments)
    }

    /// Fetches the raw unified diff for a Pull Request.
    pub async fn fetch_pr_diff(
        &self,
        owner: &str,
        repo: &str,
        pull_number: u64,
        token: Option<&str>,
    ) -> Result<String, Trans4mersError> {
        let pr_url = format!(
            "{}/repos/{}/{}/pulls/{}",
            self.base_url, owner, repo, pull_number
        );
        let headers = self.build_headers("application/vnd.github.v3.diff", token);

        let resp = self
            .http_client
            .get(&pr_url)
            .headers(headers)
            .send()
            .await
            .map_err(|e| {
                Trans4mersError::Network(format!("Failed to connect to GitHub PR API: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::Network(format!(
                "GitHub API error fetching PR diff: HTTP {} - {}",
                status, err_body
            )));
        }

        let diff_text = resp
            .text()
            .await
            .map_err(|e| Trans4mersError::Network(format!("Failed to read PR diff body: {}", e)))?;

        Ok(diff_text)
    }

    /// Submits a Pull Request review with comments.
    /// Hard invariant: Requires approved ActionDiff beforehand; token must be provided.
    pub async fn post_pr_review(
        &self,
        owner: &str,
        repo: &str,
        pull_number: u64,
        submission: &GitHubPrReviewSubmission,
        token: Option<&str>,
    ) -> Result<Value, Trans4mersError> {
        let tok = token.ok_or_else(|| {
            Trans4mersError::Validation(
                "GitHub Personal Access Token is required to submit a PR review".to_string(),
            )
        })?;

        let url = format!(
            "{}/repos/{}/{}/pulls/{}/reviews",
            self.base_url, owner, repo, pull_number
        );
        let headers = self.build_headers("application/vnd.github.v3+json", Some(tok));

        let formatted_comments: Vec<Value> = submission
            .comments
            .iter()
            .map(|c| {
                serde_json::json!({
                    "path": c.path,
                    "line": c.line,
                    "side": c.side,
                    "body": format!("[{}] {}", c.severity.to_uppercase(), c.body),
                })
            })
            .collect();

        let payload = serde_json::json!({
            "body": submission.body,
            "event": submission.event,
            "comments": formatted_comments,
        });

        let resp = self
            .http_client
            .post(&url)
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                Trans4mersError::Network(format!("Failed to submit GitHub PR review: {}", e))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_body = resp.text().await.unwrap_or_default();
            return Err(Trans4mersError::Network(format!(
                "GitHub API error submitting PR review: HTTP {} - {}",
                status, err_body
            )));
        }

        let res_json: Value = resp.json().await.map_err(|e| {
            Trans4mersError::Serialization(format!("Failed to parse GitHub review response: {}", e))
        })?;

        Ok(res_json)
    }
}
