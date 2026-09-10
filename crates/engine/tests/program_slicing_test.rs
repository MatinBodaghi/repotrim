use repotrim_engine::{AstExtractor, ContextFormatter, LodLevel};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[test]
fn test_rust_ast_slicing_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let rust_source = r#"
pub fn process_transaction(req: &Request) -> Result<Response, Error> {
    let a = 1;
    let b = 2;
    let c = 3;
    let d = 4;
    let e = 5;
    if req.is_invalid() {
        return Err(Error::Invalid);
    }
    let x = 10;
    let y = 20;
    let z = 30;
    let resp = execute_call(req)?;
    Ok(resp)
}
"#;

    let path = Path::new("src/tx.rs");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, rust_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Rust source");

    let tx_sym = symbols
        .iter()
        .find(|s| s.name == "process_transaction")
        .expect("process_transaction symbol not found");

    let sliced = ContextFormatter::render_symbol(tx_sym, LodLevel::SlicedBody, Some(rust_source));

    // Linear variable declarations are elided into concise line-count markers
    assert!(sliced.contains("// ... [5 lines elided] ..."));
    assert!(sliced.contains("// ... [3 lines elided] ..."));

    // Critical control flow branch, error return, call with ?, and final Ok return are preserved
    assert!(sliced.contains("if req.is_invalid()"));
    assert!(sliced.contains("return Err(Error::Invalid);"));
    assert!(sliced.contains("execute_call(req)?"));
    assert!(sliced.contains("Ok(resp)"));

    // Local variable declarations should be absent
    assert!(!sliced.contains("let a = 1;"));
    assert!(!sliced.contains("let x = 10;"));
}

#[test]
fn test_python_ast_slicing_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let py_source = r#"
def handle_request(payload: dict) -> dict:
    """Validates and processes payload."""
    x = 1
    y = 2
    z = 3
    w = 4
    if not payload:
        raise ValueError("empty")
    a = 10
    b = 20
    c = 30
    try:
        res = service.execute(payload)
        return res
    except Exception as e:
        logger.error(e)
        raise
"#;

    let path = Path::new("service.py");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, py_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Python source");

    let handle_sym = symbols
        .iter()
        .find(|s| s.name == "handle_request")
        .expect("handle_request symbol not found");

    let sliced = ContextFormatter::render_symbol(handle_sym, LodLevel::SlicedBody, Some(py_source));

    // Docstring is preserved
    assert!(sliced.contains("\"\"\"Validates and processes payload.\"\"\""));

    // Elided blocks
    assert!(sliced.contains("# ... [4 lines elided] ..."));
    assert!(sliced.contains("# ... [3 lines elided] ..."));

    // Control flow, error raise, and try/except call preserved
    assert!(sliced.contains("if not payload:"));
    assert!(sliced.contains("raise ValueError(\"empty\")"));
    assert!(sliced.contains("try:"));
    assert!(sliced.contains("res = service.execute(payload)"));
    assert!(sliced.contains("return res"));
    assert!(sliced.contains("except Exception as e:"));

    // Local assignments absent
    assert!(!sliced.contains("x = 1"));
    assert!(!sliced.contains("a = 10"));
}

#[test]
fn test_typescript_ast_slicing_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let ts_source = r#"
export class PaymentService {
    processPayment(amount: number): boolean {
        const fee = 10;
        const tax = 5;
        const discount = 2;
        const total = amount + fee;
        if (amount <= 0) {
            throw new Error("invalid amount");
        }
        const step1 = total * 2;
        const step2 = step1 / 2;
        return this.gateway.charge(total);
    }
}
"#;

    let path = Path::new("src/payment.ts");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, ts_source.as_bytes(), &mut next_id)
        .expect("Failed to parse TypeScript source");

    let method_sym = symbols
        .iter()
        .find(|s| s.name == "processPayment")
        .expect("processPayment symbol not found");

    let sliced = ContextFormatter::render_symbol(method_sym, LodLevel::SlicedBody, Some(ts_source));

    assert!(sliced.contains("// ... [4 lines elided] ..."));
    assert!(sliced.contains("if (amount <= 0)"));
    assert!(sliced.contains("throw new Error(\"invalid amount\");"));
    assert!(sliced.contains("// ... [2 lines elided] ..."));
    assert!(sliced.contains("return this.gateway.charge(total);"));

    assert!(!sliced.contains("const fee = 10;"));
    assert!(!sliced.contains("const step1 = total * 2;"));
}

#[test]
fn test_go_ast_slicing_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");
    let go_source = r#"
package service

type Service struct{}

func (s *Service) Execute(data string) (string, error) {
    a := 1
    b := 2
    c := 3
    d := 4
    if len(data) == 0 {
        return "", errors.New("empty data")
    }
    x := 10
    y := 20
    z := 30
    res, err := s.client.Call(data)
    if err != nil {
        return "", err
    }
    return res, nil
}
"#;

    let path = Path::new("service.go");
    let mut next_id = 0;
    let (symbols, _, _) = extractor
        .parse_file_with_imports(path, go_source.as_bytes(), &mut next_id)
        .expect("Failed to parse Go source");

    let exec_sym = symbols
        .iter()
        .find(|s| s.name == "Execute")
        .expect("Execute symbol not found");

    let sliced = ContextFormatter::render_symbol(exec_sym, LodLevel::SlicedBody, Some(go_source));

    assert!(sliced.contains("// ... [4 lines elided] ..."));
    assert!(sliced.contains("if len(data) == 0"));
    assert!(sliced.contains("return \"\", errors.New(\"empty data\")"));
    assert!(sliced.contains("// ... [3 lines elided] ..."));
    assert!(sliced.contains("res, err := s.client.Call(data)"));
    assert!(sliced.contains("if err != nil"));
    assert!(sliced.contains("return res, nil"));

    assert!(!sliced.contains("a := 1"));
    assert!(!sliced.contains("x := 10"));
}

#[test]
fn test_causal_topological_file_ordering_e2e() {
    let extractor = AstExtractor::new().expect("Failed to create AstExtractor");

    let models_src = r#"
pub struct UserProfile {
    pub id: u64,
}

pub struct SessionToken {
    pub raw: String,
}
"#;

    let routes_src = r#"
pub fn authenticate_route(token: &SessionToken) -> UserProfile {
    UserProfile { id: 1 }
}
"#;

    let main_src = r#"
pub fn run_server(handler: authenticate_route) {
    println!("running");
}
"#;

    let models_path = PathBuf::from("src/models.rs");
    let routes_path = PathBuf::from("src/routes.rs");
    let main_path = PathBuf::from("src/main.rs");

    let mut next_id = 0;
    let (syms_models, _, _) = extractor
        .parse_file_with_imports(&models_path, models_src.as_bytes(), &mut next_id)
        .unwrap();
    let (syms_routes, _, _) = extractor
        .parse_file_with_imports(&routes_path, routes_src.as_bytes(), &mut next_id)
        .unwrap();
    let (syms_main, _, _) = extractor
        .parse_file_with_imports(&main_path, main_src.as_bytes(), &mut next_id)
        .unwrap();

    let mut all_symbols = Vec::new();
    all_symbols.extend(syms_main);
    all_symbols.extend(syms_routes);
    all_symbols.extend(syms_models);

    let mut lod_map = HashMap::new();
    for sym in &all_symbols {
        lod_map.insert(sym.id, LodLevel::SignatureOnly);
    }

    let mut sources = HashMap::new();
    sources.insert(models_path.clone(), models_src.to_string());
    sources.insert(routes_path.clone(), routes_src.to_string());
    sources.insert(main_path.clone(), main_src.to_string());

    let md = ContextFormatter::format_markdown(&all_symbols, &lod_map, &sources);

    // Alphabetical ordering would be:
    // 1. src/main.rs
    // 2. src/models.rs
    // 3. src/routes.rs
    //
    // Causal topological ordering requires:
    // 1. src/models.rs (callee & type definition for routes)
    // 2. src/routes.rs (callee for main)
    // 3. src/main.rs (caller orchestrator)
    let idx_models = md
        .find("### File: `src/models.rs`")
        .expect("models.rs section missing");
    let idx_routes = md
        .find("### File: `src/routes.rs`")
        .expect("routes.rs section missing");
    let idx_main = md
        .find("### File: `src/main.rs`")
        .expect("main.rs section missing");

    assert!(
        idx_models < idx_routes,
        "models.rs ({}) must precede routes.rs ({})",
        idx_models,
        idx_routes
    );
    assert!(
        idx_routes < idx_main,
        "routes.rs ({}) must precede main.rs ({})",
        idx_routes,
        idx_main
    );
}
