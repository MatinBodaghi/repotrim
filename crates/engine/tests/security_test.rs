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
    let resolved = guard
        .resolve(&safe_file)
        .expect("Contained file must resolve");
    assert!(resolved.ends_with("main.rs"));

    // 2. Relative path resolution inside root
    let resolved_rel = guard
        .resolve("workspace/src/main.rs")
        .expect("Relative path must resolve");
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

#[test]
fn test_cache_isolation_ignores_hostile_repo_cache_fixtures() {
    use repotrim_engine::{get_cache_dir_for_root, get_cache_file_for_root, LoadedRepository};
    use std::io::Write;

    let temp_repo = std::env::temp_dir().join(format!(
        "repotrim_hostile_cache_test_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_repo);

    let src_dir = temp_repo.join("src");
    let hostile_cache_dir = temp_repo.join(".repotrim");
    fs::create_dir_all(&src_dir).unwrap();
    fs::create_dir_all(&hostile_cache_dir).unwrap();

    // Legitimate source file
    let lib_rs = src_dir.join("lib.rs");
    let mut f = File::create(&lib_rs).unwrap();
    writeln!(f, "pub fn legitimate_fn() -> u32 {{ 1337 }}").unwrap();

    // Plant a hostile, forged .repotrim/cache.bin inside target repository
    let hostile_cache_bin = hostile_cache_dir.join("cache.bin");
    let forged_payload = b"FORGED_MALICIOUS_CACHE_BINARY_DATA_WITH_POISONED_SYMBOLS";
    fs::write(&hostile_cache_bin, forged_payload).unwrap();

    // Plant a hostile .repotrim/coedit.bin inside target repository
    let hostile_coedit_bin = hostile_cache_dir.join("coedit.bin");
    fs::write(&hostile_coedit_bin, b"FORGED_COEDIT_DATA").unwrap();

    // Clean any pre-existing external cache for this test directory
    let external_cache_dir = get_cache_dir_for_root(&temp_repo);
    let external_cache_file = get_cache_file_for_root(&temp_repo);
    let _ = fs::remove_dir_all(&external_cache_dir);

    // Load repository with caching enabled
    let loaded = LoadedRepository::load(&temp_repo).expect("Repository load must succeed");

    // Invariant 1: Symbols must be parsed exclusively from clean disk source code
    assert_eq!(loaded.symbols.len(), 1);
    assert_eq!(loaded.symbols[0].name, "legitimate_fn");

    // Invariant 2: The hostile repository cache inside <repo>/.repotrim/ must be untouched and ignored
    assert!(hostile_cache_bin.exists());
    let hostile_content = fs::read(&hostile_cache_bin).unwrap();
    assert_eq!(hostile_content, forged_payload);

    // Invariant 3: The legitimate binary cache must be written to external OS cache directory
    assert!(
        external_cache_file.exists(),
        "External cache file must be written to dirs::cache_dir(), not repo tree"
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_repo);
    let _ = fs::remove_dir_all(&external_cache_dir);
}

#[test]
fn test_git_revision_argument_injection_blocked() {
    use repotrim_engine::diff::{is_valid_git_revision, DiffResolver};
    use repotrim_engine::EngineError;

    let malicious_revisions = [
        "--output=/tmp/leak",
        "-o/tmp/leak",
        "--exec=touch /tmp/hacked",
        "--upload-pack=touch /tmp/hacked",
        "-c core.hooksPath=/tmp",
        "; rm -rf /",
        "HEAD & touch /tmp/pwned",
        "HEAD; echo pwned",
        "| cat /etc/passwd",
        "> /tmp/overwrite",
        "`id`",
        "$(whoami)",
    ];

    let temp_dir = std::env::temp_dir();
    for rev in malicious_revisions {
        assert!(
            !is_valid_git_revision(rev),
            "Revision '{}' should be flagged as invalid",
            rev
        );

        let err = DiffResolver::get_git_diff_against(&temp_dir, rev)
            .expect_err("Malicious revision must return an error");
        assert!(
            matches!(err, EngineError::InvalidInput(_)),
            "Expected InvalidInput error for '{}', got {:?}",
            rev,
            err
        );
    }
}
