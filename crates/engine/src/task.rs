use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;

use crate::graph::LayerWeights;
use crate::intent::IntentResolver;
use crate::parser::SupportedLanguage;

/// High-level operational classification of an agent coding task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum TaskKind {
    /// Defect correction, regression fix, or panic resolution.
    Bugfix,
    /// New feature implementation, endpoint addition, or functionality extension.
    Feature,
    /// Structural reorganization, type refactoring, or debt elimination.
    Refactor,
    /// Test harness creation, unit test authoring, or coverage expansion.
    TestCreation,
    /// Repository comprehension, architecture exploration, or code navigation.
    Exploration,
    /// Documentation authoring, README updates, or API documentation.
    Documentation,
    /// General or unclassified programming task.
    General,
}

impl TaskKind {
    /// Infers task kind heuristically from query text tokens.
    pub fn infer(query: &str) -> Self {
        let lower = query.to_lowercase();
        let tokens: Vec<String> = IntentResolver::tokenize(&lower);

        let has_any = |keywords: &[&str]| tokens.iter().any(|t| keywords.contains(&t.as_str()));

        if has_any(&[
            "fix",
            "bug",
            "error",
            "panic",
            "issue",
            "fail",
            "failure",
            "crash",
            "regression",
        ]) {
            TaskKind::Bugfix
        } else if has_any(&["test", "tests", "spec", "assert", "coverage", "benchmark"]) {
            TaskKind::TestCreation
        } else if has_any(&[
            "refactor",
            "cleanup",
            "reorganize",
            "decouple",
            "rename",
            "simplify",
        ]) {
            TaskKind::Refactor
        } else if has_any(&[
            "add",
            "implement",
            "create",
            "support",
            "feature",
            "new",
            "extend",
        ]) {
            TaskKind::Feature
        } else if has_any(&[
            "doc",
            "docs",
            "documentation",
            "guide",
            "readme",
            "explain",
            "architecture",
            "comment",
        ]) {
            TaskKind::Documentation
        } else if has_any(&[
            "where", "find", "locate", "trace", "how", "explore", "search",
        ]) {
            TaskKind::Exploration
        } else {
            TaskKind::General
        }
    }

    /// Canonical string identifier for this task kind.
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Bugfix => "bugfix",
            TaskKind::Feature => "feature",
            TaskKind::Refactor => "refactor",
            TaskKind::TestCreation => "test_creation",
            TaskKind::Exploration => "exploration",
            TaskKind::Documentation => "documentation",
            TaskKind::General => "general",
        }
    }
}

impl fmt::Display for TaskKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Operational metadata and filtering constraints associated with an agent task.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TaskMetadata {
    /// Expected task category.
    pub kind: Option<TaskKind>,
    /// Target file paths or globs of interest.
    #[serde(default)]
    pub target_files: Vec<PathBuf>,
    /// Language restriction if known.
    #[serde(default)]
    pub target_language: Option<SupportedLanguage>,
    /// User or agent specified token constraint $B$.
    #[serde(default)]
    pub max_budget: Option<usize>,
    /// Arbitrary key-value metadata attributes.
    #[serde(default)]
    pub attributes: HashMap<String, String>,
}

/// Formal structured task representation $q = (x, z, m)$ for codebase intelligence.
///
/// Encapsulates:
/// - $x$: raw task prompt / natural language query.
/// - $z$: extracted concepts, identifiers, and seed hints.
/// - $m$: operational metadata, task category, and constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskContext {
    /// Raw natural-language task prompt or user request $x$.
    pub query: String,
    /// Extracted task concepts, keywords, and entity tokens $z$.
    pub concepts: Vec<String>,
    /// Explicitly supplied or candidate symbol seed hints.
    #[serde(default)]
    pub seed_hints: Vec<String>,
    /// Operational task metadata, category, and constraints $m$.
    #[serde(default)]
    pub metadata: TaskMetadata,
}

impl TaskContext {
    /// Creates a new `TaskContext` from a natural language query string,
    /// automatically extracting concept tokens and inferring task kind.
    pub fn from_query(query: &str) -> Self {
        let concepts = IntentResolver::tokenize(query);
        let kind = TaskKind::infer(query);

        Self {
            query: query.to_string(),
            concepts,
            seed_hints: Vec::new(),
            metadata: TaskMetadata {
                kind: Some(kind),
                ..Default::default()
            },
        }
    }

    /// Creates a new `TaskContext` from a natural language prompt string.
    #[inline]
    pub fn from_prompt(prompt: &str) -> Self {
        Self::from_query(prompt)
    }

    /// Creates a `TaskContext` with explicit seed symbol hints.
    pub fn with_seeds(query: &str, seeds: Vec<String>) -> Self {
        let mut ctx = Self::from_query(query);
        ctx.seed_hints = seeds;
        ctx
    }

    /// Attaches explicit `TaskMetadata` to this context.
    pub fn with_metadata(mut self, metadata: TaskMetadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Returns the effective `TaskKind`.
    pub fn kind(&self) -> TaskKind {
        self.metadata
            .kind
            .unwrap_or_else(|| TaskKind::infer(&self.query))
    }

    /// Calibrates multiplex edge layer weights $\omega_r(q)$ conditioned on task intent.
    ///
    /// Dynamically shifts probability mass in PageRank transition matrices:
    /// - `Bugfix`: Boosts call graphs and historical commit co-edits.
    /// - `TestCreation`: Boosts test linkages and call references.
    /// - `Refactor`: Boosts structural containment and type relationships.
    /// - `Documentation` / `Exploration`: Balances global modularity with imports.
    pub fn conditioned_layer_weights(&self, base: &LayerWeights) -> LayerWeights {
        match self.kind() {
            TaskKind::Bugfix => LayerWeights {
                ast_parent: base.ast_parent * 0.8,
                call: base.call * 1.5,
                type_ref: base.type_ref * 1.1,
                import: base.import * 1.0,
                co_edit: base.co_edit * 1.8,
            },
            TaskKind::TestCreation => LayerWeights {
                ast_parent: base.ast_parent * 0.7,
                call: base.call * 1.6,
                type_ref: base.type_ref * 1.4,
                import: base.import * 1.2,
                co_edit: base.co_edit * 1.1,
            },
            TaskKind::Refactor => LayerWeights {
                ast_parent: base.ast_parent * 1.6,
                call: base.call * 1.1,
                type_ref: base.type_ref * 1.6,
                import: base.import * 1.3,
                co_edit: base.co_edit * 0.9,
            },
            TaskKind::Documentation | TaskKind::Exploration => LayerWeights {
                ast_parent: base.ast_parent * 1.3,
                call: base.call * 1.0,
                type_ref: base.type_ref * 1.2,
                import: base.import * 1.4,
                co_edit: base.co_edit * 0.7,
            },
            TaskKind::Feature | TaskKind::General => *base,
        }
    }
}

impl fmt::Display for TaskContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "TaskContext(kind={}, query=\"{}\", concepts={})",
            self.kind(),
            self.query,
            self.concepts.len()
        )
    }
}
