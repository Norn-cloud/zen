//! Norn: documents where the two regex backends of `matches()` / `extract()` differ.
//!
//! zen-expression uses `regex` with the `regex-deprecated` feature (upstream default)
//! and `regex-lite` otherwise (`regex-lite` feature, or the Norn pure profile with no
//! regex feature). `regex-lite` is not Unicode-aware beyond matching codepoint by
//! codepoint: `\d`, `\s`, `\w` and `\b` are ASCII-only, `\p{..}` / `\P{..}` do not
//! compile, and `(?i)` only folds ASCII case. Which backend is part of the Norn
//! language profile is a Norn-side decision (see NORN.md); this table pins the
//! current behaviour of both so any change is visible.

use serde_json::json;
use zen_expression::evaluate_expression;
use zen_expression::variable::Variable;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Outcome {
    Match,
    NoMatch,
    /// The pattern does not compile (`matches()` returns an error).
    Invalid,
}

use Outcome::*;

struct Case {
    pattern: &'static str,
    input: &'static str,
    regex: Outcome,
    regex_lite: Outcome,
}

#[rustfmt::skip]
const CASES: &[Case] = &[
    // Agreement: ASCII patterns and input.
    Case { pattern: r"^[a-z]+$", input: "abc", regex: Match, regex_lite: Match },
    Case { pattern: r"^\d{3}-\d{4}$", input: "555-1234", regex: Match, regex_lite: Match },
    Case { pattern: r"^\w+@\w+\.com$", input: "a_b@example.com", regex: Match, regex_lite: Match },
    Case { pattern: r"(?i)^hello$", input: "HeLLo", regex: Match, regex_lite: Match },
    Case { pattern: r"^\s+$", input: " \t\n", regex: Match, regex_lite: Match },
    Case { pattern: r"\bcat\b", input: "a cat sat", regex: Match, regex_lite: Match },
    Case { pattern: r"^(foo|bar)+?$", input: "foobar", regex: Match, regex_lite: Match },
    Case { pattern: r"^[[:alpha:]]+$", input: "abcXYZ", regex: Match, regex_lite: Match },
    // Agreement: `.` and literal codepoints are Unicode scalar values in both.
    Case { pattern: r"^.$", input: "é", regex: Match, regex_lite: Match },
    Case { pattern: r"^café$", input: "café", regex: Match, regex_lite: Match },
    // Agreement: both reject invalid syntax.
    Case { pattern: r"(unclosed", input: "x", regex: Invalid, regex_lite: Invalid },
    // Difference: Perl classes are Unicode in `regex`, ASCII-only in `regex-lite`.
    Case { pattern: r"^\w+$", input: "café", regex: Match, regex_lite: NoMatch },
    Case { pattern: r"^\d$", input: "\u{0663}", regex: Match, regex_lite: NoMatch },
    Case { pattern: r"^\s$", input: "\u{00A0}", regex: Match, regex_lite: NoMatch },
    // Difference: word boundaries follow the `\w` definition.
    Case { pattern: r"\bé", input: "é", regex: Match, regex_lite: NoMatch },
    // Difference: Unicode property classes do not compile in `regex-lite`.
    Case { pattern: r"^\p{L}+$", input: "héllo", regex: Match, regex_lite: Invalid },
    Case { pattern: r"^\P{N}$", input: "a", regex: Match, regex_lite: Invalid },
    // Difference: case-insensitive matching is ASCII-only in `regex-lite`.
    Case { pattern: r"(?i)^é$", input: "É", regex: Match, regex_lite: NoMatch },
];

fn outcome(pattern: &str, input: &str) -> Outcome {
    let context: Variable = json!({ "p": pattern, "s": input }).into();
    match evaluate_expression("matches(s, p)", context) {
        Ok(Variable::Bool(true)) => Match,
        Ok(Variable::Bool(false)) => NoMatch,
        Ok(other) => panic!("matches({input:?}, {pattern:?}) returned {other:?}"),
        Err(_) => Invalid,
    }
}

const ACTIVE_IS_REGEX: bool = cfg!(all(
    feature = "regex-deprecated",
    not(feature = "regex-lite")
));

/// Runs the table through the expression language with whichever backend this build uses.
#[test]
fn matches_follows_the_active_backend() {
    for case in CASES {
        let expected = if ACTIVE_IS_REGEX {
            case.regex
        } else {
            case.regex_lite
        };
        assert_eq!(
            outcome(case.pattern, case.input),
            expected,
            "pattern {:?} on {:?} (backend: {})",
            case.pattern,
            case.input,
            if ACTIVE_IS_REGEX {
                "regex"
            } else {
                "regex-lite"
            },
        );
    }
}

/// `extract()` captures follow the same class definitions.
#[test]
fn extract_follows_the_active_backend() {
    let context: Variable = json!({ "s": "id: café42", "p": r"(\w+)(\d+)$" }).into();
    let result = evaluate_expression("extract(s, p)", context).unwrap();
    let expected = if ACTIVE_IS_REGEX {
        json!(["café42", "café4", "2"])
    } else {
        json!(["42", "4", "2"])
    };
    assert_eq!(serde_json::Value::from(result), expected);
}

/// With `regex` available, check both crates directly so one run documents both columns.
#[cfg(feature = "regex-deprecated")]
#[test]
fn table_matches_both_crates() {
    fn direct<E>(compiled: Result<impl Fn(&str) -> bool, E>, input: &str) -> Outcome {
        match compiled {
            Ok(is_match) if is_match(input) => Match,
            Ok(_) => NoMatch,
            Err(_) => Invalid,
        }
    }
    for case in CASES {
        let regex = regex::Regex::new(case.pattern).map(|re| move |s: &str| re.is_match(s));
        let lite = regex_lite::Regex::new(case.pattern).map(|re| move |s: &str| re.is_match(s));
        assert_eq!(
            direct(regex, case.input),
            case.regex,
            "regex: {:?}",
            case.pattern
        );
        assert_eq!(
            direct(lite, case.input),
            case.regex_lite,
            "regex-lite: {:?}",
            case.pattern
        );
    }
}
