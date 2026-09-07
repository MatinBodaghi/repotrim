/// Estimates the token count of a given text using a fast BPE heuristic.
///
/// Counts words, punctuation marks, and CamelCase transitions without heavy dependencies,
/// while applying a baseline floor of `(text.len() + 3) / 4`.
pub fn estimate_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }

    #[allow(clippy::manual_div_ceil)]
    let baseline_floor = (text.len() + 3) / 4;

    let mut token_count = 0;
    let mut in_word = false;
    let mut prev_char: Option<char> = None;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c.is_whitespace() {
            in_word = false;
            prev_char = Some(c);
            continue;
        }

        // Multi-character punctuation operators common in Rust
        let is_multi_punct = matches!(
            (c, chars.peek().copied()),
            (':', Some(':'))
                | ('-', Some('>'))
                | ('=', Some('>'))
                | ('=', Some('='))
                | ('!', Some('='))
                | ('<', Some('='))
                | ('>', Some('='))
        );

        if is_multi_punct {
            let next_c = chars.next().unwrap();
            token_count += 1;
            in_word = false;
            prev_char = Some(next_c);
            continue;
        }

        if c.is_ascii_punctuation() {
            token_count += 1;
            in_word = false;
            prev_char = Some(c);
            continue;
        }

        if c.is_alphanumeric() {
            if !in_word {
                token_count += 1;
                in_word = true;
            } else if let Some(prev) = prev_char {
                let is_camel_transition = (prev.is_lowercase() && c.is_uppercase())
                    || (prev.is_alphabetic() && c.is_ascii_digit())
                    || (prev.is_ascii_digit() && c.is_alphabetic())
                    || (prev.is_uppercase()
                        && c.is_uppercase()
                        && chars.peek().is_some_and(|next| next.is_lowercase()));

                if is_camel_transition {
                    token_count += 1;
                }
            }
            prev_char = Some(c);
            continue;
        }

        // Fallback for unicode symbols
        token_count += 1;
        in_word = false;
        prev_char = Some(c);
    }

    token_count.max(baseline_floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_and_whitespace() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("   \n\t  "), 0);
    }

    #[test]
    fn test_simple_signature() {
        let sig = "pub fn add(a: i32, b: i32) -> i32";
        let count = estimate_tokens(sig);
        // Expect sensible token count: > 10 and within reasonable BPE bounds
        assert!((14..=22).contains(&count), "Count was {}", count);
    }

    #[test]
    fn test_complex_generic_signature() {
        let sig = "pub fn parse_file<P: AsRef<Path>>(&self, path: P, next_id: &mut u32) -> Result<(Vec<SymbolNode>, Vec<ReferenceEdge>), EngineError>";
        let count = estimate_tokens(sig);
        // Signature length is ~132 chars, floor is 33
        assert!((33..=60).contains(&count), "Count was {}", count);
    }

    #[test]
    fn test_camel_case_transitions() {
        // "ASTExtractor" (12 chars): 2 words by transition ("AST", "Extractor"),
        // but floor is (12 + 3) / 4 = 3.
        let text = "ASTExtractor";
        assert_eq!(estimate_tokens(text), 3);

        // "FastBPE" (7 chars): "Fast", "BPE" -> 2 tokens. Floor is (7 + 3) / 4 = 2.
        assert_eq!(estimate_tokens("FastBPE"), 2);

        // "myVarName" (9 chars): "my", "Var", "Name" -> 3 tokens. Floor is (9 + 3) / 4 = 3.
        assert_eq!(estimate_tokens("myVarName"), 3);
    }

    #[test]
    #[allow(clippy::manual_div_ceil)]
    fn test_baseline_floor() {
        // High character density with few punctuation/word transitions
        let text = "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"; // 52 chars
        let floor = (52 + 3) / 4; // 13
        assert_eq!(estimate_tokens(text), floor);
    }
}
