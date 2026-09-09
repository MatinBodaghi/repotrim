//! Target model profiles and context headroom presets.
//!
//! Provides predefined configurations for major LLMs (Claude 3.5 Sonnet, GPT-4o,
//! DeepSeek-V3, Local Ollama), tailoring the Kneedle token budget optimizer to the
//! model's effective context window and optimal attention density.

use serde::{Deserialize, Serialize};

/// Predefined model profile governing auto-budget bounds and knee sensitivity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModelProfile {
    /// Canonical model identifier name.
    pub name: &'static str,
    /// Total nominal context window size in tokens.
    pub context_window: usize,
    /// Minimum prompt context floor (tokens) required for non-trivial code tasks.
    pub min_budget: usize,
    /// Maximum ceiling for context prompt packing (tokens) before diminishing returns set in.
    pub max_budget: usize,
    /// Kneedle sensitivity parameter $S$.
    pub sensitivity: f32,
}

impl Default for ModelProfile {
    fn default() -> Self {
        Self::claude_3_5_sonnet()
    }
}

impl ModelProfile {
    /// Anthropic Claude 3.5 Sonnet preset (200k context window, high reasoning density).
    pub const fn claude_3_5_sonnet() -> Self {
        Self {
            name: "claude-3-5-sonnet",
            context_window: 200_000,
            min_budget: 800,
            max_budget: 6_000,
            sensitivity: 1.0,
        }
    }

    /// OpenAI GPT-4o preset (128k context window).
    pub const fn gpt_4o() -> Self {
        Self {
            name: "gpt-4o",
            context_window: 128_000,
            min_budget: 600,
            max_budget: 5_000,
            sensitivity: 1.0,
        }
    }

    /// DeepSeek-V3 / DeepSeek-Coder preset (64k context window).
    pub const fn deepseek_v3() -> Self {
        Self {
            name: "deepseek-v3",
            context_window: 64_000,
            min_budget: 500,
            max_budget: 4_000,
            sensitivity: 1.1,
        }
    }

    /// Local Ollama / compact SLM preset (e.g. Llama 3 8B, Qwen 2.5 Coder, 8k-16k window).
    pub const fn local_ollama() -> Self {
        Self {
            name: "local-ollama",
            context_window: 16_000,
            min_budget: 400,
            max_budget: 2_000,
            sensitivity: 1.2,
        }
    }

    /// Generic balanced baseline preset.
    pub const fn generic() -> Self {
        Self {
            name: "generic",
            context_window: 64_000,
            min_budget: 600,
            max_budget: 4_000,
            sensitivity: 1.0,
        }
    }

    /// Parses a user-supplied model name string into the closest matching `ModelProfile`.
    ///
    /// Accepts flexible common names:
    /// - `"claude"`, `"sonnet"`, `"claude-3-5-sonnet"` -> `Claude 3.5 Sonnet`
    /// - `"gpt"`, `"gpt-4o"`, `"gpt4o"`, `"openai"` -> `GPT-4o`
    /// - `"deepseek"`, `"deepseek-v3"`, `"deepseek-coder"` -> `DeepSeek-V3`
    /// - `"ollama"`, `"local"`, `"llama"` -> `Local Ollama`
    /// - Anything else falls back to `Generic` (or `Claude 3.5 Sonnet`).
    pub fn parse(s: &str) -> Self {
        let norm = s.trim().to_lowercase();
        if norm.contains("sonnet") || norm.contains("claude") || norm.contains("anthropic") {
            Self::claude_3_5_sonnet()
        } else if norm.contains("gpt") || norm.contains("openai") || norm.contains("4o") {
            Self::gpt_4o()
        } else if norm.contains("deepseek") {
            Self::deepseek_v3()
        } else if norm.contains("ollama")
            || norm.contains("local")
            || norm.contains("llama")
            || norm.contains("qwen")
        {
            Self::local_ollama()
        } else {
            Self::generic()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_profile_parse() {
        assert_eq!(ModelProfile::parse("claude").name, "claude-3-5-sonnet");
        assert_eq!(ModelProfile::parse("sonnet-3.5").name, "claude-3-5-sonnet");
        assert_eq!(ModelProfile::parse("gpt-4o").name, "gpt-4o");
        assert_eq!(ModelProfile::parse("deepseek-coder").name, "deepseek-v3");
        assert_eq!(ModelProfile::parse("ollama-llama3").name, "local-ollama");
        assert_eq!(ModelProfile::parse("unknown-custom").name, "generic");
    }

    #[test]
    fn test_model_profile_bounds() {
        let p = ModelProfile::claude_3_5_sonnet();
        assert!(p.min_budget < p.max_budget);
        assert!(p.max_budget < p.context_window);
    }
}
