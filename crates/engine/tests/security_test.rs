use repotrim_engine::{RootGuard, SecurityError};
use std::fs::{self, File};
use std::path::PathBuf;

#[test]
fn test_root_guard_confinement_invariants() {
    let temp = std::env::temp_dir().join(format!("repotrim_test_conf_{}", std::process::id()));
    let safe_dir = temp.join("workspace").join("src");
    fs::create_dir_all(&safe_dir).unwrap();

    let safe_file = safe_dir.join("main.rs");
    File::create(&safe_file).unwrap();

    let guard = RootGuard::with_root(&temp);

    // 1. Valid file inside root
    let resolved = guard.resolve(&safe_file).expect("Contained file must resolve");
    assert!(resolved.ends_with("main.rs"));

    // 2. Relative path resolution inside root
    let resolved_rel = guard.resolve("workspace/src/main.rs").expect("Relative path must resolve");
    assert!(resolved_rel.ends_with("main.rs"));

    // 3. Parent escape attempts
    let escape_rel = guard.resolve("workspace/../../outside.rs");
    assert!(
        matches!(escape_rel, Err(SecurityError::PathEscapesRoot { .. })),
        "Parent escape must be blocked"
    );

    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn test_root_guard_blocks_arbitrary_external_directories() {
    let temp = std::env::temp_dir().join(format!("repotrim_test_ext_{}", std::process::id()));
    fs::create_dir_all(&temp).unwrap();

    let guard = RootGuard::with_root(&temp);

    #[cfg(windows)]
    let outside_path = PathBuf::from(r"C:\Windows\System32");
    #[cfg(not(windows))]
    let outside_path = PathBuf::from("/etc/passwd");

    if outside_path.exists() {
        let result = guard.resolve(&outside_path);
        assert!(
            matches!(result, Err(SecurityError::PathEscapesRoot { .. })),
            "External path '{:?}' must be rejected by RootGuard",
            outside_path
        );
    }

    let _ = fs::remove_dir_all(&temp);
}

#[test]
fn test_multiple_roots_confinement() {
    let base = std::env::temp_dir().join(format!("repotrim_multi_test_{}", std::process::id()));
    let root1 = base.join("repo_a");
    let root2 = base.join("repo_b");
    let forbidden = base.join("repo_c");

    fs::create_dir_all(&root1).unwrap();
    fs::create_dir_all(&root2).unwrap();
    fs::create_dir_all(&forbidden).unwrap();

    let file1 = root1.join("a.rs");
    let file2 = root2.join("b.rs");
    let file_forbidden = forbidden.join("c.rs");

    File::create(&file1).unwrap();
    File::create(&file2).unwrap();
    File::create(&file_forbidden).unwrap();

    let guard = RootGuard::new(vec![&root1, &root2]);

    assert!(guard.is_allowed(&file1));
    assert!(guard.is_allowed(&file2));
    assert!(!guard.is_allowed(&file_forbidden));

    let _ = fs::remove_dir_all(&base);
}
