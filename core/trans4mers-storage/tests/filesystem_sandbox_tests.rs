use std::fs;
use trans4mers_domain::error::Trans4mersError;
use trans4mers_storage::filesystem::{FileSystemGuard, sha256_hex};

fn create_test_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("t4m_fs_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_filesystem_sandbox_within_workspace() {
    let workspace_root = create_test_dir();

    // 1. Write file in workspace root
    let write_res = FileSystemGuard::write_workspace_file(&workspace_root, "hello.txt", "hello world");
    assert!(write_res.is_ok(), "Writing inside workspace must succeed");

    // 2. Read file from workspace root
    let read_res = FileSystemGuard::read_workspace_file(&workspace_root, "hello.txt");
    assert_eq!(read_res.unwrap(), "hello world");

    // 3. Write file in nested subdirectory
    let nested_write = FileSystemGuard::write_workspace_file(
        &workspace_root,
        "src/components/button.rs",
        "pub struct Button;",
    );
    assert!(nested_write.is_ok(), "Writing in nested workspace subdirectories must succeed");

    let nested_read = FileSystemGuard::read_workspace_file(&workspace_root, "src/components/button.rs");
    assert_eq!(nested_read.unwrap(), "pub struct Button;");

    let _ = fs::remove_dir_all(&workspace_root);
}

#[test]
fn test_filesystem_sandbox_rejects_parent_traversal() {
    let workspace_root = create_test_dir();

    // Attempt simple parent directory traversal
    let escape_res = FileSystemGuard::read_workspace_file(&workspace_root, "../outside.txt");
    assert!(
        matches!(escape_res, Err(Trans4mersError::PathOutsideWorkspace(_))),
        "Parent directory traversal '../' must be rejected"
    );

    // Attempt multi-hop parent traversal
    let deep_escape = FileSystemGuard::read_workspace_file(&workspace_root, "../../../Windows/System32/cmd.exe");
    assert!(
        matches!(deep_escape, Err(Trans4mersError::PathOutsideWorkspace(_))),
        "Deep parent traversal '../../../' must be rejected"
    );

    let _ = fs::remove_dir_all(&workspace_root);
}

#[test]
fn test_filesystem_sandbox_rejects_nested_parent_traversal() {
    let workspace_root = create_test_dir();

    // Create a subfolder
    let subfolder = workspace_root.join("subfolder");
    fs::create_dir_all(&subfolder).unwrap();

    // Attempt traversal from inside subfolder
    let target = subfolder.join("../../outside.txt");
    let validate_res = FileSystemGuard::validate_path_in_workspace(&workspace_root, &target);
    assert!(
        matches!(validate_res, Err(Trans4mersError::PathOutsideWorkspace(_))),
        "Traversal breaking out of workspace from subfolder must be rejected"
    );

    let _ = fs::remove_dir_all(&workspace_root);
}

#[test]
fn test_filesystem_sandbox_rejects_absolute_path_outside_workspace() {
    let workspace_root = create_test_dir();

    // Create another directory completely outside workspace
    let other_dir = create_test_dir();
    let outside_file = other_dir.join("secret.txt");
    fs::write(&outside_file, "secret credentials").unwrap();

    let validate_res = FileSystemGuard::validate_path_in_workspace(&workspace_root, &outside_file);
    assert!(
        matches!(validate_res, Err(Trans4mersError::PathOutsideWorkspace(_))),
        "Absolute path pointing outside workspace must be rejected"
    );

    let _ = fs::remove_dir_all(&workspace_root);
    let _ = fs::remove_dir_all(&other_dir);
}

#[test]
fn test_filesystem_sandbox_sha256_checksum_deterministic() {
    let data1 = b"trans4mers-sovereign-agent-os";
    let data2 = b"trans4mers-sovereign-agent-os";
    let data_diff = b"trans4mers-tampered-agent-os";

    let hash1 = sha256_hex(data1);
    let hash2 = sha256_hex(data2);
    let hash_diff = sha256_hex(data_diff);

    assert_eq!(hash1, hash2, "Identical bytes must produce identical hex hash");
    assert_ne!(hash1, hash_diff, "Different bytes must produce different hex hash");
    assert_eq!(hash1.len(), 64, "SHA-256 hex string must be 64 characters");
}
