//! Little-endian IEEE-754 binary serialization for vector embeddings stored in SQLite BLOBs.

/// Converts a slice of 32-bit floats into a byte vector of little-endian IEEE-754 bytes.
pub fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(embedding.len() * 4);
    for &f in embedding {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    bytes
}

/// Parses a byte slice of little-endian IEEE-754 bytes into a vector of 32-bit floats.
pub fn blob_to_embedding(blob: &[u8]) -> Vec<f32> {
    blob.as_chunks::<4>()
        .0
        .iter()
        .map(|&c| f32::from_le_bytes(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_blob_roundtrip() {
        let original = vec![0.123f32, -45.67, 0.0, 999.999, f32::MIN, f32::MAX];
        let blob = embedding_to_blob(&original);
        assert_eq!(blob.len(), original.len() * 4);
        let restored = blob_to_embedding(&blob);
        assert_eq!(original, restored);
    }
}
