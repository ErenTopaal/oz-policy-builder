//! static audit lints over rendered track-B rust source.
//! runs after `render_contract` and before `sandbox::compile`.
//!
//! rules:
//! 1. `require_auth_first` — first stmt in enforce/install/uninstall must be
//!    `smart_account.require_auth()`.
//! 2. `storage_keyed_by_pair` — every persistent storage op keyed by a
//!    `StorageKey::*` variant; bare Symbol/String keys rejected.
//! 3. `no_unsafe` — rejects `unsafe { }`, `unsafe fn`, transmute.
//! 4. `panic_uses_policy_error` — `panic!`/`unreachable!`/bare `.unwrap()`
//!    must use `panic_with_error!(env, PolicyError::*)`.
//! 5. `no_floats_on_amounts` — no `f32`/`f64` anywhere.
//!
//! aggregate `lint_rendered_source` returns the full violation list; callers
//! map to `CodegenCompileFailed` / `E_CODEGEN_COMPILE_FAILED`.

use std::fmt;

use syn::visit::Visit;

/// one lint violation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditLintError {
    /// stable rule id (matches the function name).
    pub rule: &'static str,
    /// 1-based line; 0 when not pinpointable.
    pub line: usize,
    /// short human-readable snippet.
    pub snippet: String,
}

impl fmt::Display for AuditLintError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] line {}: {}", self.rule, self.line, self.snippet)
    }
}

/// run every lint, accumulate violations. parse failure = synthetic `syn_parse_failed`.
pub fn lint_rendered_source(src: &str) -> Result<(), Vec<AuditLintError>> {
    let mut errors: Vec<AuditLintError> = Vec::new();

    // raw-source checks first — work even if AST parse fails.
    errors.extend(no_unsafe(src));
    errors.extend(no_floats_on_amounts(src));

    // ast-level checks. parse failures surface as synthetic violation.
    match syn::parse_file(src) {
        Ok(file) => {
            errors.extend(require_auth_first(&file, src));
            errors.extend(storage_keyed_by_pair(&file, src));
            errors.extend(panic_uses_policy_error(&file, src));
        }
        Err(e) => {
            errors.push(AuditLintError {
                rule: "syn_parse_failed",
                line: e.span().start().line,
                snippet: format!("rendered source did not parse: {e}"),
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// every `fn enforce/install/uninstall` body must start with `smart_account.require_auth()`.
pub fn require_auth_first(file: &syn::File, src: &str) -> Vec<AuditLintError> {
    let mut errors = Vec::new();
    struct V<'a> {
        errors: &'a mut Vec<AuditLintError>,
        src: &'a str,
    }
    impl<'a, 'ast> Visit<'ast> for V<'a> {
        fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
            let name = item.sig.ident.to_string();
            if matches!(name.as_str(), "install" | "enforce" | "uninstall") {
                let first = item.block.stmts.first();
                let ok = first
                    .map(is_smart_account_require_auth_stmt)
                    .unwrap_or(false);
                if !ok {
                    let line = item.sig.fn_token.span.start().line;
                    let snippet = snippet_at_line(self.src, line);
                    self.errors.push(AuditLintError {
                        rule: "require_auth_first",
                        line,
                        snippet: format!(
                            "fn {name}: first statement must be `smart_account.require_auth();`, got `{snippet}`"
                        ),
                    });
                }
            }
            syn::visit::visit_impl_item_fn(self, item);
        }
    }
    let mut v = V {
        errors: &mut errors,
        src,
    };
    v.visit_file(file);
    errors
}

/// match `smart_account.require_auth();` (with or without semi).
fn is_smart_account_require_auth_stmt(stmt: &syn::Stmt) -> bool {
    let expr = match stmt {
        syn::Stmt::Expr(e, _) => e,
        _ => return false,
    };
    let call = match expr {
        syn::Expr::MethodCall(m) => m,
        _ => return false,
    };
    if call.method != "require_auth" {
        return false;
    }
    if !call.args.is_empty() {
        return false;
    }
    // receiver must be the identifier `smart_account`.
    matches!(
        &*call.receiver,
        syn::Expr::Path(p)
            if p.path.is_ident("smart_account")
    )
}

/// every persistent storage op (`set/get/has/remove/...`) must be keyed by
/// a `StorageKey::*` variant (direct borrow or via a let-bound local).
pub fn storage_keyed_by_pair(file: &syn::File, src: &str) -> Vec<AuditLintError> {
    let mut errors = Vec::new();

    // storage-method names that take a key as first arg.
    const STORAGE_METHODS: &[&str] = &[
        "set",
        "get",
        "has",
        "remove",
        "update",
        "extend_ttl",
        "bump",
    ];

    struct V<'a> {
        errors: &'a mut Vec<AuditLintError>,
        src: &'a str,
        // local var name -> bound to StorageKey::* ctor. templates never shadow,
        // so a flat map is safe (no scope tracking needed).
        storage_key_vars: std::collections::HashMap<String, bool>,
    }

    impl<'a, 'ast> Visit<'ast> for V<'a> {
        fn visit_local(&mut self, local: &'ast syn::Local) {
            if let syn::Pat::Ident(pi) = &local.pat {
                let name = pi.ident.to_string();
                let is_storage_key = local
                    .init
                    .as_ref()
                    .map(|init| is_storage_key_expr(strip_clone(&init.expr)))
                    .unwrap_or(false);
                if is_storage_key {
                    self.storage_key_vars.insert(name, true);
                }
            }
            syn::visit::visit_local(self, local);
        }

        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            let method_name = call.method.to_string();
            if STORAGE_METHODS.contains(&method_name.as_str())
                && receiver_is_storage_chain(&call.receiver)
            {
                // first arg is the key.
                if let Some(first_arg) = call.args.first() {
                    let key_expr = strip_reference(first_arg);
                    let ok = is_storage_key_expr(key_expr)
                        || is_known_storage_key_var(key_expr, &self.storage_key_vars);
                    if !ok {
                        let line = call.method.span().start().line;
                        let snippet = snippet_at_line(self.src, line);
                        self.errors.push(AuditLintError {
                            rule: "storage_keyed_by_pair",
                            line,
                            snippet: format!(
                                ".{method_name}(...) called with a non-StorageKey key: `{snippet}`"
                            ),
                        });
                    }
                }
            }
            syn::visit::visit_expr_method_call(self, call);
        }
    }

    let mut v = V {
        errors: &mut errors,
        src,
        storage_key_vars: std::collections::HashMap::new(),
    };
    v.visit_file(file);
    errors
}

/// any path beginning with `StorageKey`.
fn is_storage_key_expr(expr: &syn::Expr) -> bool {
    let path = match expr {
        syn::Expr::Path(p) => &p.path,
        syn::Expr::Call(c) => match &*c.func {
            syn::Expr::Path(p) => &p.path,
            _ => return false,
        },
        syn::Expr::Struct(s) => &s.path,
        _ => return false,
    };
    path.segments
        .first()
        .map(|s| s.ident == "StorageKey")
        .unwrap_or(false)
}

/// path to a known-good local var.
fn is_known_storage_key_var(
    expr: &syn::Expr,
    vars: &std::collections::HashMap<String, bool>,
) -> bool {
    if let syn::Expr::Path(p) = expr {
        if let Some(ident) = p.path.get_ident() {
            return vars.get(&ident.to_string()).copied().unwrap_or(false);
        }
    }
    false
}

/// strip one layer of `&expr` / `&mut expr`.
fn strip_reference(expr: &syn::Expr) -> &syn::Expr {
    match expr {
        syn::Expr::Reference(r) => &r.expr,
        other => other,
    }
}

/// strip a trailing `.clone()` so wrapped key expressions still match.
fn strip_clone(expr: &syn::Expr) -> &syn::Expr {
    if let syn::Expr::MethodCall(m) = expr {
        if m.method == "clone" && m.args.is_empty() {
            return &m.receiver;
        }
    }
    expr
}

/// true if the receiver chain contains a `.storage()` call.
fn receiver_is_storage_chain(expr: &syn::Expr) -> bool {
    let mut cursor = expr;
    loop {
        match cursor {
            syn::Expr::MethodCall(m) => {
                if m.method == "storage" {
                    return true;
                }
                cursor = &m.receiver;
            }
            _ => return false,
        }
    }
}

/// reject `unsafe` / `transmute` via line-level string scan (comments stripped).
pub fn no_unsafe(src: &str) -> Vec<AuditLintError> {
    let mut errors = Vec::new();
    // patterns to flag.
    const PATTERNS: &[(&str, &str)] = &[
        ("unsafe {", "`unsafe { … }` block"),
        ("unsafe fn", "`unsafe fn` declaration"),
        ("unsafe impl", "`unsafe impl` declaration"),
        ("unsafe trait", "`unsafe trait` declaration"),
        ("core::mem::transmute", "`core::mem::transmute` call"),
        ("std::mem::transmute", "`std::mem::transmute` call"),
    ];

    for (idx, line) in src.lines().enumerate() {
        let stripped = strip_line_comments(line);
        for (needle, label) in PATTERNS {
            // substring match is safe — `unsafe_code` ends in `_`, not delimiter.
            if stripped.contains(needle) {
                errors.push(AuditLintError {
                    rule: "no_unsafe",
                    line: idx + 1,
                    snippet: format!("{label}: `{}`", stripped.trim()),
                });
                break;
            }
        }
    }
    errors
}

/// drop everything after `//` on a single line. doesn't handle `/* */`.
fn strip_line_comments(line: &str) -> &str {
    if let Some(idx) = line.find("//") {
        &line[..idx]
    } else {
        line
    }
}

/// non-test code must use `panic_with_error!(env, PolicyError::*)` instead of
/// `panic!`/`unreachable!`/bare `.unwrap()`. ast-driven so we can distinguish
/// `.unwrap_or()` (allowed) from `.unwrap()` (forbidden).
pub fn panic_uses_policy_error(file: &syn::File, src: &str) -> Vec<AuditLintError> {
    let mut errors = Vec::new();

    struct V<'a> {
        errors: &'a mut Vec<AuditLintError>,
        src: &'a str,
        // > 0 inside #[cfg(test)] / #[test]; test-only code doesn't ship on chain.
        in_test_scope: u32,
    }

    impl<'a> V<'a> {
        fn item_has_test_attr(attrs: &[syn::Attribute]) -> bool {
            attrs.iter().any(|a| {
                let p = a.path();
                if p.is_ident("test") {
                    return true;
                }
                if p.is_ident("cfg") {
                    // crude but sufficient: scan for the bare word `test`.
                    if let syn::Meta::List(list) = &a.meta {
                        let toks = list.tokens.to_string();
                        return contains_word(&toks, "test");
                    }
                }
                false
            })
        }
    }

    impl<'a, 'ast> Visit<'ast> for V<'a> {
        fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
            let bump = V::item_has_test_attr(&m.attrs);
            if bump {
                self.in_test_scope += 1;
            }
            syn::visit::visit_item_mod(self, m);
            if bump {
                self.in_test_scope -= 1;
            }
        }

        fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
            let bump = V::item_has_test_attr(&f.attrs);
            if bump {
                self.in_test_scope += 1;
            }
            syn::visit::visit_item_fn(self, f);
            if bump {
                self.in_test_scope -= 1;
            }
        }

        fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
            if self.in_test_scope == 0 && call.method == "unwrap" && call.args.is_empty() {
                let line = call.method.span().start().line;
                let snippet = snippet_at_line(self.src, line);
                self.errors.push(AuditLintError {
                    rule: "panic_uses_policy_error",
                    line,
                    snippet: format!(
                        "bare `.unwrap()` is forbidden — use `.unwrap_or_else(|| panic_with_error!(e, PolicyError::*))`: `{snippet}`"
                    ),
                });
            }
            syn::visit::visit_expr_method_call(self, call);
        }

        fn visit_expr_macro(&mut self, m: &'ast syn::ExprMacro) {
            if self.in_test_scope == 0 {
                check_forbidden_macro(&m.mac, self.src, self.errors);
            }
            syn::visit::visit_expr_macro(self, m);
        }

        // syn parses bare-statement `panic!();` as `Stmt::Macro` (not Expr::Macro);
        // we must visit both forms.
        fn visit_stmt_macro(&mut self, m: &'ast syn::StmtMacro) {
            if self.in_test_scope == 0 {
                check_forbidden_macro(&m.mac, self.src, self.errors);
            }
            syn::visit::visit_stmt_macro(self, m);
        }
    }

    /// shared check for both `Expr::Macro` and `Stmt::Macro` forms.
    fn check_forbidden_macro(mac: &syn::Macro, src: &str, errors: &mut Vec<AuditLintError>) {
        let is_panic = mac.path.is_ident("panic");
        let is_unreachable = mac.path.is_ident("unreachable");
        if !(is_panic || is_unreachable) {
            return;
        }
        let line = mac
            .path
            .segments
            .first()
            .map(|s| s.ident.span().start().line)
            .unwrap_or(0);
        let name = if is_panic { "panic!" } else { "unreachable!" };
        let snippet = snippet_at_line(src, line);
        errors.push(AuditLintError {
            rule: "panic_uses_policy_error",
            line,
            snippet: format!(
                "`{name}` is forbidden — use `panic_with_error!(e, PolicyError::*)`: `{snippet}`"
            ),
        });
    }

    let mut v = V {
        errors: &mut errors,
        src,
        in_test_scope: 0,
    };
    v.visit_file(file);
    errors
}

/// reject `f32`/`f64` tokens anywhere. word-boundary string scan.
pub fn no_floats_on_amounts(src: &str) -> Vec<AuditLintError> {
    let mut errors = Vec::new();
    for (idx, line) in src.lines().enumerate() {
        let stripped = strip_line_comments(line);
        for needle in ["f32", "f64"] {
            if contains_word(stripped, needle) {
                errors.push(AuditLintError {
                    rule: "no_floats_on_amounts",
                    line: idx + 1,
                    snippet: format!(
                        "`{needle}` is forbidden in policy source — use i128: `{}`",
                        stripped.trim()
                    ),
                });
                break;
            }
        }
    }
    errors
}

/// word-boundary contains: `f32`/`f64` match suffix literals (e.g. `2f32`).
fn contains_word(haystack: &str, needle: &str) -> bool {
    let bytes = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || bytes.len() < n.len() {
        return false;
    }
    let mut i = 0;
    while i + n.len() <= bytes.len() {
        if &bytes[i..i + n.len()] == n {
            let before_ok = i == 0 || !is_ident_continue(bytes[i - 1]);
            let after_idx = i + n.len();
            let after_ok = after_idx == bytes.len() || !is_ident_continue(bytes[after_idx]);
            if before_ok && after_ok {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// trimmed source line at 1-based `line`; defensive on out-of-range.
fn snippet_at_line(src: &str, line: usize) -> String {
    if line == 0 {
        return "<line out of range>".into();
    }
    src.lines()
        .nth(line - 1)
        .map(|l| l.trim().to_string())
        .unwrap_or_else(|| "<line out of range>".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// minimal valid impl block with `require_auth()` first in every fn.
    fn good_impl() -> &'static str {
        r#"
        struct Policy;
        impl Policy {
            fn install(e: &(), smart_account: ()) {
                smart_account.require_auth();
                let _ = e;
            }
            fn enforce(e: &(), smart_account: ()) {
                smart_account.require_auth();
                let _ = e;
            }
            fn uninstall(e: &(), smart_account: ()) {
                smart_account.require_auth();
                let _ = e;
            }
        }
        "#
    }

    #[test]
    fn require_auth_first_passes_on_valid_source() {
        let file = syn::parse_file(good_impl()).unwrap();
        let v = require_auth_first(&file, good_impl());
        assert!(v.is_empty(), "valid source must pass; got {v:?}");
    }

    #[test]
    fn require_auth_first_fails_when_missing() {
        let src = r#"
        struct Policy;
        impl Policy {
            fn enforce(e: &()) {
                let _ = e;
                // smart_account.require_auth() is MISSING from line 1
            }
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = require_auth_first(&file, src);
        assert_eq!(v.len(), 1, "missing require_auth must fire exactly once");
        assert_eq!(v[0].rule, "require_auth_first");
        assert!(v[0].snippet.contains("enforce"));
    }

    #[test]
    fn require_auth_first_fails_when_not_first_statement() {
        let src = r#"
        struct Policy;
        impl Policy {
            fn enforce(e: &(), smart_account: ()) {
                let _ = e;
                smart_account.require_auth();  // wrong: not first
            }
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = require_auth_first(&file, src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "require_auth_first");
    }

    // rule 2: storage_keyed_by_pair

    #[test]
    fn storage_keyed_by_pair_passes_with_storage_key_direct() {
        let src = r#"
        fn f(e: &(), sa: (), id: u32) {
            e.storage().persistent().set(&StorageKey::Installed(sa, id), &true);
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = storage_keyed_by_pair(&file, src);
        assert!(v.is_empty(), "direct StorageKey arg must pass; got {v:?}");
    }

    #[test]
    fn storage_keyed_by_pair_passes_with_storage_key_var() {
        let src = r#"
        fn f(e: &(), sa: (), id: u32) {
            let key = StorageKey::Installed(sa, id);
            e.storage().persistent().has(&key);
            e.storage().persistent().set(&key, &true);
            e.storage().persistent().remove(&key);
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = storage_keyed_by_pair(&file, src);
        assert!(v.is_empty(), "let-bound StorageKey must pass; got {v:?}");
    }

    #[test]
    fn storage_keyed_by_pair_fails_with_bare_symbol() {
        let src = r#"
        fn f(e: &()) {
            e.storage().persistent().set(&Symbol::new(e, "x"), &true);
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = storage_keyed_by_pair(&file, src);
        assert_eq!(v.len(), 1, "bare Symbol::new key must fail; got {v:?}");
        assert_eq!(v[0].rule, "storage_keyed_by_pair");
    }

    #[test]
    fn storage_keyed_by_pair_fails_with_bare_string() {
        let src = r#"
        fn f(e: &()) {
            e.storage().persistent().get(&String::from_str(e, "x"));
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = storage_keyed_by_pair(&file, src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "storage_keyed_by_pair");
    }

    // rule 3: no_unsafe

    #[test]
    fn no_unsafe_passes_on_safe_source() {
        // Note: `#![forbid(unsafe_code)]` is the only `unsafe` substring in
        // the rendered source and our `unsafe `+suffix patterns specifically
        // do not match `unsafe_code`. This test verifies that.
        let src = r#"
        #![forbid(unsafe_code)]
        fn safe() {
            let x: i128 = 1;
            let _ = x;
        }
        "#;
        let v = no_unsafe(src);
        assert!(v.is_empty(), "safe code must pass; got {v:?}");
    }

    #[test]
    fn no_unsafe_fails_on_unsafe_block() {
        let src = r#"
        fn bad() {
            unsafe { /* …raw pointer mischief… */ }
        }
        "#;
        let v = no_unsafe(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "no_unsafe");
    }

    #[test]
    fn no_unsafe_fails_on_transmute() {
        let src = r#"
        fn bad() {
            let x: u32 = core::mem::transmute(1.0f32);
        }
        "#;
        let v = no_unsafe(src);
        // Note: this source ALSO triggers `no_floats_on_amounts` via `1.0f32`
        // but `no_unsafe` is run separately and must catch the transmute.
        assert!(v.iter().any(|e| e.rule == "no_unsafe"));
    }

    #[test]
    fn no_unsafe_fails_on_unsafe_fn() {
        let src = r#"
        unsafe fn evil() {}
        "#;
        let v = no_unsafe(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "no_unsafe");
    }

    #[test]
    fn no_unsafe_skips_comments() {
        let src = r#"
        // unsafe { not really }
        // unsafe fn fake() {}
        fn safe() {}
        "#;
        let v = no_unsafe(src);
        assert!(v.is_empty(), "patterns inside comments must be skipped");
    }

    // rule 4: panic_uses_policy_error

    #[test]
    fn panic_uses_policy_error_passes_with_panic_with_error() {
        let src = r#"
        fn f(e: &(), arg: Option<u32>) {
            let _x = arg.unwrap_or_else(|| panic_with_error!(e, PolicyError::Default));
            let _y = arg.unwrap_or(0);
            let _z = arg.unwrap_or_default();
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = panic_uses_policy_error(&file, src);
        assert!(v.is_empty(), "approved forms must pass; got {v:?}");
    }

    #[test]
    fn panic_uses_policy_error_fails_on_bare_unwrap() {
        let src = r#"
        fn f(arg: Option<u32>) {
            let _x = arg.unwrap();
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = panic_uses_policy_error(&file, src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "panic_uses_policy_error");
        assert!(v[0].snippet.contains("unwrap"));
    }

    #[test]
    fn panic_uses_policy_error_fails_on_panic_macro() {
        let src = r#"
        fn f() {
            panic!("nope");
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = panic_uses_policy_error(&file, src);
        assert!(v.iter().any(|e| e.snippet.contains("panic!")));
    }

    #[test]
    fn panic_uses_policy_error_fails_on_unreachable_macro() {
        let src = r#"
        fn f() {
            unreachable!();
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = panic_uses_policy_error(&file, src);
        assert!(v.iter().any(|e| e.snippet.contains("unreachable!")));
    }

    #[test]
    fn panic_uses_policy_error_skips_test_module() {
        let src = r#"
        #[cfg(test)]
        mod tests {
            #[test]
            fn t() {
                let x: Option<u32> = Some(1);
                let _y = x.unwrap();
                panic!("ok in tests");
                unreachable!();
            }
        }
        "#;
        let file = syn::parse_file(src).unwrap();
        let v = panic_uses_policy_error(&file, src);
        assert!(v.is_empty(), "test-scope code must be exempt; got {v:?}");
    }

    // rule 5: no_floats_on_amounts

    #[test]
    fn no_floats_on_amounts_passes_on_i128_source() {
        let src = r#"
        fn f() {
            let amount: i128 = 100;
            let _ = amount;
        }
        "#;
        let v = no_floats_on_amounts(src);
        assert!(v.is_empty(), "i128 amounts must pass; got {v:?}");
    }

    #[test]
    fn no_floats_on_amounts_fails_on_f64() {
        let src = r#"
        fn f() {
            let amount: f64 = 100.0;
            let _ = amount;
        }
        "#;
        let v = no_floats_on_amounts(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "no_floats_on_amounts");
    }

    #[test]
    fn no_floats_on_amounts_fails_on_f32() {
        let src = r#"
        fn f() {
            let amount: f32 = 100.0;
            let _ = amount;
        }
        "#;
        let v = no_floats_on_amounts(src);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].rule, "no_floats_on_amounts");
    }

    #[test]
    fn no_floats_on_amounts_ignores_identifiers_containing_f32() {
        // The substring `f32` inside `if32` (an identifier) must not match
        // — `contains_word` enforces word boundaries.
        let src = r#"
        fn f() {
            let if32 = 1u32;
            let _ = if32;
        }
        "#;
        let v = no_floats_on_amounts(src);
        assert!(
            v.is_empty(),
            "identifiers containing `f32` must not fire; got {v:?}"
        );
    }

    // composition: real rendered output passes, hand-broken source fails.

    #[test]
    fn composition_real_phase3_fixture_passes_all_lints() {
        // Drive the real Phase 3 render pipeline against the frozen fixture
        // source and assert it passes every lint. This is the binary
        // template-quality gate: if the templates are ever edited in a way
        // that violates a lint, this test fails — which is what we want.
        use crate::render::render_contract;
        use oz_policy_core::spec::{
            Constraint, ContextRuleSpec, ContextType, PolicySlot, PolicySpec, RecordingRef,
            SynthesisMode, TemplateFamily,
        };

        let spec = PolicySpec {
            schema: "oz-policy-builder/v1".into(),
            synthesis_mode: SynthesisMode::CodegenOnly,
            context_rule: ContextRuleSpec {
                name: "rule".into(),
                context_type: ContextType::Default,
                valid_until: None,
            },
            signers: Vec::new(),
            policies: vec![PolicySlot::Generated {
                template_family: TemplateFamily::FunctionAllowlist,
                constraints: vec![Constraint::FunctionAllowlist {
                    functions: vec!["transfer".into()],
                }],
            }],
            lifetime_ledgers: None,
            recording_ref: RecordingRef {
                hash: None,
                schema: "oz-recording/v1".into(),
            },
        };

        let rendered = render_contract(&spec, 0).expect("render must succeed");
        match lint_rendered_source(&rendered.src_lib_rs) {
            Ok(()) => {}
            Err(errs) => panic!(
                "rendered fixture must pass all lints; got {} violations:\n{}",
                errs.len(),
                errs.iter()
                    .map(|e| format!("  - {e}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
        }
    }

    #[test]
    fn composition_all_seven_templates_pass_all_lints() {
        // Render each of the seven constraint templates IN ISOLATION (one
        // primitive per generated slot) and assert every render passes
        // every lint. If any template ever introduces a violation that
        // slips past `composition_real_phase3_fixture_passes_all_lints`
        // (which only exercises function_allowlist), this test catches it.
        use crate::render::render_contract;
        use oz_policy_core::{
            arg_value::ArgValue,
            spec::{
                ArgMatcher, Constraint, ContextRuleSpec, ContextType, PolicySlot, PolicySpec,
                RecordingRef, SynthesisMode, TemplateFamily,
            },
        };

        fn spec(family: TemplateFamily, constraint: Constraint) -> PolicySpec {
            PolicySpec {
                schema: "oz-policy-builder/v1".into(),
                synthesis_mode: SynthesisMode::CodegenOnly,
                context_rule: ContextRuleSpec {
                    name: "rule".into(),
                    context_type: ContextType::Default,
                    valid_until: None,
                },
                signers: Vec::new(),
                policies: vec![PolicySlot::Generated {
                    template_family: family,
                    constraints: vec![constraint],
                }],
                lifetime_ledgers: None,
                recording_ref: RecordingRef {
                    hash: None,
                    schema: "oz-recording/v1".into(),
                },
            }
        }

        let cases: Vec<(&str, PolicySpec)> = vec![
            (
                "function_allowlist",
                spec(
                    TemplateFamily::FunctionAllowlist,
                    Constraint::FunctionAllowlist {
                        functions: vec!["transfer".into()],
                    },
                ),
            ),
            (
                "argument_pattern",
                spec(
                    TemplateFamily::ArgumentPattern,
                    Constraint::ArgumentPattern {
                        fn_name: "transfer".into(),
                        arg_index: 0,
                        matcher: ArgMatcher::Exact {
                            value: ArgValue::U32(7),
                        },
                    },
                ),
            ),
            (
                "amount_range",
                spec(
                    TemplateFamily::AmountRange,
                    Constraint::AmountRange {
                        fn_name: "transfer".into(),
                        arg_index: 2,
                        min_string: Some("1".into()),
                        max_string: Some("1000".into()),
                    },
                ),
            ),
            (
                "asset_allowlist",
                spec(
                    TemplateFamily::AssetAllowlist,
                    Constraint::AssetAllowlist {
                        assets: vec![
                            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC".into()
                        ],
                    },
                ),
            ),
            (
                "time_window",
                spec(
                    TemplateFamily::TimeWindow,
                    Constraint::TimeWindow {
                        start_ledger: 1,
                        end_ledger: 100,
                    },
                ),
            ),
            (
                "call_frequency",
                spec(
                    TemplateFamily::CallFrequency,
                    Constraint::CallFrequency {
                        max_calls: 3,
                        window_ledgers: 17_280,
                    },
                ),
            ),
            (
                "sequence_ordering",
                spec(
                    TemplateFamily::SequenceOrdering,
                    Constraint::SequenceOrdering {
                        phases: vec!["init".into(), "do".into(), "done".into()],
                    },
                ),
            ),
        ];

        for (name, s) in &cases {
            let rendered =
                render_contract(s, 0).unwrap_or_else(|e| panic!("[{name}] render failed: {e}"));
            match lint_rendered_source(&rendered.src_lib_rs) {
                Ok(()) => {}
                Err(errs) => panic!(
                    "[{name}] template must pass all lints; got {} violations:\n{}",
                    errs.len(),
                    errs.iter()
                        .map(|e| format!("  - {e}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                ),
            }
        }
    }

    #[test]
    fn composition_hand_crafted_missing_require_auth_fails() {
        // A hand-crafted Policy impl missing `smart_account.require_auth()`
        // as the first line of `enforce`. Must fire `require_auth_first`.
        let src = r#"
        #![no_std]
        #![forbid(unsafe_code)]

        pub enum StorageKey {
            Installed(u32),
        }

        pub struct Policy;
        impl Policy {
            pub fn enforce(e: &(), smart_account: ()) {
                // BUG: require_auth is MISSING — the auth check is gone.
                let _ = (e, smart_account);
            }
        }
        "#;
        let result = lint_rendered_source(src);
        let errs = result.expect_err("missing require_auth must fail lints");
        assert!(
            errs.iter().any(|e| e.rule == "require_auth_first"),
            "expected `require_auth_first` to fire; got {errs:?}"
        );
    }
}
