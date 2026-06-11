//! corpus tests for `simulate_custom_source::check_forbidden`.
//!
//! mirrors `docs/superpowers/specs/2026-06-14-playground-design.md` §6.1 +
//! §9 — the frontend's `preflight.ts` consumes a copy of this same
//! corpus to verify the two implementations agree pattern-for-pattern.
//!
//! ┌────────────────────────────────────────────────────────────────────┐
//! │ Reject corpus: at least one source per forbidden pattern, exercising│
//! │ both the minimum match shape AND a benign-looking obfuscation that  │
//! │ the regex must still catch.                                         │
//! │ Accept corpus: five realistic policy sources that must NOT trigger  │
//! │ any pattern (asserts no false positives on legitimate Rust idioms). │
//! └────────────────────────────────────────────────────────────────────┘
//!
//! Pattern labels match `simulate_custom_source::PATTERNS` exactly — if
//! you rename a label there, the assert here is the cross-stack break
//! the spec calls out.

use oz_policy_mcp::tools::simulate_custom_source::check_forbidden;

// the six label strings, exposed so the assertions read like the spec
// table. Keeping these as local consts (NOT cross-crate imports) makes
// the test self-documenting — a reviewer can grep for the labels here
// without bouncing into the tool module.
const LABEL_UNSAFE: &str = r"\bunsafe\s*(\{|\bfn|\bimpl|\btrait)";
const LABEL_EXTERN: &str = r#"\bextern\s+"[A-Za-z]+""#;
const LABEL_PROC_MACRO: &str = r"#\[\s*proc_macro(_derive|_attribute)?\s*[\(\]]";
const LABEL_LINK: &str = r"#\[\s*link(_name)?\b";
const LABEL_INCLUDE: &str = r"\binclude_(bytes|str)!\s*\(";
const LABEL_BUILD_RS: &str = r#""build.rs""#;

// ----- reject corpus -------------------------------------------------

/// each `(pattern_label, source_snippet)` row triggers exactly the named
/// pattern. The `expected_line` is the 1-based line in `source` that
/// must surface in the error payload.
struct RejectCase {
    label: &'static str,
    source: &'static str,
    expected_line: usize,
}

const REJECT_CORPUS: &[RejectCase] = &[
    // --- 1. unsafe ---------------------------------------------------
    RejectCase {
        label: LABEL_UNSAFE,
        source: "fn enforce() {\n    unsafe { core::ptr::null::<u8>(); }\n}\n",
        expected_line: 2,
    },
    RejectCase {
        label: LABEL_UNSAFE,
        // unsafe fn definition (different sub-keyword to exercise the
        // alternation branch).
        source: "pub unsafe fn back_door() {}\n",
        expected_line: 1,
    },
    // --- 2. extern "ABI" ---------------------------------------------
    RejectCase {
        label: LABEL_EXTERN,
        source: "extern \"C\" {\n    fn libc_open(p: *const u8) -> i32;\n}\n",
        expected_line: 1,
    },
    RejectCase {
        label: LABEL_EXTERN,
        // exotic ABI string — still rejected (any letters).
        source: "extern \"Rust\" fn smuggle() {}\n",
        expected_line: 1,
    },
    // --- 3. proc_macro attribute --------------------------------------
    RejectCase {
        label: LABEL_PROC_MACRO,
        source: "#[proc_macro]\npub fn smuggled(_: TokenStream) -> TokenStream { todo!() }\n",
        expected_line: 1,
    },
    RejectCase {
        label: LABEL_PROC_MACRO,
        // proc_macro_derive with whitespace + opening paren.
        source: "#[ proc_macro_derive (Foo) ]\npub fn d(_: TokenStream) -> TokenStream { todo!() }\n",
        expected_line: 1,
    },
    // --- 4. #[link] / #[link_name] -----------------------------------
    RejectCase {
        label: LABEL_LINK,
        source: "#[link(name = \"c\")]\nextern { fn x(); }\n",
        expected_line: 1,
    },
    RejectCase {
        label: LABEL_LINK,
        source: "#[link_name = \"actual_symbol\"]\nfn aliased() {}\n",
        expected_line: 1,
    },
    // --- 5. include_bytes! / include_str! ----------------------------
    RejectCase {
        label: LABEL_INCLUDE,
        source: "static PAYLOAD: &[u8] = include_bytes!(\"/etc/passwd\");\n",
        expected_line: 1,
    },
    RejectCase {
        label: LABEL_INCLUDE,
        source: "const SCRIPT: &str = include_str! ( \"/tmp/x\" );\n",
        expected_line: 1,
    },
    // --- 6. literal "build.rs" ---------------------------------------
    RejectCase {
        label: LABEL_BUILD_RS,
        source: "// see \"build.rs\" for codegen\n",
        expected_line: 1,
    },
    RejectCase {
        label: LABEL_BUILD_RS,
        // even as a const initializer this should fire.
        source: "const BUILD_SCRIPT_NAME: &str = \"build.rs\";\n",
        expected_line: 1,
    },
];

#[test]
fn reject_corpus_each_case_fires_expected_pattern() {
    for (idx, case) in REJECT_CORPUS.iter().enumerate() {
        let err = match check_forbidden(case.source) {
            Err(hit) => hit,
            Ok(()) => panic!(
                "reject case #{idx} (pattern {}) was accepted by check_forbidden — \
                 source:\n{}",
                case.label, case.source
            ),
        };
        assert_eq!(
            err.pattern, case.label,
            "reject case #{idx} expected pattern {} but got {} (line {}: {:?})",
            case.label, err.pattern, err.line_number, err.line_text
        );
        assert_eq!(
            err.line_number, case.expected_line,
            "reject case #{idx} pattern {} fired on line {} but expected {}",
            case.label, err.line_number, case.expected_line
        );
        assert!(
            !err.line_text.is_empty(),
            "reject case #{idx} pattern {} surfaced an empty line_text",
            case.label
        );
    }
}

// ----- accept corpus -------------------------------------------------

/// five realistic Track-B-style policy sources that must NOT trigger any
/// of the forbidden patterns. Each one exercises a different idiom that
/// could naively false-positive against the regex list:
///   1. plain function-allowlist enforce body.
///   2. comment containing the word `unsafe` (not the keyword).
///   3. doc-comment referencing `extern_caller` (substring match guard).
///   4. attribute `#[contractimpl]` (different `#[` content).
///   5. macro use `vec![…]` (not include_bytes!/include_str!).
const ACCEPT_CORPUS: &[&str] = &[
    // 1. plain enforce body using soroban-sdk idioms; no forbidden tokens.
    r#"
use soroban_sdk::{contractimpl, Address, Env, Symbol};

pub struct Policy;

#[contractimpl]
impl Policy {
    pub fn enforce(env: Env, smart_account: Address, fn_name: Symbol) {
        smart_account.require_auth();
        let allowed = [Symbol::new(&env, "transfer")];
        if !allowed.contains(&fn_name) {
            panic!("disallowed fn");
        }
    }
}
"#,
    // 2. comment with the word `unsafe` as English prose — must NOT match.
    r#"
// IMPORTANT: this policy refuses unsafe arguments by deny-listing them
// explicitly below. (The Rust `unsafe` keyword is forbidden; we never
// need it for policy logic.)
pub fn enforce() { /* ... */ }
"#,
    // 3. doc-comment referencing `extern_caller` as a string identifier —
    //    not the `extern "ABI"` syntax. Regex must distinguish.
    r#"
/// Inspect the `extern_caller` argument supplied by the host. The
/// extern caller is a contract id — see § integrating-with-blend.
pub fn enforce() {}
"#,
    // 4. `#[contractimpl]` and other valid attribute idioms — must NOT
    //    trigger the proc_macro or link pattern.
    r#"
#[contractimpl]
#[cfg(target_arch = "wasm32")]
#[derive(Clone)]
pub struct Policy;
"#,
    // 5. `vec![...]` macro use — distinct from include_bytes! / include_str!.
    r#"
pub fn allowed_fns(env: &soroban_sdk::Env) -> soroban_sdk::Vec<soroban_sdk::Symbol> {
    soroban_sdk::vec![env, soroban_sdk::Symbol::new(env, "transfer")]
}
"#,
];

#[test]
fn accept_corpus_each_case_passes() {
    assert_eq!(
        ACCEPT_CORPUS.len(),
        5,
        "spec §9 calls for 5 valid policy sources in the accept corpus"
    );
    for (idx, src) in ACCEPT_CORPUS.iter().enumerate() {
        match check_forbidden(src) {
            Ok(()) => {}
            Err(hit) => panic!(
                "accept case #{idx} false-positive: pattern {} fired on line {} ({:?})\n\
                 source was:\n{}",
                hit.pattern, hit.line_number, hit.line_text, src
            ),
        }
    }
}

// ----- structural assertions -----------------------------------------

/// every label referenced in the reject corpus must be one of the six
/// canonical labels. Guards against drift — if a contributor adds a row
/// with a typo'd label, the test won't silently accept the result.
#[test]
fn reject_corpus_labels_are_all_canonical() {
    let canonical: &[&str] = &[
        LABEL_UNSAFE,
        LABEL_EXTERN,
        LABEL_PROC_MACRO,
        LABEL_LINK,
        LABEL_INCLUDE,
        LABEL_BUILD_RS,
    ];
    for case in REJECT_CORPUS {
        assert!(
            canonical.contains(&case.label),
            "reject case uses non-canonical label {:?}",
            case.label
        );
    }
}

/// every canonical pattern is exercised by at least one reject case.
/// Catches accidental coverage holes.
#[test]
fn every_canonical_pattern_has_a_reject_case() {
    let canonical: &[&str] = &[
        LABEL_UNSAFE,
        LABEL_EXTERN,
        LABEL_PROC_MACRO,
        LABEL_LINK,
        LABEL_INCLUDE,
        LABEL_BUILD_RS,
    ];
    for label in canonical {
        assert!(
            REJECT_CORPUS.iter().any(|c| c.label == *label),
            "no reject case covers canonical pattern {:?}",
            label
        );
    }
}
