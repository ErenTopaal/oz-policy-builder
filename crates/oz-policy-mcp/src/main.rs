//! `oz-policy-mcp` — binary entrypoint for the OZ Accounts Policy Builder
//! MCP server (Phase 5 Stream C).
//!
//! supports two transports per `plan.md` Phase 5 (Implementation → main.rs):
//!
//! * **STDIO** (`--stdio`, default) — reads json-rpc frames from `stdin`,
//!   writes to `stdout`. used by mcp clients that subprocess-spawn the server.
//! * **Streamable HTTP** (`--http <port>`) — binds `0.0.0.0:<port>` and
//!   exposes `POST /mcp` (the rmcp `StreamableHttpService` per MCP spec
//!   2025-11-25) plus an unauthenticated `GET /healthz` probe for k8s /
//!   load-balancer liveness checks.
//!
//! HTTP requests to `/mcp` MUST carry `Authorization: Bearer <token>`. The
//! token is read from the `--token` flag or, failing that, the
//! `OZ_POLICY_MCP_TOKEN` env var (the flag wins per CLI convention). HTTP
//! mode refuses to start when neither source supplies a token — silently
//! defaulting to "no auth" would be a deployment footgun.
//!
//! ## Logging contract
//!
//! **All logs go to stderr.** STDIO transport uses `stdout` for JSON-RPC
//! framing; any stray write to stdout corrupts the protocol. The
//! `tracing_subscriber::fmt` initializer below pins the writer to
//! `std::io::stderr` and reads the level from `RUST_LOG` (falling back to
//! `info`). Both transports share this initialization.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{extract::State, http::Request, middleware::Next, response::Response};
use clap::Parser;
use http::{header::AUTHORIZATION, StatusCode};
use oz_policy_mcp::{McpStore, PolicyServer};
use rmcp::{
    transport::streamable_http_server::{
        session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ServiceExt,
};
use tokio_util::sync::CancellationToken;
use tracing::info;

/// maximum time we'll wait for the HTTP server to shut down cleanly after
/// receiving SIGINT / SIGTERM. After this, the runtime drops in-flight
/// connections. Matches the rmcp test-suite's typical bound — long enough
/// for SSE streams to drain a final priming event, short enough that
/// container orchestrators don't escalate to SIGKILL.
const GRACEFUL_SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(10);

/// CLI for the `oz-policy-mcp` binary. See module docs for transport and
/// auth semantics.
#[derive(Parser, Debug)]
#[command(
    name = "oz-policy-mcp",
    version,
    about = "OZ Accounts Policy Builder MCP server (STDIO + Streamable HTTP)",
    long_about = None,
)]
struct Args {
    /// run in STDIO mode (default if no other transport flag is set).
    /// mutually exclusive with `--http`.
    #[arg(long, conflicts_with = "http")]
    stdio: bool,

    /// run as Streamable HTTP server on the given port (e.g. `8080`).
    /// pass `0` to let the OS assign a free port (used by the integration
    /// smoke test — the assigned port is logged to stderr at startup).
    #[arg(long, conflicts_with = "stdio", value_name = "PORT")]
    http: Option<u16>,

    /// bearer token required for HTTP requests. Falls back to the
    /// `OZ_POLICY_MCP_TOKEN` environment variable if unset. Required for
    /// HTTP mode; ignored for STDIO.
    #[arg(
        long,
        value_name = "TOKEN",
        env = "OZ_POLICY_MCP_TOKEN",
        hide_env_values = true
    )]
    token: Option<String>,

    /// data dir for store persistence. If unset, `McpStore` falls back to
    /// `$XDG_DATA_HOME/oz-policy-mcp` (if that directory already exists)
    /// and otherwise to memory-only. The flag wires through by setting
    /// the `OZ_POLICY_MCP_DATA_DIR` env var that `McpStore` reads.
    #[arg(long, value_name = "PATH")]
    data_dir: Option<PathBuf>,
}

fn init_tracing() {
    // RUST_LOG=info,oz_policy_mcp=debug ... is the recommended dev flow.
    // we default to `info` when RUST_LOG is unset so STDIO logs don't drown
    // a debugger in trace-level rmcp internals.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    // writer pinned to stderr per the module-level logging contract — any
    // stray stdout write corrupts the STDIO JSON-RPC framing.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .try_init();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing();
    let args = Args::parse();

    // forward `--data-dir` to the store via env var. This is the single
    // wiring point: `McpStore::resolve_data_dir()` reads
    // `OZ_POLICY_MCP_DATA_DIR` first.
    //
    // safety: `std::env::set_var` is marked unsafe in Rust 2024 because
    // concurrent access to the process env is undefined behaviour. We set
    // this exactly once, before any tokio task spawns that might read env
    // vars in parallel, so the call is sound. Documented hazard.
    if let Some(dir) = args.data_dir.as_ref() {
        unsafe {
            std::env::set_var("OZ_POLICY_MCP_DATA_DIR", dir);
        }
    }

    let store = Arc::new(McpStore::new());

    match args.http {
        Some(port) => run_http_server(port, args.token, store).await,
        None => run_stdio_server(store).await,
    }
}

/// drive the STDIO transport. Blocks until the client disconnects (stdin
/// EOF) or an unrecoverable transport error occurs.
async fn run_stdio_server(store: Arc<McpStore>) -> Result<(), Box<dyn std::error::Error>> {
    info!(
        version = env!("CARGO_PKG_VERSION"),
        transport = "stdio",
        "oz-policy-mcp starting"
    );
    let server = PolicyServer::with_store(store);
    // `rmcp::transport::io::stdio()` returns `(tokio::io::Stdin, tokio::io::Stdout)`
    // which together implement `IntoTransport<RoleServer, …, …>` via the
    // tuple blanket impl on `(AsyncRead, AsyncWrite)`.
    let transport = rmcp::transport::io::stdio();
    let service = server.serve(transport).await.inspect_err(|e| {
        tracing::error!(error = ?e, "STDIO transport startup failed");
    })?;
    // `service.waiting()` resolves when the peer closes — for STDIO that's
    // stdin EOF or a transport error.
    let reason = service.waiting().await;
    info!(
        ?reason,
        "oz-policy-mcp STDIO transport closed; exiting cleanly"
    );
    Ok(())
}

/// drive the Streamable HTTP transport. Binds `0.0.0.0:<port>`, mounts
/// `POST /mcp` behind the bearer-auth middleware, and exposes a sibling
/// `GET /healthz` outside the auth layer.
async fn run_http_server(
    port: u16,
    token_arg: Option<String>,
    store: Arc<McpStore>,
) -> Result<(), Box<dyn std::error::Error>> {
    let token = token_arg.ok_or_else(|| -> Box<dyn std::error::Error> {
        "OZ_POLICY_MCP_TOKEN required for --http mode \
            (pass --token <value> or set the env var)"
            .into()
    })?;
    if token.trim().is_empty() {
        return Err("OZ_POLICY_MCP_TOKEN must be a non-empty string".into());
    }

    // `CancellationToken` is the rmcp-recommended graceful-shutdown signal
    // — we forward the same token into both `StreamableHttpServerConfig`
    // and `axum::serve(...).with_graceful_shutdown(...)` so a SIGINT cancels
    // active SSE streams and stops accepting new connections in lockstep.
    let cancel = CancellationToken::new();

    let cancel_for_service = cancel.child_token();
    let factory_store = Arc::clone(&store);
    let service = StreamableHttpService::new(
        // per-connection service factory. Each new MCP session gets a fresh
        // `PolicyServer` wrapper, but they all share the same `Arc<McpStore>`
        // so artefacts produced by one session's `record_transaction` call
        // are visible to a later session's `resources/read`.
        move || Ok(PolicyServer::with_store(Arc::clone(&factory_store))),
        Arc::new(LocalSessionManager::default()),
        // `StreamableHttpServerConfig` is `#[non_exhaustive]`; we build it
        // via `Default::default()` + the `with_*` helpers. Defaults are
        // restrictive (loopback only); we additionally allow `0.0.0.0` so
        // container deployments behind a reverse proxy work — TLS
        // termination + ingress allow-listing live one layer up
        // (per plan.md Phase 10 deployment posture).
        StreamableHttpServerConfig::default()
            .with_sse_keep_alive(Some(std::time::Duration::from_secs(15)))
            .with_sse_retry(Some(std::time::Duration::from_secs(3)))
            .with_stateful_mode(true)
            .with_json_response(false)
            .with_cancellation_token(cancel_for_service)
            .with_allowed_hosts(["localhost", "127.0.0.1", "::1", "0.0.0.0"]),
    );

    // `/healthz` is intentionally outside the bearer-auth layer so load
    // balancers / k8s liveness probes don't need the secret. The MCP
    // transport is mounted under `/mcp` using `nest_service`, with a
    // tower `ServiceBuilder` stack that applies the bearer-auth
    // middleware before forwarding to the rmcp service. Going through
    // `nest_service` (rather than `nest` on a Router) keeps the rmcp
    // body type (`BoxBody<Bytes, Infallible>`) untouched — wrapping the
    // service in a raw `tower::Layer` that re-types the response body
    // to axum's `Body` would force per-response conversion glue.
    //
    // `axum::middleware::from_fn_with_state` produces a `Layer` that
    // takes `Request<Body>` and returns `Response<Body>`; the rmcp
    // service takes `Request<Body>` (any `http_body::Body` works) and
    // returns `Response<BoxBody<...>>`. axum's `nest_service` accepts
    // arbitrary tower services here as long as they speak the
    // request/response shape it wires through.
    let auth_layer = axum::middleware::from_fn_with_state(Arc::new(token), bearer_auth_middleware);
    let mcp_service = tower::ServiceBuilder::new()
        .layer(auth_layer)
        .service(service);
    let app = axum::Router::new()
        .route("/healthz", axum::routing::get(healthz_handler))
        .nest_service("/mcp", mcp_service);

    let bind_addr: std::net::SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = tokio::net::TcpListener::bind(bind_addr)
        .await
        .map_err(|e| -> Box<dyn std::error::Error> { format!("bind {bind_addr}: {e}").into() })?;
    let actual_addr = listener.local_addr()?;
    info!(
        version = env!("CARGO_PKG_VERSION"),
        transport = "http",
        addr = %actual_addr,
        "oz-policy-mcp listening (POST /mcp with bearer auth; GET /healthz unauth)"
    );
    // the integration smoke test (`tests/http_smoke.rs`) greps stderr for
    // this exact prefix to discover the OS-assigned port when started with
    // `--http 0`. Keep the format stable. We always emit the loopback-form
    // address (`127.0.0.1:<port>` instead of `0.0.0.0:<port>`) so the
    // test's reqwest client connects to a definitely-reachable address
    // even when the bind address is the wildcard — some platforms
    // refuse outgoing `0.0.0.0` connects.
    let display_addr = if actual_addr.ip().is_unspecified() {
        format!("127.0.0.1:{}", actual_addr.port())
    } else {
        actual_addr.to_string()
    };
    eprintln!("oz-policy-mcp http listening on {display_addr}");

    // wire SIGINT / SIGTERM → CancellationToken so axum's graceful
    // shutdown drains in-flight connections before exit.
    let cancel_for_signal = cancel.clone();
    tokio::spawn(async move {
        if let Err(e) = wait_for_shutdown_signal().await {
            tracing::warn!(error = ?e, "shutdown signal handler failed; cancelling anyway");
        }
        info!("shutdown signal received; draining HTTP server");
        cancel_for_signal.cancel();
    });

    let serve_cancel = cancel.clone();
    let serve_result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            serve_cancel.cancelled().await;
            tokio::time::sleep(GRACEFUL_SHUTDOWN_GRACE).await;
        })
        .await;
    if let Err(e) = serve_result {
        tracing::error!(error = ?e, "axum::serve returned error");
        return Err(e.into());
    }
    info!("oz-policy-mcp HTTP transport exited cleanly");
    Ok(())
}

/// liveness/readiness probe handler. Returns 200 with a tiny JSON body
/// (`{"status":"ok","version":"<pkg-version>"}`). Intentionally outside
/// the bearer-auth layer so load balancers don't need the secret.
async fn healthz_handler() -> axum::Json<HealthzBody> {
    axum::Json(HealthzBody {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// axum-compatible bearer-token middleware applied as a `route_layer` over
/// the `/mcp` subroute. Delegates to the same constant-time comparison the
/// standalone `BearerAuth` tower middleware uses (see
/// [`oz_policy_mcp::auth`]), but re-implemented here as a
/// `axum::middleware::from_fn`-friendly function so the rmcp
/// `StreamableHttpService` body type (`BoxBody<Bytes, Infallible>`) doesn't
/// have to be threaded through a tower `Service` body-type translation.
///
/// state: `Arc<String>` (the token), cloned cheaply per request.
async fn bearer_auth_middleware(
    State(token): State<Arc<String>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let header = request.headers().get(AUTHORIZATION);
    match oz_policy_mcp::auth::check_bearer(header, &token) {
        oz_policy_mcp::auth::BearerOutcome::Ok => next.run(request).await,
        oz_policy_mcp::auth::BearerOutcome::Missing => unauthorized_response("missing bearer"),
        oz_policy_mcp::auth::BearerOutcome::Invalid => unauthorized_response("invalid bearer"),
    }
}

/// build a plain-text 401 response. The `WWW-Authenticate: Bearer` header
/// is set per RFC 6750 §3 so spec-compliant clients can prompt for
/// credentials.
fn unauthorized_response(body: &'static str) -> Response {
    let mut resp = axum::response::Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(axum::body::Body::from(body))
        .expect("static 401 response");
    resp.headers_mut().insert(
        http::header::WWW_AUTHENTICATE,
        http::HeaderValue::from_static("Bearer"),
    );
    resp
}

#[derive(serde::Serialize)]
struct HealthzBody {
    status: &'static str,
    version: &'static str,
}

/// awaits SIGINT (Ctrl-C) on all platforms plus SIGTERM on Unix. Either
/// resolves to `Ok(())` on first arrival. On Windows the SIGTERM branch
/// is omitted (the OS doesn't deliver POSIX SIGTERM; container runtimes
/// use Job Object stop signals).
async fn wait_for_shutdown_signal() -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate())?;
        let mut sigint = signal(SignalKind::interrupt())?;
        tokio::select! {
            _ = sigterm.recv() => Ok(()),
            _ = sigint.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// default invocation (no flags) parses to STDIO mode.
    #[test]
    fn args_default_is_stdio() {
        let args = Args::parse_from(["oz-policy-mcp"]);
        assert!(!args.stdio);
        assert_eq!(args.http, None);
        assert_eq!(args.token, None);
        assert_eq!(args.data_dir, None);
    }

    #[test]
    fn args_explicit_stdio_flag() {
        let args = Args::parse_from(["oz-policy-mcp", "--stdio"]);
        assert!(args.stdio);
        assert_eq!(args.http, None);
    }

    #[test]
    fn args_http_port_parses() {
        let args = Args::parse_from(["oz-policy-mcp", "--http", "8080"]);
        assert_eq!(args.http, Some(8080));
        assert!(!args.stdio);
    }

    #[test]
    fn args_http_zero_means_os_assigned() {
        let args = Args::parse_from(["oz-policy-mcp", "--http", "0"]);
        assert_eq!(args.http, Some(0));
    }

    #[test]
    fn args_token_flag_parses() {
        let args = Args::parse_from(["oz-policy-mcp", "--http", "9090", "--token", "abc"]);
        assert_eq!(args.token.as_deref(), Some("abc"));
    }

    #[test]
    fn args_data_dir_parses() {
        let args = Args::parse_from(["oz-policy-mcp", "--data-dir", "/tmp/store"]);
        assert_eq!(
            args.data_dir.as_deref(),
            Some(std::path::Path::new("/tmp/store"))
        );
    }

    /// `--stdio` and `--http` are mutually exclusive — clap rejects both.
    #[test]
    fn args_stdio_and_http_conflict() {
        let result = Args::try_parse_from(["oz-policy-mcp", "--stdio", "--http", "8080"]);
        assert!(result.is_err(), "expected clap to reject both transports");
    }

    /// `run_http_server` rejects an absent token before binding.
    #[tokio::test]
    async fn http_rejects_missing_token() {
        let store = Arc::new(McpStore::new());
        let res = run_http_server(0, None, store).await;
        let err = res.expect_err("must error without a token");
        let msg = err.to_string();
        assert!(
            msg.contains("OZ_POLICY_MCP_TOKEN"),
            "expected token-required error, got: {msg}"
        );
    }

    /// `run_http_server` rejects an empty (whitespace-only) token before binding.
    #[tokio::test]
    async fn http_rejects_empty_token() {
        let store = Arc::new(McpStore::new());
        let res = run_http_server(0, Some("   ".to_string()), store).await;
        let err = res.expect_err("must error on empty token");
        let msg = err.to_string();
        assert!(
            msg.contains("non-empty"),
            "expected empty-token error, got: {msg}"
        );
    }

    /// `/healthz` returns the expected JSON body with the package version.
    #[tokio::test]
    async fn healthz_returns_ok_and_version() {
        let axum::Json(body) = healthz_handler().await;
        assert_eq!(body.status, "ok");
        assert_eq!(body.version, env!("CARGO_PKG_VERSION"));
    }
}
