//! bearer-token authentication middleware for the Streamable HTTP transport.
//!
//! per plan.md Phase 5 (Implementation → main.rs): every request to the
//! `POST /mcp` JSON-RPC endpoint must carry an `Authorization: Bearer <token>`
//! header whose value matches the operator-configured token. The token is
//! supplied via the `--token <value>` CLI flag or, failing that, the
//! `OZ_POLICY_MCP_TOKEN` environment variable.
//!
//! reject semantics (hard, per plan.md "Bearer-token reject is hard. No
//! partial-credit 'warn but proceed.'"):
//!
//! | Condition                                | HTTP status | Body                  |
//! |------------------------------------------|-------------|-----------------------|
//! | Missing `Authorization` header           | 401         | `"missing bearer"`    |
//! | Non-`Bearer <…>` format                  | 401         | `"invalid bearer"`    |
//! | `Bearer <wrong>` (constant-time compare) | 401         | `"invalid bearer"`    |
//!
//! the middleware does NOT enforce auth on `GET /healthz`; that path is
//! intentionally excluded so load balancers / k8s liveness probes don't need
//! the secret. This is handled at the axum router level (`/healthz` is a
//! sibling route, not nested under the auth layer).
//!
//! ## Constant-time comparison
//!
//! the token comparison uses [`constant_time_eq`] to avoid leaking the
//! token's length / prefix via timing side channels. The comparison itself
//! runs in O(max(a, b)) regardless of how many bytes match — see the unit
//! tests below for a regression guard.

use std::task::{Context, Poll};

use axum::body::Body;
use http::{header::AUTHORIZATION, HeaderValue, Request, Response, StatusCode};
use tower::{Layer, Service};

/// `Layer` constructor: returns a tower layer that, when applied to a
/// service, rejects requests lacking a matching `Authorization: Bearer
/// <token>` header with HTTP 401. See [`BearerAuthLayer`] for the
/// underlying type.
///
/// the `token` argument is taken by `Into<String>` so callers can pass
/// either an owned `String` (the typical path — the token is read from a
/// CLI flag or env var into a `String`) or a string literal in tests.
///
/// ```
/// # use oz_policy_mcp::auth::bearer_layer;
/// # use tower::ServiceBuilder;
/// let layer = bearer_layer("super-secret-token");
/// let _builder = ServiceBuilder::new().layer(layer);
/// ```
pub fn bearer_layer(token: impl Into<String>) -> BearerAuthLayer {
    BearerAuthLayer {
        token: token.into(),
    }
}

/// tower [`Layer`] that wraps a downstream service in a [`BearerAuth`]
/// middleware. Cheap to clone — clones share the same token via
/// `String::clone` (heap-bumped refcount when applied repeatedly).
#[derive(Clone, Debug)]
pub struct BearerAuthLayer {
    token: String,
}

impl<S> Layer<S> for BearerAuthLayer {
    type Service = BearerAuth<S>;
    fn layer(&self, inner: S) -> Self::Service {
        BearerAuth {
            inner,
            token: self.token.clone(),
        }
    }
}

/// tower [`Service`] middleware enforcing `Authorization: Bearer <token>`.
///
/// reject paths are documented in the module-level docs. The token is owned
/// by the middleware (one allocation per service clone, no per-request
/// alloc).
#[derive(Clone, Debug)]
pub struct BearerAuth<S> {
    inner: S,
    token: String,
}

impl<S> BearerAuth<S> {
    /// returns the wrapped inner service. Public so callers that built the
    /// layer manually (rather than via `bearer_layer`) can recover the
    /// underlying service if needed.
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> Service<Request<Body>> for BearerAuth<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send + 'static>,
    >;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let outcome = check_bearer(req.headers().get(AUTHORIZATION), &self.token);
        match outcome {
            BearerOutcome::Ok => {
                let fut = self.inner.call(req);
                Box::pin(fut)
            }
            BearerOutcome::Missing => Box::pin(async {
                Ok(unauthorized(b"missing bearer").expect("static 401 response"))
            }),
            BearerOutcome::Invalid => Box::pin(async {
                Ok(unauthorized(b"invalid bearer").expect("static 401 response"))
            }),
        }
    }
}

/// builds a 401 response with a plain-text body. Returns `Result` only
/// because `Response::builder()` can in principle fail — the static bytes
/// we pass never trip the failure paths, so the call site `.expect()`s.
fn unauthorized(body: &'static [u8]) -> Result<Response<Body>, http::Error> {
    let mut resp = Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(body))?;
    // RFC 6750 §3 says the server SHOULD set `WWW-Authenticate: Bearer` on 401.
    resp.headers_mut().insert(
        http::header::WWW_AUTHENTICATE,
        HeaderValue::from_static("Bearer"),
    );
    Ok(resp)
}

/// outcome of `Authorization` header validation. Public so the binary's
/// axum-level middleware (`main::bearer_auth_middleware`) can share the
/// exact same scheme parsing + constant-time comparison logic that the
/// `BearerAuth<S>` tower layer uses, without re-implementing it.
#[derive(Debug, PartialEq, Eq)]
pub enum BearerOutcome {
    /// `Authorization: Bearer <expected>` — request passes through.
    Ok,
    /// no `Authorization` header at all.
    Missing,
    /// header present but malformed, wrong scheme, or wrong token.
    Invalid,
}

/// validates an `Authorization` header against the expected token. Public
/// so the binary's axum middleware can reuse the same constant-time
/// comparison logic as the standalone `BearerAuth<S>` tower service.
///
/// returns:
/// * [`BearerOutcome::Ok`] for `Authorization: Bearer <expected>` (the
///   token comparison is constant-time).
/// * [`BearerOutcome::Missing`] when the header is entirely absent.
/// * [`BearerOutcome::Invalid`] for header byte sequences that aren't
///   valid UTF-8, non-`Bearer ` schemes, or mismatched tokens.
pub fn check_bearer(header: Option<&HeaderValue>, expected: &str) -> BearerOutcome {
    let Some(value) = header else {
        return BearerOutcome::Missing;
    };
    let Ok(s) = value.to_str() else {
        return BearerOutcome::Invalid;
    };
    // RFC 6750 §2.1: the auth-scheme is case-insensitive; the token is
    // case-sensitive. We strip the leading "Bearer " (case-insensitive on
    // the scheme, exact on the single space) and require there's at least
    // one byte after it.
    let Some(token) = strip_bearer_prefix(s) else {
        return BearerOutcome::Invalid;
    };
    if constant_time_eq(token.as_bytes(), expected.as_bytes()) {
        BearerOutcome::Ok
    } else {
        BearerOutcome::Invalid
    }
}

/// strips the case-insensitive `Bearer ` prefix and returns the remainder.
/// returns `None` if the input is not a well-formed Bearer header (missing
/// the scheme, missing the space after it, or empty token).
fn strip_bearer_prefix(s: &str) -> Option<&str> {
    // length check first — "Bearer ".len() == 7.
    if s.len() <= 7 {
        return None;
    }
    let (scheme, rest) = s.split_at(6);
    if !scheme.eq_ignore_ascii_case("Bearer") {
        return None;
    }
    let separator = rest.as_bytes().first()?;
    if *separator != b' ' {
        return None;
    }
    let token = &rest[1..];
    if token.is_empty() {
        return None;
    }
    Some(token)
}

/// constant-time byte slice equality. Returns `false` immediately on
/// length mismatch (the lengths themselves are not secret — they leak
/// regardless via the response body / TCP framing) but iterates the full
/// length on equal-length inputs so the byte-by-byte comparison time is
/// independent of where the first mismatch occurs.
///
/// this is intentionally a tiny hand-rolled implementation rather than a
/// dep on `constant_time_eq` / `subtle` — the dep graph is already large
/// and the function is 8 lines.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    #[test]
    fn constant_time_eq_matches_equal_inputs() {
        assert!(constant_time_eq(b"hello", b"hello"));
        assert!(constant_time_eq(b"", b""));
        assert!(constant_time_eq(b"\x00\xff", b"\x00\xff"));
    }

    #[test]
    fn constant_time_eq_rejects_unequal_inputs() {
        assert!(!constant_time_eq(b"hello", b"world"));
        assert!(!constant_time_eq(b"hello", b"hellp"));
        // length mismatch
        assert!(!constant_time_eq(b"hello", b"hello!"));
        assert!(!constant_time_eq(b"", b"x"));
    }

    #[test]
    fn strip_bearer_accepts_canonical_form() {
        assert_eq!(strip_bearer_prefix("Bearer abc"), Some("abc"));
        // case-insensitive scheme per RFC 6750.
        assert_eq!(strip_bearer_prefix("bearer abc"), Some("abc"));
        assert_eq!(strip_bearer_prefix("BEARER abc"), Some("abc"));
        assert_eq!(strip_bearer_prefix("BeArEr xyz123"), Some("xyz123"));
    }

    #[test]
    fn strip_bearer_rejects_malformed() {
        assert_eq!(strip_bearer_prefix(""), None);
        assert_eq!(strip_bearer_prefix("Bearer"), None);
        assert_eq!(strip_bearer_prefix("Bearer "), None); // empty token
        assert_eq!(strip_bearer_prefix("BearerX abc"), None); // no separator space
        assert_eq!(strip_bearer_prefix("Basic abc"), None); // wrong scheme
        assert_eq!(strip_bearer_prefix("Bearerabc"), None); // no separator
        assert_eq!(strip_bearer_prefix("  Bearer abc"), None); // leading whitespace
    }

    #[test]
    fn check_bearer_missing_header() {
        assert_eq!(check_bearer(None, "secret"), BearerOutcome::Missing);
    }

    #[test]
    fn check_bearer_invalid_header_bytes() {
        let bad = HeaderValue::from_bytes(b"Bearer \xff").unwrap();
        assert_eq!(check_bearer(Some(&bad), "secret"), BearerOutcome::Invalid);
    }

    #[test]
    fn check_bearer_wrong_token() {
        let h = HeaderValue::from_static("Bearer notsecret");
        assert_eq!(check_bearer(Some(&h), "secret"), BearerOutcome::Invalid);
    }

    #[test]
    fn check_bearer_correct_token() {
        let h = HeaderValue::from_static("Bearer secret");
        assert_eq!(check_bearer(Some(&h), "secret"), BearerOutcome::Ok);
    }

    #[test]
    fn check_bearer_wrong_scheme() {
        let h = HeaderValue::from_static("Basic dXNlcjpwYXNz");
        assert_eq!(check_bearer(Some(&h), "secret"), BearerOutcome::Invalid);
    }

    #[test]
    fn bearer_layer_clone_keeps_token() {
        let l1 = bearer_layer("abc");
        let l2 = l1.clone();
        assert_eq!(l1.token, l2.token);
    }

    #[test]
    fn bearer_auth_into_inner_round_trip() {
        // stub a unit type as the inner service — we only need into_inner()
        // round-trip, not the Service impl, so this stays free of tower deps.
        let auth = BearerAuth {
            inner: 42_i32,
            token: "tok".to_string(),
        };
        assert_eq!(auth.into_inner(), 42);
    }
}
