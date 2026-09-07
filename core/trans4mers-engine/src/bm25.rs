use std::collections::{HashMap, HashSet};

pub struct Bm25Index {
    k1: f32,
    b: f32,
}

impl Default for Bm25Index {
    fn default() -> Self {
        Self::new()
    }
}

impl Bm25Index {
    pub fn new() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }

    pub fn with_params(k1: f32, b: f32) -> Self {
        Self { k1, b }
    }

    /// Tokenizes input into lowercased alphanumeric tokens.
    pub fn tokenize(text: &str) -> Vec<String> {
        text.to_lowercase()
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s| !s.is_empty() && s.len() > 1)
            .map(|s| s.to_string())
            .collect()
    }

    /// Scores a list of (id, text) documents against a query.
    /// Returns (id, score) pairs sorted by score descending.
    pub fn score_documents(&self, query: &str, docs: &[(&str, &str)]) -> Vec<(String, f32)> {
        let n = docs.len();
        if n == 0 {
            return Vec::new();
        }

        let query_tokens = Self::tokenize(query);
        if query_tokens.is_empty() {
            return Vec::new();
        }

        // Tokenize all docs and compute doc lengths
        let mut doc_tokens: Vec<Vec<String>> = Vec::with_capacity(n);
        let mut total_tokens = 0;
        let mut doc_term_freqs: Vec<HashMap<String, usize>> = Vec::with_capacity(n);
        let mut doc_freqs: HashMap<String, usize> = HashMap::new();

        for (_id, text) in docs {
            let tokens = Self::tokenize(text);
            total_tokens += tokens.len();

            let mut tf = HashMap::new();
            let mut seen_terms = HashSet::new();
            for t in &tokens {
                *tf.entry(t.clone()).or_insert(0) += 1;
                if seen_terms.insert(t.clone()) {
                    *doc_freqs.entry(t.clone()).or_insert(0) += 1;
                }
            }
            doc_term_freqs.push(tf);
            doc_tokens.push(tokens);
        }

        let avgdl = (total_tokens as f32) / (n as f32);

        // Precompute IDFs for query tokens
        let mut idfs: HashMap<String, f32> = HashMap::new();
        for t in &query_tokens {
            if !idfs.contains_key(t) {
                let df = doc_freqs.get(t).copied().unwrap_or(0);
                // Standard smoothed BM25 IDF
                let idf = (1.0 + (n as f32 - df as f32 + 0.5) / (df as f32 + 0.5)).ln();
                idfs.insert(t.clone(), idf.max(0.0));
            }
        }

        // Score each document
        let mut scores: Vec<(String, f32)> = Vec::with_capacity(n);
        for (i, (id, _)) in docs.iter().enumerate() {
            let doc_len = doc_tokens[i].len() as f32;
            let tf_map = &doc_term_freqs[i];
            let mut score = 0.0f32;

            for t in &query_tokens {
                let tf = tf_map.get(t).copied().unwrap_or(0) as f32;
                if tf > 0.0 {
                    let idf = idfs.get(t).copied().unwrap_or(0.0);
                    let numerator = tf * (self.k1 + 1.0);
                    let denominator = tf + self.k1 * (1.0 - self.b + self.b * (doc_len / avgdl));
                    score += idf * (numerator / denominator);
                }
            }

            if score > 0.0 {
                scores.push((id.to_string(), score));
            }
        }

        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scores
    }
}
