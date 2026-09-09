use repotrim_engine::{is_ignored_path, LoadedRepository, RepositoryWatcher, WatcherEvent};
use std::fs;
use std::path::Path;
use std::sync::{mpsc, Arc, RwLock};
use std::time::Duration;

#[test]
fn test_is_ignored_path() {
    let root = Path::new("/workspace/project");

    assert!(is_ignored_path(
        Path::new("/workspace/project/.git/HEAD"),
        root
    ));
    assert!(is_ignored_path(
        Path::new("/workspace/project/target/debug/foo.rs"),
        root
    ));
    assert!(is_ignored_path(
        Path::new("/workspace/project/node_modules/pkg/index.js"),
        root
    ));
    assert!(is_ignored_path(
        Path::new("/workspace/project/.repotrim/cache.bin"),
        root
    ));
    assert!(is_ignored_path(
        Path::new("/workspace/project/.gemini/prompt.md"),
        root
    ));

    assert!(!is_ignored_path(
        Path::new("/workspace/project/src/lib.rs"),
        root
    ));
    assert!(!is_ignored_path(
        Path::new("/workspace/project/crates/engine/src/watcher.rs"),
        root
    ));
    assert!(!is_ignored_path(
        Path::new("/workspace/project/tests/integration.rs"),
        root
    ));
}

#[test]
fn test_live_repository_watcher_synchronization() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_watch_test_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("src")).unwrap();

    let file_rs = temp_dir.join("src/lib.rs");
    fs::write(
        &file_rs,
        "pub fn compute_sum(a: i32, b: i32) -> i32 { a + b }\n",
    )
    .unwrap();

    let loaded = LoadedRepository::load(&temp_dir).expect("Failed to load repo");
    assert_eq!(loaded.symbols.len(), 1);
    assert_eq!(loaded.symbols[0].name, "compute_sum");

    let initial_graph = loaded.build_graph();
    assert_eq!(initial_graph.num_symbols(), 1);

    let repo_arc = Arc::new(RwLock::new(loaded));
    let graph_arc = Arc::new(RwLock::new(initial_graph));
    let (event_tx, event_rx) = mpsc::channel();

    let watcher = RepositoryWatcher::start_with_debounce(
        Arc::clone(&repo_arc),
        Some(Arc::clone(&graph_arc)),
        Some(event_tx),
        Duration::from_millis(50),
    )
    .expect("Failed to start RepositoryWatcher");

    assert!(watcher.is_running());

    // 1. Modify existing file
    fs::write(
        &file_rs,
        "pub fn compute_sum(a: i32, b: i32) -> i32 { a + b }\npub fn compute_product(a: i32, b: i32) -> i32 { a * b }\n",
    )
    .unwrap();

    // Wait for debounced watcher event (up to 3 seconds)
    let mut patched = false;
    let timeout = Duration::from_secs(3);
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Ok(WatcherEvent::GraphUpdated { num_symbols: 2, .. }) =
            event_rx.recv_timeout(Duration::from_millis(100))
        {
            patched = true;
            break;
        }
    }
    assert!(patched, "Expected GraphUpdated event with 2 symbols");

    // Verify in-memory graph updated
    {
        let g = graph_arc.read().unwrap();
        assert_eq!(g.num_symbols(), 2);
        let names: Vec<_> = g.symbols().iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"compute_sum"));
        assert!(names.contains(&"compute_product"));
    }

    // 2. Add a new polyglot file
    let file_py = temp_dir.join("src/helper.py");
    fs::write(&file_py, "def helper_func():\n    pass\n").unwrap();

    let mut py_patched = false;
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Ok(WatcherEvent::GraphUpdated { num_symbols: 3, .. }) =
            event_rx.recv_timeout(Duration::from_millis(100))
        {
            py_patched = true;
            break;
        }
    }
    assert!(py_patched, "Expected GraphUpdated event with 3 symbols");

    {
        let g = graph_arc.read().unwrap();
        assert_eq!(g.num_symbols(), 3);
    }

    // 3. Ignored directory activity should not trigger any events
    fs::create_dir_all(temp_dir.join("target/debug")).unwrap();
    fs::write(
        temp_dir.join("target/debug/build.rs"),
        "pub fn ignored() {}\n",
    )
    .unwrap();
    // Flush briefly; no event for target/debug/build.rs should arrive
    std::thread::sleep(Duration::from_millis(150));
    while let Ok(event) = event_rx.try_recv() {
        if let WatcherEvent::FilePatched { ref path } = event {
            assert!(!path.to_string_lossy().contains("target"));
        }
    }

    // 4. Remove file
    fs::remove_file(&file_py).unwrap();
    let mut py_removed = false;
    let deadline = std::time::Instant::now() + timeout;
    while std::time::Instant::now() < deadline {
        if let Ok(WatcherEvent::GraphUpdated { num_symbols: 2, .. }) =
            event_rx.recv_timeout(Duration::from_millis(100))
        {
            py_removed = true;
            break;
        }
    }
    assert!(py_removed, "Expected GraphUpdated event with 2 symbols");

    {
        let g = graph_arc.read().unwrap();
        assert_eq!(g.num_symbols(), 2);
    }

    // Stop watcher cleanly
    watcher.stop();
    let _ = fs::remove_dir_all(&temp_dir);
}
