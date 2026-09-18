use repotrim_engine::{LoadedRepository, DEFAULT_MAX_FILE_BYTES};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[test]
fn test_gitignore_exclusions_honored() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_gitignore_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).unwrap();
    fs::create_dir_all(temp_dir.join("vendor")).unwrap();

    // Create .gitignore
    let gitignore_path = temp_dir.join(".gitignore");
    fs::write(
        &gitignore_path,
        "vendor/\nsecret.rs\n*.ignored.rs\n",
    )
    .unwrap();

    // Included file
    let lib_file = temp_dir.join("src/lib.rs");
    fs::write(&lib_file, "pub fn active_code() -> u32 { 100 }\n").unwrap();

    // Excluded files via gitignore
    let vendor_file = temp_dir.join("vendor/bundle.rs");
    fs::write(&vendor_file, "pub fn vendor_code() {}\n").unwrap();

    let secret_file = temp_dir.join("src/secret.rs");
    fs::write(&secret_file, "pub fn secret_key() {}\n").unwrap();

    let ignored_glob = temp_dir.join("src/data.ignored.rs");
    fs::write(&ignored_glob, "pub fn ignored_by_glob() {}\n").unwrap();

    let repo = LoadedRepository::load_with_options(&temp_dir, false)
        .expect("Repository load must succeed");

    assert_eq!(repo.cache_report.total_files, 1);
    assert_eq!(repo.symbols.len(), 1);
    assert_eq!(repo.symbols[0].name, "active_code");
    assert_eq!(repo.max_file_bytes, DEFAULT_MAX_FILE_BYTES);
    assert!(repo.file_sources.keys().any(|p| p == Path::new("src/lib.rs")));
    assert!(!repo.file_sources.keys().any(|p| p.to_string_lossy().contains("vendor")));
    assert!(!repo.file_sources.keys().any(|p| p.to_string_lossy().contains("secret")));
    assert!(!repo.file_sources.keys().any(|p| p.to_string_lossy().contains("ignored")));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_max_file_bytes_limit_skips_oversized_files() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_max_bytes_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).unwrap();

    // Small file (50 bytes)
    let small_file = temp_dir.join("src/small.rs");
    fs::write(&small_file, "pub fn small() -> u32 { 42 }\n").unwrap();

    // Oversized file (> 500 bytes)
    let huge_file = temp_dir.join("src/huge.rs");
    let mut f = File::create(&huge_file).unwrap();
    for _ in 0..200 {
        writeln!(f, "pub fn huge_filler_function_to_exceed_limit() {{}}").unwrap();
    }

    // Load with 200 bytes limit
    let repo = LoadedRepository::load_with_config(&temp_dir, false, 200)
        .expect("Repository load must succeed with size limit");

    assert_eq!(repo.cache_report.total_files, 1);
    assert_eq!(repo.cache_report.skipped_files, 1);
    assert_eq!(repo.symbols.len(), 1);
    assert_eq!(repo.symbols[0].name, "small");
    assert!(repo.file_sources.contains_key(Path::new("src/small.rs")));
    assert!(!repo.file_sources.contains_key(Path::new("src/huge.rs")));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_default_ignored_directories_without_gitignore() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_ignored_dirs_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).unwrap();
    fs::create_dir_all(temp_dir.join(".venv/lib")).unwrap();
    fs::create_dir_all(temp_dir.join("node_modules/pkg")).unwrap();
    fs::create_dir_all(temp_dir.join("__pycache__")).unwrap();
    fs::create_dir_all(temp_dir.join("target/debug")).unwrap();

    let valid = temp_dir.join("src/index.ts");
    fs::write(&valid, "export function run(): void {}\n").unwrap();

    let py_venv = temp_dir.join(".venv/lib/site.py");
    fs::write(&py_venv, "def site_func(): pass\n").unwrap();

    let node_pkg = temp_dir.join("node_modules/pkg/mod.js");
    fs::write(&node_pkg, "function pkg() {}\n").unwrap();

    let py_cache = temp_dir.join("__pycache__/cached.py");
    fs::write(&py_cache, "def cache_func(): pass\n").unwrap();

    let target_file = temp_dir.join("target/debug/build.rs");
    fs::write(&target_file, "fn main() {}\n").unwrap();

    let repo = LoadedRepository::load_with_options(&temp_dir, false)
        .expect("Repository load must succeed");

    assert_eq!(repo.cache_report.total_files, 1);
    assert_eq!(repo.symbols.len(), 1);
    assert_eq!(repo.symbols[0].name, "run");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_traversal_cycle_defense_with_symlinks() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_symlink_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    let sub_dir = temp_dir.join("nested");
    fs::create_dir_all(&sub_dir).unwrap();

    let file = sub_dir.join("test.rs");
    fs::write(&file, "pub fn normal_func() -> bool { true }\n").unwrap();

    let cycle_link = sub_dir.join("loop_link");
    let mut symlink_created = false;

    #[cfg(unix)]
    {
        if std::os::unix::fs::symlink(&sub_dir, &cycle_link).is_ok() {
            symlink_created = true;
        }
    }

    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(&sub_dir, &cycle_link).is_ok() {
            symlink_created = true;
        }
    }

    let repo = LoadedRepository::load_with_options(&temp_dir, false)
        .expect("Repository load must terminate cleanly without infinite loop");

    assert_eq!(repo.symbols.len(), 1);
    assert_eq!(repo.symbols[0].name, "normal_func");
    assert_eq!(repo.cache_report.total_files, 1);

    if symlink_created {
        // Confirm symlink loop did not duplicate symbols or cause stack overflow
        assert_eq!(repo.symbols.len(), 1);
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
