/// Sanitizes and scrubs text payloads before transmitting to Langfuse.
/// When prompt capture is disabled, the full text is suppressed entirely.
/// When prompt capture is enabled, sovereign regex filters scrub sensitive credentials.
pub fn sanitize_payload(text: &str, capture_prompts: bool) -> String {
    if !capture_prompts {
        return "[REDACTED - Prompt capture disabled in settings]".to_string();
    }

    let mut sanitized = text.to_string();

    // 1. OpenAI / generic secret keys: sk-...
    let sk_regex = regex_lite_or_replace_sk(&sanitized);
    sanitized = sk_regex;

    // 2. Bearer tokens: Bearer <token>
    let bearer_regex = replace_bearer_tokens(&sanitized);
    sanitized = bearer_regex;

    // 3. GitHub personal access tokens: ghp_...
    let ghp_regex = replace_ghp_tokens(&sanitized);
    sanitized = ghp_regex;

    // 4. PEM private keys
    if let Some(start) = sanitized.find("-----BEGIN")
        && let Some(end) = sanitized.find("-----END")
        && let Some(final_end) = sanitized[end..].find("-----")
    {
        let actual_end = end + final_end + 5;
        sanitized.replace_range(start..actual_end, "[REDACTED PRIVATE KEY]");
    }

    sanitized
}

fn regex_lite_or_replace_sk(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(idx) = rest.find("sk-") {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        // find end of token: whitespace, quote, comma, parenthesis
        let token_end = tail
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ')')
            .unwrap_or(tail.len());
        let token = &tail[..token_end];
        if token.len() >= 15 {
            out.push_str("[REDACTED_API_KEY]");
        } else {
            out.push_str(token);
        }
        rest = &tail[token_end..];
    }
    out.push_str(rest);
    out
}

fn replace_bearer_tokens(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(idx) = rest.to_lowercase().find("bearer ") {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        let prefix_len = "bearer ".len();
        let token_slice = &tail[prefix_len..];
        let token_end = token_slice
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ')')
            .unwrap_or(token_slice.len());
        out.push_str("Bearer [REDACTED_BEARER_TOKEN]");
        rest = &token_slice[token_end..];
    }
    out.push_str(rest);
    out
}

fn replace_ghp_tokens(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(idx) = rest.find("ghp_") {
        out.push_str(&rest[..idx]);
        let tail = &rest[idx..];
        let token_end = tail
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ')')
            .unwrap_or(tail.len());
        if token_end >= 20 {
            out.push_str("[REDACTED_GITHUB_TOKEN]");
        } else {
            out.push_str(&tail[..token_end]);
        }
        rest = &tail[token_end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitizer_disabled_capture() {
        let input = "System instructions containing secret sk-proj-1234567890abcdef1234567890";
        let result = sanitize_payload(input, false);
        assert_eq!(result, "[REDACTED - Prompt capture disabled in settings]");
    }

    #[test]
    fn test_sanitizer_enabled_scrubs_secrets() {
        let input = "Connecting with Bearer mysecrettoken123 and sk-abcdef123456789012345 to API";
        let result = sanitize_payload(input, true);
        assert!(!result.contains("sk-abcdef"));
        assert!(!result.contains("mysecrettoken123"));
        assert!(result.contains("[REDACTED_API_KEY]"));
        assert!(result.contains("[REDACTED_BEARER_TOKEN]"));
    }
}
