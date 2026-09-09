use repotrim_engine::{
    AstExtractor, CelfConfig, ContextSelector, EdgeKind, LoadedRepository, PprConfig, SymbolKind,
};
use std::fs;
use std::path::Path;

#[test]
fn test_go_ast_extraction() {
    let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

    let go_source = r#"package server

// Config holds service configuration.
type Config struct {
    Port int
    Host string
}

// Handler handles incoming requests.
type Handler interface {
    Handle(path string) error
}

// Server manages network connections.
type Server struct {
    cfg Config
}

// Start launches the network server.
func (s *Server) Start() error {
    logMessage("starting server")
    return nil
}

// Stop shuts down the network server.
func (s Server) Stop() error {
    logMessage("stopping server")
    return nil
}

func NewServer(cfg Config) *Server {
    return &Server{cfg: cfg}
}

func logMessage(msg string) {
    println(msg)
}
"#;

    let mut next_id = 0;
    let (symbols, edges, imports) = extractor
        .parse_file_with_imports(
            Path::new("pkg/server/server.go"),
            go_source.as_bytes(),
            &mut next_id,
        )
        .expect("Failed to parse Go source");

    let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        symbol_names.contains(&"Config"),
        "Expected Config struct, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"Handler"),
        "Expected Handler interface, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"Server"),
        "Expected Server struct, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"Start"),
        "Expected Start method, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"Stop"),
        "Expected Stop method, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"NewServer"),
        "Expected NewServer function, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"logMessage"),
        "Expected logMessage function, got: {:?}",
        symbol_names
    );

    // Verify method classification
    let start_sym = symbols
        .iter()
        .find(|s| s.name == "Start")
        .expect("Start not found");
    assert_eq!(start_sym.kind, SymbolKind::Method);
    assert!(
        start_sym.docstring.is_some(),
        "Expected docstring on Start method"
    );

    let stop_sym = symbols
        .iter()
        .find(|s| s.name == "Stop")
        .expect("Stop not found");
    assert_eq!(stop_sym.kind, SymbolKind::Method);

    // Verify AstParent edges from methods -> Server struct
    let start_parent = edges.iter().any(|e| {
        e.source == start_sym.id && e.target_ident == "Server" && e.kind == EdgeKind::AstParent
    });
    assert!(start_parent, "Expected AstParent edge from Start to Server");

    let stop_parent = edges.iter().any(|e| {
        e.source == stop_sym.id && e.target_ident == "Server" && e.kind == EdgeKind::AstParent
    });
    assert!(stop_parent, "Expected AstParent edge from Stop to Server");

    // Verify Call edge from Start -> logMessage
    let has_call = edges.iter().any(|e| {
        e.source == start_sym.id && e.target_ident == "logMessage" && e.kind == EdgeKind::Call
    });
    assert!(has_call, "Expected Call edge from Start to logMessage");

    // Verify imports empty for this file
    assert!(imports.is_empty());
}

#[test]
fn test_go_package_multi_file_resolution_e2e() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_go_pkg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(temp_dir.join("pkg").join("auth")).expect("Failed to create pkg/auth");
    fs::create_dir_all(temp_dir.join("cmd")).expect("Failed to create cmd");

    // pkg/auth/auth.go
    let auth_path = temp_dir.join("pkg").join("auth").join("auth.go");
    fs::write(
        &auth_path,
        r#"package auth

type Authenticator struct {
    Secret string
}

func (a *Authenticator) VerifyToken(token string) bool {
    return validateHash(token)
}
"#,
    )
    .unwrap();

    // pkg/auth/hash.go (same package directory)
    let hash_path = temp_dir.join("pkg").join("auth").join("hash.go");
    fs::write(
        &hash_path,
        r#"package auth

func validateHash(token string) bool {
    return len(token) > 10
}
"#,
    )
    .unwrap();

    // cmd/main.go (imports pkg/auth)
    let main_path = temp_dir.join("cmd").join("main.go");
    fs::write(
        &main_path,
        r#"package main

import "myproject/pkg/auth"

func main() {
    authService := &auth.Authenticator{Secret: "key"}
    authService.VerifyToken("secret-token")
}
"#,
    )
    .unwrap();

    // Load repository
    let loaded = LoadedRepository::load_with_options(&temp_dir, false)
        .expect("Failed to load Go package repository");

    assert_eq!(
        loaded.cache_report.total_files, 3,
        "Expected 3 Go files loaded"
    );

    let symbol_names: Vec<&str> = loaded.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(symbol_names.contains(&"Authenticator"));
    assert!(symbol_names.contains(&"VerifyToken"));
    assert!(symbol_names.contains(&"validateHash"));
    assert!(symbol_names.contains(&"main"));

    // Build graph
    let graph = loaded.build_graph();
    assert_eq!(graph.num_symbols(), 4);

    // Select context seeded at main
    let main_sym = loaded
        .symbols
        .iter()
        .find(|s| s.name == "main")
        .expect("main symbol not found");

    let selector = ContextSelector::new(PprConfig::default(), CelfConfig::default());
    let (selected_syms, markdown) =
        selector.select_and_format_context(&graph, &[main_sym.id], 500, &loaded.file_sources);

    assert!(!selected_syms.is_empty());
    assert!(
        markdown.contains("### File: `cmd/main.go`")
            || markdown.contains("### File: `cmd\\main.go`"),
        "Expected cmd/main.go in markdown output, got:\n{}",
        markdown
    );
    assert!(
        markdown.contains("```go"),
        "Expected Go code block in markdown output, got:\n{}",
        markdown
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_quad_polyglot_integration() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_quad_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // 1. Rust backend
    let rs_path = temp_dir.join("core.rs");
    fs::write(
        &rs_path,
        r#"
pub struct RustCore {
    pub id: u64,
}
pub fn init_core() -> RustCore {
    RustCore { id: 42 }
}
"#,
    )
    .unwrap();

    // 2. Python ML script
    let py_path = temp_dir.join("model.py");
    fs::write(
        &py_path,
        r#"
class Predictor:
    def predict(self, x: float) -> float:
        return x * 2.0
"#,
    )
    .unwrap();

    // 3. TypeScript UI component
    let ts_path = temp_dir.join("widget.ts");
    fs::write(
        &ts_path,
        r#"
export interface WidgetProps {
    visible: boolean;
}
export function renderWidget(props: WidgetProps): void {}
"#,
    )
    .unwrap();

    // 4. Go microservice
    let go_path = temp_dir.join("service.go");
    fs::write(
        &go_path,
        r#"package service

type WorkerPool struct {
    Size int
}

func (w *WorkerPool) Dispatch() error {
    return nil
}
"#,
    )
    .unwrap();

    // Ingest all 4 languages simultaneously
    let loaded = LoadedRepository::load_with_options(&temp_dir, false)
        .expect("Failed to load quad-polyglot repository");

    assert_eq!(
        loaded.cache_report.total_files, 4,
        "Expected 4 polyglot files"
    );

    let symbol_names: Vec<&str> = loaded.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(symbol_names.contains(&"RustCore"));
    assert!(symbol_names.contains(&"Predictor"));
    assert!(symbol_names.contains(&"WidgetProps"));
    assert!(symbol_names.contains(&"WorkerPool"));
    assert!(symbol_names.contains(&"Dispatch"));

    let graph = loaded.build_graph();
    assert!(graph.num_symbols() >= 7);

    // Select context seeded at Go "WorkerPool"
    let wp_sym = loaded
        .symbols
        .iter()
        .find(|s| s.name == "WorkerPool")
        .expect("WorkerPool symbol not found");

    let selector = ContextSelector::new(PprConfig::default(), CelfConfig::default());
    let (_selected_syms, markdown) =
        selector.select_and_format_context(&graph, &[wp_sym.id], 500, &loaded.file_sources);

    assert!(
        markdown.contains("### File: `service.go`"),
        "Expected service.go in markdown output, got:\n{}",
        markdown
    );
    assert!(
        markdown.contains("```go"),
        "Expected Go code block in markdown output, got:\n{}",
        markdown
    );

    let _ = fs::remove_dir_all(&temp_dir);
}
