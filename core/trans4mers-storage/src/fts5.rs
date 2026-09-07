//! Utilities for SQLite FTS5 query sanitization and matching.

/// Sanitizes an arbitrary user query string into a safe FTS5 query.
///
/// FTS5 query syntax can produce syntax errors if special characters such as
/// `"`, `:`, `*`, `^`, `(`, `)`, `AND`, `OR`, `NOT` are unbalanced or malformed.
/// This function extracts clean alphanumeric tokens, escapes any quotes,
/// wraps each token in double quotes, and joins them with `OR` to allow
/// partial term matches while letting SQLite's BM25 ranking prioritize
/// documents matching more (and rarer) terms.
pub fn sanitize_fts5_query(query: &str) -> String {
    let mut tokens = Vec::new();

    for raw_word in query.split_whitespace() {
        let trimmed = raw_word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        let clean: String = trimmed
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();

        if !clean.is_empty() && clean.chars().any(|c| c.is_alphanumeric() || c == '_') {
            let escaped = clean.replace('"', "\"\"");
            tokens.push(format!("\"{}\"", escaped));
        }
    }

    if tokens.is_empty() {
        String::new()
    } else {
        tokens.join(" OR ")
    }
}

/// Sanitizes an arbitrary user query for exact token or prefix matching.
/// Appends a wildcard `*` to the last token if it is at least 3 characters long.
pub fn sanitize_fts5_prefix_query(query: &str) -> String {
    let mut clean_words = Vec::new();

    for raw_word in query.split_whitespace() {
        let trimmed = raw_word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
        let clean: String = trimmed
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
            .collect();

        if !clean.is_empty() && clean.chars().any(|c| c.is_alphanumeric() || c == '_') {
            clean_words.push(clean);
        }
    }

    let num_tokens = clean_words.len();
    let mut tokens = Vec::with_capacity(num_tokens);

    for (idx, word) in clean_words.into_iter().enumerate() {
        let escaped = word.replace('"', "\"\"");
        if idx == num_tokens - 1 && escaped.len() >= 3 {
            tokens.push(format!("\"{}\"*", escaped));
        } else {
            tokens.push(format!("\"{}\"", escaped));
        }
    }

    if tokens.is_empty() {
        String::new()
    } else {
        tokens.join(" OR ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_clean_words() {
        assert_eq!(sanitize_fts5_query("hello world"), "\"hello\" OR \"world\"");
    }

    #[test]
    fn test_sanitize_special_characters() {
        assert_eq!(
            sanitize_fts5_query("test: (foo\"bar*) AND NOT OR"),
            "\"test\" OR \"foobar\" OR \"AND\" OR \"NOT\" OR \"OR\""
        );
    }

    #[test]
    fn test_sanitize_code_snippet() {
        assert_eq!(
            sanitize_fts5_query("fn execute_step() -> Result<(), Error>"),
            "\"fn\" OR \"execute_step\" OR \"Result\" OR \"Error\""
        );
    }

    #[test]
    fn test_sanitize_empty_and_punctuation() {
        assert_eq!(sanitize_fts5_query(""), "");
        assert_eq!(sanitize_fts5_query("   "), "");
        assert_eq!(
            sanitize_fts5_query("!@#$%^&*()_+={}|[]\\;':\",.<>?/"),
            "\"_\""
        );
        assert_eq!(sanitize_fts5_query("!@#$%^&*()"), "");
    }

    #[test]
    fn test_prefix_query() {
        assert_eq!(
            sanitize_fts5_prefix_query("search doc"),
            "\"search\" OR \"doc\"*"
        );
    }
}
