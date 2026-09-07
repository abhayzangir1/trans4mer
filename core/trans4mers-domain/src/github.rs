use crate::error::Trans4mersError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubComment {
    pub id: u64,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubIssue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub html_url: String,
    pub author: String,
    pub labels: Vec<String>,
    pub comments: Vec<GitHubComment>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPrReviewComment {
    pub path: String,
    pub line: usize,
    pub side: String,
    pub body: String,
    pub severity: String, // e.g. "suggestion", "warning", "error", "nitpick"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubPrReviewSubmission {
    pub owner: String,
    pub repo: String,
    pub pull_number: u64,
    pub event: String, // "COMMENT", "APPROVE", "REQUEST_CHANGES"
    pub body: String,
    pub comments: Vec<GitHubPrReviewComment>,
}

/// Parses a GitHub issue or PR reference string into (owner, repo, number).
/// Supported formats:
/// - `https://github.com/owner/repo/issues/123`
/// - `https://github.com/owner/repo/pull/123`
/// - `owner/repo#123`
/// - `owner/repo/123`
pub fn parse_issue_reference(input: &str) -> Result<(String, String, u64), Trans4mersError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(Trans4mersError::Validation(
            "GitHub issue reference cannot be empty".to_string(),
        ));
    }

    // Format 1: Full URL e.g. https://github.com/owner/repo/issues/123 or pull/123
    let url_stripped = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
        .or_else(|| trimmed.strip_prefix("github.com/"));

    if let Some(stripped) = url_stripped {
        let segments: Vec<&str> = stripped.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() >= 4 && (segments[2] == "issues" || segments[2] == "pull") {
            let owner = segments[0].to_string();
            let repo = segments[1].to_string();
            let num: u64 = segments[3].parse().map_err(|_| {
                Trans4mersError::Validation(format!(
                    "Invalid GitHub issue/pull number: '{}'",
                    segments[3]
                ))
            })?;
            return Ok((owner, repo, num));
        }
    }

    // Format 2: owner/repo#123
    if let Some((repo_part, num_part)) = trimmed.split_once('#') {
        let parts: Vec<&str> = repo_part.split('/').collect();
        if parts.len() == 2 {
            let owner = parts[0].trim().to_string();
            let repo = parts[1].trim().to_string();
            let num: u64 = num_part.trim().parse().map_err(|_| {
                Trans4mersError::Validation(format!(
                    "Invalid GitHub issue number after '#': '{}'",
                    num_part
                ))
            })?;
            return Ok((owner, repo, num));
        }
    }

    // Format 3: owner/repo/123 or owner/repo/issues/123
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() == 3 {
        let owner = parts[0].trim().to_string();
        let repo = parts[1].trim().to_string();
        let num: u64 = parts[2].trim().parse().map_err(|_| {
            Trans4mersError::Validation(format!("Invalid GitHub issue number: '{}'", parts[2]))
        })?;
        return Ok((owner, repo, num));
    } else if parts.len() == 4 && parts[2] == "issues" {
        let owner = parts[0].trim().to_string();
        let repo = parts[1].trim().to_string();
        let num: u64 = parts[3].trim().parse().map_err(|_| {
            Trans4mersError::Validation(format!("Invalid GitHub issue number: '{}'", parts[3]))
        })?;
        return Ok((owner, repo, num));
    }

    Err(Trans4mersError::Validation(format!(
        "Invalid GitHub issue reference '{}'. Expected 'https://github.com/owner/repo/issues/123' or 'owner/repo#123'",
        trimmed
    )))
}

/// Formats a complete GitHub issue with its comment thread into a clean markdown brief artifact.
pub fn format_issue_brief_markdown(issue: &GitHubIssue) -> String {
    let mut doc = String::new();
    doc.push_str(&format!(
        "# GitHub Issue #{}: {}\n\n",
        issue.number, issue.title
    ));
    doc.push_str(&format!(
        "- **URL**: [{}]({})\n",
        issue.html_url, issue.html_url
    ));
    doc.push_str(&format!("- **Author**: @{}\n", issue.author));
    doc.push_str(&format!("- **Status**: {}\n", issue.state));
    doc.push_str(&format!("- **Created**: {}\n", issue.created_at));
    if !issue.labels.is_empty() {
        doc.push_str(&format!("- **Labels**: {}\n", issue.labels.join(", ")));
    }
    doc.push_str("\n---\n\n");
    doc.push_str("## Description\n\n");
    if let Some(ref body) = issue.body {
        if !body.trim().is_empty() {
            doc.push_str(body);
            doc.push_str("\n\n");
        } else {
            doc.push_str("*No description provided.*\n\n");
        }
    } else {
        doc.push_str("*No description provided.*\n\n");
    }

    if !issue.comments.is_empty() {
        doc.push_str("## Discussion Thread\n\n");
        for (i, c) in issue.comments.iter().enumerate() {
            doc.push_str(&format!(
                "### Comment #{} by @{} ({})\n\n",
                i + 1,
                c.author,
                c.created_at
            ));
            doc.push_str(&c.body);
            doc.push_str("\n\n---\n\n");
        }
    }

    doc
}
