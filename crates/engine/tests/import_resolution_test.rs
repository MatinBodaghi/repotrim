use repotrim_engine::{LoadedRepository, ScopedResolver};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

fn setup_file(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("Failed to create parent dir");
    }
    let mut file = File::create(path).expect("Failed to create file");
    file.write_all(content.as_bytes())
        .expect("Failed to write file");
}

#[test]
fn test_rust_cross_module_import_disambiguation() {
    let temp_dir =
        std::env::temp_dir().join(format!("repotrim_test_rust_imports_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);

    // Two identical function names in different modules
    let parser_path = temp_dir.join("src/parser/mod.rs");
    setup_file(
        &parser_path,
        r#"
pub fn parse_file() -> usize {
    42
}
"#,
    );

    let other_path = temp_dir.join("src/other/mod.rs");
    setup_file(
        &other_path,
        r#"
pub fn parse_file() -> usize {
    99
}
"#,
    );

    // Caller explicitly imports the parser version
    let main_path = temp_dir.join("src/main.rs");
    setup_file(
        &main_path,
        r#"
use crate::parser::parse_file;

pub fn main() {
    let _ = parse_file();
}
"#,
    );

    let repo = LoadedRepository::load(&temp_dir).expect("Failed to load repository");
    assert_eq!(repo.symbols.len(), 3);
    assert!(!repo.imports.is_empty(), "Imports should be extracted");

    let resolver = ScopedResolver::with_imports(&repo.symbols, &repo.imports);
    let main_sym = repo
        .symbols
        .iter()
        .find(|s| s.name == "main")
        .expect("main symbol not found");

    let resolved = resolver
        .resolve(main_sym, "parse_file")
        .expect("Failed to resolve parse_file");

    assert_eq!(
        resolved.1, 1.0,
        "Imported symbol should resolve with exact 1.0 confidence"
    );

    let target_sym = repo
        .symbols
        .iter()
        .find(|s| s.id == resolved.0)
        .expect("Target symbol not found");

    assert_eq!(
        target_sym.file_path,
        PathBuf::from("src/parser/mod.rs"),
        "Should resolve to src/parser/mod.rs, NOT src/other/mod.rs"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_python_relative_and_aliased_import_resolution() {
    let temp_dir = std::env::temp_dir().join(format!(
        "repotrim_test_python_imports_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);

    let user_model_path = temp_dir.join("app/models/user.py");
    setup_file(
        &user_model_path,
        r#"
class User:
    def get_id(self) -> int:
        return 1

class Role:
    pass
"#,
    );

    let auth_service_path = temp_dir.join("app/services/auth.py");
    setup_file(
        &auth_service_path,
        r#"
from ..models.user import User as AppUser, Role

def login(user_id: int):
    u = AppUser()
    return u
"#,
    );

    let repo = LoadedRepository::load(&temp_dir).expect("Failed to load repository");
    assert!(repo.symbols.len() >= 3);
    assert!(!repo.imports.is_empty());

    let resolver = ScopedResolver::with_imports(&repo.symbols, &repo.imports);
    let login_sym = repo
        .symbols
        .iter()
        .find(|s| s.name == "login")
        .expect("login symbol not found");

    // Resolving aliased name AppUser
    let (target_id, conf) = resolver
        .resolve(login_sym, "AppUser")
        .expect("Should resolve AppUser");
    assert_eq!(conf, 1.0);

    let target_sym = repo
        .symbols
        .iter()
        .find(|s| s.id == target_id)
        .expect("Target not found");
    assert_eq!(target_sym.name, "User");
    assert_eq!(target_sym.file_path, PathBuf::from("app/models/user.py"));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_typescript_default_and_named_import_resolution() {
    let temp_dir =
        std::env::temp_dir().join(format!("repotrim_test_ts_imports_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);

    let button_path = temp_dir.join("src/components/Button.tsx");
    setup_file(
        &button_path,
        r#"
export default function Button() {
    return null;
}

export function formatLabel(text: string): string {
    return text.trim();
}
"#,
    );

    let home_path = temp_dir.join("src/views/Home.tsx");
    setup_file(
        &home_path,
        r#"
import CustomButton, { formatLabel as fmt } from '../components/Button';

export function Home() {
    const label = fmt("test");
    return null;
}
"#,
    );

    let repo = LoadedRepository::load(&temp_dir).expect("Failed to load repository");
    assert!(repo.symbols.len() >= 3);

    let resolver = ScopedResolver::with_imports(&repo.symbols, &repo.imports);
    let home_sym = repo
        .symbols
        .iter()
        .find(|s| s.name == "Home")
        .expect("Home symbol not found");

    // Check named import with alias
    let (fmt_id, fmt_conf) = resolver
        .resolve(home_sym, "fmt")
        .expect("Should resolve fmt");
    assert_eq!(fmt_conf, 1.0);
    let fmt_sym = repo.symbols.iter().find(|s| s.id == fmt_id).unwrap();
    assert_eq!(fmt_sym.name, "formatLabel");

    // Check default import alias
    let (btn_id, btn_conf) = resolver
        .resolve(home_sym, "CustomButton")
        .expect("Should resolve CustomButton");
    assert_eq!(btn_conf, 1.0);
    let btn_sym = repo.symbols.iter().find(|s| s.id == btn_id).unwrap();
    assert_eq!(btn_sym.name, "Button");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_cache_persistence_and_hit_ratio_with_imports() {
    let temp_dir = std::env::temp_dir().join(format!(
        "repotrim_test_cache_imports_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&temp_dir);

    let file_a = temp_dir.join("src/math.rs");
    setup_file(
        &file_a,
        r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#,
    );

    let file_b = temp_dir.join("src/calc.rs");
    setup_file(
        &file_b,
        r#"
use crate::math::add;

pub fn calculate() -> i32 {
    add(1, 2)
}
"#,
    );

    // First load: cold cache
    let repo1 = LoadedRepository::load(&temp_dir).expect("Failed first load");
    assert_eq!(repo1.cache_report.cached_files, 0);
    assert_eq!(repo1.cache_report.recomputed_files, 2);
    assert_eq!(repo1.cache_report.hit_ratio, 0.0);
    assert!(!repo1.imports.is_empty());

    // Build graph from repo1
    let graph1 = repo1.build_graph();
    assert!(graph1.num_edges() > 0);

    // Second load: warm cache from disk
    let repo2 = LoadedRepository::load(&temp_dir).expect("Failed second load");
    assert_eq!(repo2.cache_report.cached_files, 2);
    assert_eq!(repo2.cache_report.recomputed_files, 0);
    assert_eq!(repo2.cache_report.hit_ratio, 1.0);
    assert_eq!(repo1.imports, repo2.imports);

    // Build graph from repo2 - must be structurally identical
    let graph2 = repo2.build_graph();
    assert_eq!(graph1.num_symbols(), graph2.num_symbols());
    assert_eq!(graph1.num_edges(), graph2.num_edges());

    let _ = fs::remove_dir_all(&temp_dir);
}
