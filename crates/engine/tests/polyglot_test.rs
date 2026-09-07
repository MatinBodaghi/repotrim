use repotrim_engine::{
    AstExtractor, CelfConfig, ContextSelector, EdgeKind, LoadedRepository, PprConfig, SymbolKind,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

#[test]
fn test_python_ast_extraction() {
    let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

    let python_source = r#"
"""Module docstring for user service."""

class UserResponse:
    """User response data transfer object."""
    id: int
    username: str

def format_user(user_id: int, username: str) -> UserResponse:
    """Formats raw user data into response."""
    return UserResponse()

class UserManager:
    """Manages user operations."""
    def get_user(self, user_id: int) -> UserResponse:
        """Retrieves and formats a user."""
        res = format_user(user_id, "alice")
        return res
"#;

    let mut next_id = 0;
    let (symbols, edges) = extractor
        .parse_file(
            Path::new("services/user.py"),
            python_source.as_bytes(),
            &mut next_id,
        )
        .expect("Failed to parse Python source");

    let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        symbol_names.contains(&"UserResponse"),
        "Expected UserResponse class, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"format_user"),
        "Expected format_user function, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"UserManager"),
        "Expected UserManager class, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"get_user"),
        "Expected get_user method, got: {:?}",
        symbol_names
    );

    // Verify method kind
    let get_user_sym = symbols
        .iter()
        .find(|s| s.name == "get_user")
        .expect("get_user not found");
    assert_eq!(get_user_sym.kind, SymbolKind::Method);
    assert!(
        get_user_sym.docstring.is_some(),
        "Expected docstring on get_user"
    );

    // Verify AstParent edge from get_user -> UserManager
    let has_parent = edges.iter().any(|e| {
        e.source == get_user_sym.id
            && e.target_ident == "UserManager"
            && e.kind == EdgeKind::AstParent
    });
    assert!(
        has_parent,
        "Expected AstParent edge from get_user to UserManager"
    );

    // Verify call edge from get_user -> format_user
    let has_call = edges.iter().any(|e| {
        e.source == get_user_sym.id && e.target_ident == "format_user" && e.kind == EdgeKind::Call
    });
    assert!(has_call, "Expected Call edge from get_user to format_user");
}

#[test]
fn test_typescript_ast_extraction() {
    let extractor = AstExtractor::new().expect("Failed to initialize AstExtractor");

    let ts_source = r#"
/**
 * Interface representing user configuration.
 */
export interface UserConfig {
    timeoutMs: number;
    retries: number;
}

export type Status = "active" | "inactive";

export class ApiClient {
    private config: UserConfig;

    /**
     * Executes HTTP request.
     */
    public executeRequest(endpoint: string): Status {
        notifyLogger(endpoint);
        return "active";
    }
}

export function notifyLogger(msg: string): void {
    console.log(msg);
}
"#;

    let mut next_id = 0;
    let (symbols, edges) = extractor
        .parse_file(
            Path::new("src/client.ts"),
            ts_source.as_bytes(),
            &mut next_id,
        )
        .expect("Failed to parse TypeScript source");

    let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(
        symbol_names.contains(&"UserConfig"),
        "Expected UserConfig interface, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"Status"),
        "Expected Status type alias, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"ApiClient"),
        "Expected ApiClient class, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"executeRequest"),
        "Expected executeRequest method, got: {:?}",
        symbol_names
    );
    assert!(
        symbol_names.contains(&"notifyLogger"),
        "Expected notifyLogger function, got: {:?}",
        symbol_names
    );

    let execute_sym = symbols
        .iter()
        .find(|s| s.name == "executeRequest")
        .expect("executeRequest not found");
    assert_eq!(execute_sym.kind, SymbolKind::Method);

    // Verify AstParent edge from executeRequest -> ApiClient
    let has_parent = edges.iter().any(|e| {
        e.source == execute_sym.id && e.target_ident == "ApiClient" && e.kind == EdgeKind::AstParent
    });
    assert!(
        has_parent,
        "Expected AstParent edge from executeRequest to ApiClient"
    );

    // Verify Call edge from executeRequest -> notifyLogger
    let has_call = edges.iter().any(|e| {
        e.source == execute_sym.id && e.target_ident == "notifyLogger" && e.kind == EdgeKind::Call
    });
    assert!(
        has_call,
        "Expected Call edge from executeRequest to notifyLogger"
    );
}

#[test]
fn test_polyglot_repository_e2e_selection() {
    let temp_dir = std::env::temp_dir().join(format!("repotrim_polyglot_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    // Rust backend file
    let rs_path = temp_dir.join("main.rs");
    let mut rs_file = File::create(&rs_path).unwrap();
    writeln!(
        rs_file,
        r#"
pub struct RustCore {{
    pub version: u32,
}}

pub fn run_core() -> RustCore {{
    RustCore {{ version: 1 }}
}}
"#
    )
    .unwrap();

    // Python data pipeline file
    let py_path = temp_dir.join("pipeline.py");
    let mut py_file = File::create(&py_path).unwrap();
    writeln!(
        py_file,
        r#"
class DataProcessor:
    def process(self, data: str) -> bool:
        clean_data(data)
        return True

def clean_data(raw: str):
    pass
"#
    )
    .unwrap();

    // TypeScript web component file
    let ts_path = temp_dir.join("app.tsx");
    let mut ts_file = File::create(&ts_path).unwrap();
    writeln!(
        ts_file,
        r#"
export interface AppProps {{
    title: string;
}}

export function App(props: AppProps) {{
    renderUI();
}}

function renderUI() {{}}
"#
    )
    .unwrap();

    // Ingest multi-language repository
    let loaded = LoadedRepository::load_with_options(&temp_dir, false)
        .expect("Failed to load polyglot repository");

    assert_eq!(
        loaded.cache_report.total_files, 3,
        "Expected 3 polyglot files scanned"
    );

    let symbol_names: Vec<&str> = loaded.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(symbol_names.contains(&"RustCore"));
    assert!(symbol_names.contains(&"DataProcessor"));
    assert!(symbol_names.contains(&"process"));
    assert!(symbol_names.contains(&"AppProps"));
    assert!(symbol_names.contains(&"App"));

    // Build multiplex graph across Rust, Python, and TypeScript
    let graph = loaded.build_graph();
    assert!(graph.num_symbols() >= 6);

    // Select context seeded around Python "DataProcessor"
    let dp_sym = loaded
        .symbols
        .iter()
        .find(|s| s.name == "DataProcessor")
        .expect("DataProcessor symbol not found");

    let selector = ContextSelector::new(PprConfig::default(), CelfConfig::default());
    let (_selected_syms, markdown) =
        selector.select_and_format_context(&graph, &[dp_sym.id], 500, &loaded.file_sources);

    assert!(
        markdown.contains("### File: `pipeline.py`"),
        "Expected pipeline.py in markdown output, got:\n{}",
        markdown
    );
    assert!(
        markdown.contains("```python"),
        "Expected python code block in markdown output, got:\n{}",
        markdown
    );

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}
