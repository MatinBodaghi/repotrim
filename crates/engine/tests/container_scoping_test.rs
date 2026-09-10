use repotrim_engine::{AstExtractor, ContextFormatter, LodLevel};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[test]
fn test_rust_container_scoping_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let rust_source = r#"
pub struct DirEntry {
    path: String,
}

impl DirEntry {
    /// Constructs a new DirEntry.
    pub fn new(path: String) -> Self {
        Self { path }
    }

    /// Returns the entry path.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl Clone for DirEntry {
    fn clone(&self) -> Self {
        Self { path: self.path.clone() }
    }
}

pub fn standalone_helper() -> bool {
    true
}
"#;

    let path = Path::new("src/entry.rs");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, rust_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Rust source");

    // Verify container and trait metadata extraction
    let new_sym = symbols.iter().find(|s| s.name == "new").unwrap();
    assert_eq!(new_sym.container_name.as_deref(), Some("DirEntry"));
    assert_eq!(new_sym.trait_name, None);

    let path_sym = symbols.iter().find(|s| s.name == "path").unwrap();
    assert_eq!(path_sym.container_name.as_deref(), Some("DirEntry"));
    assert_eq!(path_sym.trait_name, None);

    let clone_sym = symbols.iter().find(|s| s.name == "clone").unwrap();
    assert_eq!(clone_sym.container_name.as_deref(), Some("DirEntry"));
    assert_eq!(clone_sym.trait_name.as_deref(), Some("Clone"));

    let struct_sym = symbols.iter().find(|s| s.name == "DirEntry").unwrap();
    assert_eq!(struct_sym.container_name, None);

    let helper_sym = symbols
        .iter()
        .find(|s| s.name == "standalone_helper")
        .unwrap();
    assert_eq!(helper_sym.container_name, None);

    // Format markdown
    let mut lod_map = HashMap::new();
    for s in &symbols {
        lod_map.insert(s.id, LodLevel::SignatureOnly);
    }
    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/entry.rs"), rust_source.to_string());

    let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

    // Verify Markdown structure
    assert!(md.contains("### File: `src/entry.rs`"));
    assert!(md.contains("```rust"));

    // Verify inherent impl block with indented methods
    assert!(md.contains("impl DirEntry {\n    // Lines"));
    assert!(md.contains("    pub fn new(path: String) -> Self;"));
    assert!(md.contains("    pub fn path(&self) -> &str;"));

    // Verify trait impl block with indented clone
    assert!(md.contains("impl Clone for DirEntry {\n    // Lines"));
    assert!(md.contains("    fn clone(&self) -> Self;"));

    // Verify standalone symbols at top level
    assert!(md.contains("pub struct DirEntry {"));
    assert!(md.contains("pub fn standalone_helper() -> bool;"));
}

#[test]
fn test_rust_duplicate_method_names_in_different_traits() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let rust_source = r#"
pub struct Worker;

impl Worker {
    pub fn process(&self) {}
}

impl TaskHandler for Worker {
    fn process(&self) {}
}

impl BatchProcessor for Worker {
    fn process(&self) {}
}
"#;

    let path = Path::new("src/worker.rs");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, rust_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Rust source");

    let process_syms: Vec<_> = symbols.iter().filter(|s| s.name == "process").collect();
    assert_eq!(process_syms.len(), 3);

    let inherent = process_syms
        .iter()
        .find(|s| s.trait_name.is_none())
        .unwrap();
    assert_eq!(inherent.container_name.as_deref(), Some("Worker"));

    let task_handler = process_syms
        .iter()
        .find(|s| s.trait_name.as_deref() == Some("TaskHandler"))
        .unwrap();
    assert_eq!(task_handler.container_name.as_deref(), Some("Worker"));

    let batch_proc = process_syms
        .iter()
        .find(|s| s.trait_name.as_deref() == Some("BatchProcessor"))
        .unwrap();
    assert_eq!(batch_proc.container_name.as_deref(), Some("Worker"));

    let mut lod_map = HashMap::new();
    for s in &symbols {
        lod_map.insert(s.id, LodLevel::SignatureOnly);
    }
    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/worker.rs"), rust_source.to_string());

    let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

    // Each identical method is cleanly partitioned in its own container
    assert!(md.contains("impl Worker {\n    // Lines"));
    assert!(md.contains("impl TaskHandler for Worker {\n    // Lines"));
    assert!(md.contains("impl BatchProcessor for Worker {\n    // Lines"));
}

#[test]
fn test_python_class_scoping_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let python_source = r#"
class AccountService:
    """Service managing user financial accounts."""
    def __init__(self, db_conn):
        """Initialize database connection."""
        self.db = db_conn

    def get_balance(self, account_id: int) -> float:
        """Return current account balance."""
        return 100.0

def calculate_interest(balance: float) -> float:
    """Standalone interest calculation helper."""
    return balance * 0.05
"#;

    let path = Path::new("services/account.py");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, python_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Python source");

    let init_sym = symbols.iter().find(|s| s.name == "__init__").unwrap();
    assert_eq!(init_sym.container_name.as_deref(), Some("AccountService"));

    let balance_sym = symbols.iter().find(|s| s.name == "get_balance").unwrap();
    assert_eq!(
        balance_sym.container_name.as_deref(),
        Some("AccountService")
    );

    let interest_sym = symbols
        .iter()
        .find(|s| s.name == "calculate_interest")
        .unwrap();
    assert_eq!(interest_sym.container_name, None);

    let mut lod_map = HashMap::new();
    for s in &symbols {
        lod_map.insert(s.id, LodLevel::SignatureAndDoc);
    }
    let mut sources = HashMap::new();
    sources.insert(
        PathBuf::from("services/account.py"),
        python_source.to_string(),
    );

    let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

    assert!(md.contains("### File: `services/account.py`"));
    assert!(md.contains("```python"));

    // Verify class docstring and nested method indentation
    assert!(md.contains("class AccountService:"));
    assert!(md.contains("    # Lines"));
    assert!(md.contains("    def __init__(self, db_conn):"));
    assert!(md.contains("    def get_balance(self, account_id: int) -> float:"));

    // Verify standalone function at root
    assert!(md.contains("def calculate_interest(balance: float) -> float:"));
}

#[test]
fn test_typescript_class_scoping_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let ts_source = r#"
export class PaymentGateway {
    private apiKey: string;

    public charge(amount: number): boolean {
        return true;
    }

    public refund(transactionId: string): boolean {
        return true;
    }
}

function validateAmount(amount: number): boolean {
    return amount > 0;
}
"#;

    let path = Path::new("src/payment.ts");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, ts_source.as_bytes(), &mut next_id)
        .expect("Failed to parse TypeScript source");

    let charge_sym = symbols.iter().find(|s| s.name == "charge").unwrap();
    assert_eq!(charge_sym.container_name.as_deref(), Some("PaymentGateway"));

    let refund_sym = symbols.iter().find(|s| s.name == "refund").unwrap();
    assert_eq!(refund_sym.container_name.as_deref(), Some("PaymentGateway"));

    let validate_sym = symbols.iter().find(|s| s.name == "validateAmount").unwrap();
    assert_eq!(validate_sym.container_name, None);

    let mut lod_map = HashMap::new();
    for s in &symbols {
        lod_map.insert(s.id, LodLevel::SignatureOnly);
    }
    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("src/payment.ts"), ts_source.to_string());

    let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

    assert!(md.contains("### File: `src/payment.ts`"));
    assert!(md.contains("```typescript"));
    assert!(md.contains("class PaymentGateway {\n    // Lines"));
    assert!(md.contains("    public charge(amount: number): boolean;"));
    assert!(md.contains("    public refund(transactionId: string): boolean;"));
    assert!(md.contains("function validateAmount(amount: number): boolean;"));
}

#[test]
fn test_go_receiver_method_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let go_source = r#"
package main

type Dispatcher struct {
    workers int
}

func (d *Dispatcher) Dispatch(job string) error {
    return nil
}

func (d *Dispatcher) Status() string {
    return "ready"
}

func GlobalInit() {
}
"#;

    let path = Path::new("dispatcher.go");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, go_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Go source");

    let dispatch_sym = symbols.iter().find(|s| s.name == "Dispatch").unwrap();
    assert_eq!(dispatch_sym.container_name.as_deref(), Some("Dispatcher"));

    let status_sym = symbols.iter().find(|s| s.name == "Status").unwrap();
    assert_eq!(status_sym.container_name.as_deref(), Some("Dispatcher"));

    let global_sym = symbols.iter().find(|s| s.name == "GlobalInit").unwrap();
    assert_eq!(global_sym.container_name, None);

    let mut lod_map = HashMap::new();
    for s in &symbols {
        lod_map.insert(s.id, LodLevel::SignatureOnly);
    }
    let mut sources = HashMap::new();
    sources.insert(PathBuf::from("dispatcher.go"), go_source.to_string());

    let md = ContextFormatter::format_markdown(&symbols, &lod_map, &sources);

    assert!(md.contains("### File: `dispatcher.go`"));
    assert!(md.contains("```go"));

    // Idiomatic Go top-level receiver functions
    assert!(md.contains("func (d *Dispatcher) Dispatch(job string) error"));
    assert!(md.contains("func (d *Dispatcher) Status() string"));
    assert!(md.contains("func GlobalInit()"));

    // Must not be wrapped in invalid synthetic blocks
    assert!(!md.contains("class Dispatcher"));
    assert!(!md.contains("impl Dispatcher"));
}
