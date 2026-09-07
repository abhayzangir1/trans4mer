use std::fs;
use std::path::{Path, PathBuf};
use trans4mers_domain::error::Trans4mersError;

/// Strictly validates and normalizes paths to prevent directory traversal.
pub struct FileSystemGuard;

impl FileSystemGuard {
    /// Ensures `requested_path` is strictly within `workspace_root`.
    /// Resolves all symlinks and `..` components to prevent breakout.
    pub fn validate_path_in_workspace(
        workspace_root: &Path,
        requested_path: &Path,
    ) -> Result<PathBuf, Trans4mersError> {
        let root = workspace_root.canonicalize().map_err(|_| {
            Trans4mersError::Internal("Workspace root does not exist or is invalid".to_string())
        })?;

        // If the path doesn't exist yet, find the nearest existing ancestor, canonicalize it, and append missing components
        let absolute_requested = if requested_path.exists() {
            requested_path
                .canonicalize()
                .map_err(|e| Trans4mersError::Internal(format!("Failed to resolve path: {}", e)))?
        } else {
            let mut current = requested_path;
            let mut missing_components = Vec::new();
            while !current.exists() {
                if let Some(file_name) = current.file_name() {
                    let name_str = file_name.to_string_lossy();
                    if name_str == ".." || name_str == "." {
                        return Err(Trans4mersError::PathOutsideWorkspace(
                            "Invalid path traversal component".to_string(),
                        ));
                    }
                    missing_components.push(file_name.to_os_string());
                }
                match current.parent() {
                    Some(p) => current = p,
                    None => break,
                }
            }

            let mut safe_ancestor = current.canonicalize().map_err(|e| {
                Trans4mersError::Internal(format!("Failed to resolve path ancestor: {}", e))
            })?;

            if !safe_ancestor.starts_with(&root) {
                return Err(Trans4mersError::PathOutsideWorkspace(
                    safe_ancestor.display().to_string(),
                ));
            }

            for comp in missing_components.into_iter().rev() {
                safe_ancestor.push(comp);
            }
            safe_ancestor
        };

        if !absolute_requested.starts_with(&root) {
            return Err(Trans4mersError::PathOutsideWorkspace(
                absolute_requested.display().to_string(),
            ));
        }

        Ok(absolute_requested)
    }

    /// Reads a file strictly within the workspace.
    pub fn read_workspace_file(
        workspace_root: &Path,
        relative_path: &str,
    ) -> Result<String, Trans4mersError> {
        let target = workspace_root.join(relative_path);
        let safe_path = Self::validate_path_in_workspace(workspace_root, &target)?;

        fs::read_to_string(&safe_path)
            .map_err(|e| Trans4mersError::Internal(format!("File read failed: {}", e)))
    }

    /// Writes a file strictly within the workspace.
    pub fn write_workspace_file(
        workspace_root: &Path,
        relative_path: &str,
        content: &str,
    ) -> Result<(), Trans4mersError> {
        let target = workspace_root.join(relative_path);
        let safe_path = Self::validate_path_in_workspace(workspace_root, &target)?;

        if let Some(parent) = safe_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Trans4mersError::Internal(format!("Failed to create directories: {}", e))
            })?;
        }

        fs::write(&safe_path, content)
            .map_err(|e| Trans4mersError::Internal(format!("File write failed: {}", e)))
    }
}

/// Pure SHA-256 hex digest (no external crate: uses the standard algorithm
/// implemented over 64-byte blocks).
#[allow(clippy::chunks_exact_to_as_chunks)]
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    let mut w = [0u32; 64];
    for chunk in msg.chunks_exact(64) {
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut out = String::with_capacity(64);
    for word in h {
        out.push_str(&format!("{:08x}", word));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nested_path_creation_allowed() {
        let unique = format!("t4m_test_{}", uuid::Uuid::new_v4());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();

        let nested = root.join("deep").join("nested").join("file.rs");
        let validated = FileSystemGuard::validate_path_in_workspace(&root, &nested).unwrap();
        assert!(validated.starts_with(root.canonicalize().unwrap()));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_traversal_blocked() {
        let unique = format!("t4m_test_{}", uuid::Uuid::new_v4());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();

        let breakout = root.join("..").join("outside.rs");
        let res = FileSystemGuard::validate_path_in_workspace(&root, &breakout);
        assert!(res.is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_write_and_read_nested_workspace_file() {
        let unique = format!("t4m_test_{}", uuid::Uuid::new_v4());
        let root = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&root).unwrap();

        FileSystemGuard::write_workspace_file(&root, "a/b/c/test.txt", "hello world").unwrap();
        let content = FileSystemGuard::read_workspace_file(&root, "a/b/c/test.txt").unwrap();
        assert_eq!(content, "hello world");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_sha256_hex() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b"The quick brown fox jumps over the lazy dog"),
            "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
        );
    }
}
