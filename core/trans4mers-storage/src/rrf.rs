use std::collections::HashMap;

/// Reciprocal Rank Fusion (RRF) combines ranked lists from multiple retrieval systems.
/// Score for item: sum_{lists} 1.0 / (k + rank(item))
/// Returns (id, fused_score) sorted descending by score.
pub fn reciprocal_rank_fusion<T: AsRef<str>>(ranked_lists: &[&[T]], k: f32) -> Vec<(String, f32)> {
    let mut scores: HashMap<String, f32> = HashMap::new();
    for list in ranked_lists {
        for (rank, item) in list.iter().enumerate() {
            let score = 1.0 / (k + rank as f32);
            *scores.entry(item.as_ref().to_string()).or_insert(0.0) += score;
        }
    }

    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}
