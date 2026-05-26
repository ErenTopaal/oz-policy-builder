//! MCP `prompts/list` + `prompts/get` surface.
//!
//! Phase 5 Stream B owns the three wizard prompt templates that the
//! Anthropic agent skill (Phase 6) and other MCP clients pull from the
//! server. Each template hand-walks a user through one of the canonical
//! flows in `plan.md` § Phase 5 Implementation → Prompts:
//!
//! * `record_and_explain` — hash/envelope ingest + plain-English summary.
//! * `synthesize_subscription` — SEP-41 subscription wizard (walkthrough 2).
//! * `synthesize_delegated_trading` — Soroswap delegated trading wizard
//!   (walkthrough 3).
//!
//! All three return a multi-message conversation as a
//! [`rmcp::model::GetPromptResult`] when rendered. The messages embed the
//! user-supplied argument values via simple `{placeholder}` substitution
//! (no Handlebars / Tera dependency — the templates are short, fixed, and
//! reviewed in this file).

use std::collections::BTreeMap;

use rmcp::{
    model::{GetPromptResult, Prompt, PromptArgument, PromptMessage, PromptMessageRole},
    ErrorData,
};

// ----------------------------------------------------------------------
// Prompt name constants — single source of truth so the registry and
// individual renderers can't drift.
// ----------------------------------------------------------------------

/// Walks the user from "I have a transaction hash / envelope" to a
/// reviewed `Recording` JSON.
pub const PROMPT_RECORD_AND_EXPLAIN: &str = "record_and_explain";

/// Walks the user through synthesising a policy for a recurring SEP-41
/// transfer (subscription / streaming pay flow). Bound to
/// `walkthroughs/02-sep41-subscription/`.
pub const PROMPT_SYNTHESIZE_SUBSCRIPTION: &str = "synthesize_subscription";

/// Walks the user through synthesising a policy for delegated trading on
/// Soroswap with a fenced daily budget. Bound to
/// `walkthroughs/03-soroswap-bounded/`.
pub const PROMPT_SYNTHESIZE_DELEGATED_TRADING: &str = "synthesize_delegated_trading";

/// Prompt registry — exposes `list` and `get` over the three templates
/// above. Constructed with `Prompts::new()`; cheap to clone and `Send +
/// Sync` so Stream C can stash a copy in the server state.
#[derive(Debug, Clone, Default)]
pub struct Prompts {
    _private: (),
}

impl Prompts {
    /// Returns an empty registry — the three templates are baked into the
    /// implementation so no further configuration is needed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Implements `prompts/list`. Returns the static set of three
    /// templates (the MCP spec doesn't paginate prompts).
    pub fn list_prompts(&self) -> Vec<Prompt> {
        vec![
            record_and_explain_descriptor(),
            synthesize_subscription_descriptor(),
            synthesize_delegated_trading_descriptor(),
        ]
    }

    /// Implements `prompts/get`. Dispatches by name and renders the
    /// requested template with the supplied arguments.
    ///
    /// Returns
    /// * `Err(invalid_params)` if the prompt name is unknown.
    /// * `Err(invalid_params)` if a required argument is missing.
    /// * `Ok(GetPromptResult)` otherwise — the multi-message conversation
    ///   the agent should send to the model.
    pub fn get_prompt(
        &self,
        name: &str,
        arguments: Option<BTreeMap<String, String>>,
    ) -> Result<GetPromptResult, ErrorData> {
        let args = arguments.unwrap_or_default();
        match name {
            PROMPT_RECORD_AND_EXPLAIN => render_record_and_explain(&args),
            PROMPT_SYNTHESIZE_SUBSCRIPTION => render_synthesize_subscription(&args),
            PROMPT_SYNTHESIZE_DELEGATED_TRADING => render_synthesize_delegated_trading(&args),
            other => Err(ErrorData::invalid_params(
                format!("unknown prompt: {other}"),
                Some(serde_json::json!({ "name": other })),
            )),
        }
    }
}

// ----------------------------------------------------------------------
// Prompt descriptors (Prompt = name + description + argument schema)
// ----------------------------------------------------------------------

fn record_and_explain_descriptor() -> Prompt {
    Prompt::new(
        PROMPT_RECORD_AND_EXPLAIN,
        Some(
            "Wizard: ingest a Stellar transaction (by hash or envelope), record it, \
             and produce a plain-English summary the user can sign off on before \
             synthesis begins.",
        ),
        Some(vec![
            PromptArgument::new("mode")
                .with_description(
                    "One of \"hash\" or \"envelope\". Determines which `record_transaction` input the \
                     agent should fill in.",
                )
                .with_required(true),
            PromptArgument::new("value")
                .with_description(
                    "The transaction hash (for mode=hash) or base64 envelope XDR (for mode=envelope).",
                )
                .with_required(true),
            PromptArgument::new("network")
                .with_description("\"testnet\" or \"mainnet\" — selects the recorder RPC endpoint.")
                .with_required(true),
        ]),
    )
}

fn synthesize_subscription_descriptor() -> Prompt {
    Prompt::new(
        PROMPT_SYNTHESIZE_SUBSCRIPTION,
        Some(
            "Wizard: synthesise a policy that authorises a recurring SEP-41 token transfer \
             (subscription / streaming pay). Bound to walkthrough 02-sep41-subscription.",
        ),
        Some(vec![
            PromptArgument::new("token")
                .with_description("StrKey `C…` address of the SEP-41 token contract.")
                .with_required(true),
            PromptArgument::new("recipient")
                .with_description("StrKey `G…` or `C…` address the recurring transfer pays out to.")
                .with_required(true),
            PromptArgument::new("amount_per_period_stroops")
                .with_description(
                    "Maximum per-period amount, denominated in stroops (i128 string).",
                )
                .with_required(true),
            PromptArgument::new("period_ledgers")
                .with_description(
                    "Period length in Soroban ledgers (≈ 5 s/ledger). \
                     e.g. 1209600 ≈ 7 days, 5184000 ≈ 30 days.",
                )
                .with_required(true),
            PromptArgument::new("delegated_signer")
                .with_description(
                    "Optional StrKey address of the agent the subscription is delegated to. \
                     If absent, the wizard asks the user whether to generate a new agent key.",
                )
                .with_required(false),
        ]),
    )
}

fn synthesize_delegated_trading_descriptor() -> Prompt {
    Prompt::new(
        PROMPT_SYNTHESIZE_DELEGATED_TRADING,
        Some(
            "Wizard: synthesise a policy that authorises bounded delegated trading via Soroswap. \
             Bound to walkthrough 03-soroswap-bounded (not yet captured at the time of writing — \
             references the in-progress fixture).",
        ),
        Some(vec![
            PromptArgument::new("router")
                .with_description("StrKey `C…` address of the Soroswap router contract.")
                .with_required(true),
            PromptArgument::new("agent_signer")
                .with_description("StrKey address of the trading-bot signer that holds the delegated authority.")
                .with_required(true),
            PromptArgument::new("daily_budget_stroops")
                .with_description("Maximum daily spend across all swap legs, in stroops (i128 string).")
                .with_required(true),
            PromptArgument::new("allowed_assets")
                .with_description(
                    "Comma-separated StrKey `C…` addresses of tokens the agent is allowed to swap. \
                     The wizard turns this into an `AssetAllowlist` constraint.",
                )
                .with_required(true),
            PromptArgument::new("slippage_bps")
                .with_description(
                    "Maximum slippage in basis points (0..=10000). Defaults to observed + 200 bps \
                     if omitted.",
                )
                .with_required(false),
        ]),
    )
}

// ----------------------------------------------------------------------
// Prompt renderers — turn (descriptor, args) into the 4-message
// conversation the agent ultimately sends to the model.
// ----------------------------------------------------------------------

/// Renders `record_and_explain` into the canonical 4-message conversation:
///
/// 1. **assistant** — frames the task for the model (sets the role).
/// 2. **user**      — the user's stated intent + the literal hash/envelope.
/// 3. **assistant** — tool-call plan (call `record_transaction`).
/// 4. **assistant** — summary template the model fills in once the tool
///    returns, ready for the user to sign off on.
fn render_record_and_explain(
    args: &BTreeMap<String, String>,
) -> Result<GetPromptResult, ErrorData> {
    let mode = require_arg(args, "mode", PROMPT_RECORD_AND_EXPLAIN)?;
    let value = require_arg(args, "value", PROMPT_RECORD_AND_EXPLAIN)?;
    let network = require_arg(args, "network", PROMPT_RECORD_AND_EXPLAIN)?;

    if mode != "hash" && mode != "envelope" {
        return Err(ErrorData::invalid_params(
            format!("`mode` must be \"hash\" or \"envelope\", got: {mode}"),
            Some(serde_json::json!({ "argument": "mode", "got": mode })),
        ));
    }

    let messages = vec![
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            "You are the OZ Accounts Policy Builder skill. Your job is to record a Stellar \
             transaction the user wants to authorise and produce a plain-English summary they \
             can confirm before synthesis begins. Never auto-deploy; always end with a request \
             for explicit user confirmation.",
        ),
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "I want to record a transaction on {network} so I can build a policy that permits \
                 exactly this kind of flow.\n\nIngest mode: {mode}\nValue: {value}"
            ),
        ),
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            format!(
                "Plan:\n\
                 1. Call the `record_transaction` tool with `{{ \"{mode}\": \"{value}\", \
                 \"network\": \"{network}\" }}`.\n\
                 2. Cross-check the returned `Recording` for: target contract address(es), \
                    function name(s), authorised signers, observed amounts, and any sub-invocations.\n\
                 3. Summarise in plain English (see next message)."
            ),
        ),
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            "Plain-English summary template (fill in once the tool returns):\n\n\
             > This transaction lets the smart account call **<function>** on the contract \
             **<target_address>**. The observed arguments were: **<args>**. The signer(s) \
             authorising it: **<signers>**. State changes: **<delta_summary>**. \
             Events emitted: **<event_summary>**.\n\n\
             Ask the user: \"Does this match what you intended to authorise? If so, I'll move on \
             to synthesise the matching policy.\"",
        ),
    ];

    Ok(GetPromptResult::new(messages).with_description(
        "Four-message wizard for ingesting a Stellar transaction and summarising the recording.",
    ))
}

/// Renders `synthesize_subscription` into a 4-message conversation that
/// gathers the subscription parameters and proposes a Track-A
/// `spending_limit` composition under a `CallContract(<token>)` context
/// rule (per `docs/oz-internal-shapes.md` §4 — `spending_limit` is only
/// safe when scoped to the token target).
fn render_synthesize_subscription(
    args: &BTreeMap<String, String>,
) -> Result<GetPromptResult, ErrorData> {
    let token = require_arg(args, "token", PROMPT_SYNTHESIZE_SUBSCRIPTION)?;
    let recipient = require_arg(args, "recipient", PROMPT_SYNTHESIZE_SUBSCRIPTION)?;
    let amount = require_arg(
        args,
        "amount_per_period_stroops",
        PROMPT_SYNTHESIZE_SUBSCRIPTION,
    )?;
    let period = require_arg(args, "period_ledgers", PROMPT_SYNTHESIZE_SUBSCRIPTION)?;
    let delegated = args.get("delegated_signer").cloned();

    let signer_block = match delegated {
        Some(ref addr) if !addr.is_empty() => format!(
            "Delegated signer: **{addr}** (the existing agent key the user has chosen)."
        ),
        _ => "Delegated signer: **(none specified — wizard will ask whether to generate a fresh agent key, recommended for least-privilege)**.".to_string(),
    };

    let messages = vec![
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            "You are the OZ Accounts Policy Builder skill in subscription-wizard mode. The \
             user wants to authorise a recurring SEP-41 transfer. Compose `spending_limit` \
             under a `CallContract(<token>)` context rule — never `Default`, per \
             `docs/oz-internal-shapes.md` §4.",
        ),
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "I want to authorise a recurring SEP-41 transfer.\n\n\
                 Token contract: {token}\n\
                 Recipient: {recipient}\n\
                 Cap per period: {amount} stroops\n\
                 Period: {period} ledgers"
            ),
        ),
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            format!(
                "Plan:\n\
                 1. Call `synthesize_policy` with `mode: compose_only`, `tightness: small_margin`, \
                    target token `{token}`, recipient `{recipient}`, period `{period}` ledgers, \
                    limit `{amount}` stroops.\n\
                 2. The expected output is a `PolicySpec` with one `Existing(SpendingLimit)` slot \
                    under context rule `CallContract({{ address: \"{token}\" }})`.\n\
                 3. Call `simulate_policy` with the resulting `spec_id` and surface the permit + \
                    deny results before exporting.\n\n\
                 {signer_block}"
            ),
        ),
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            "Clarifications to ask the user before proceeding (per skill triggers in \
             `propose_clarifications.py`):\n\
             - \"Should the cap roll over unused balance period-to-period, or reset hard?\"\n\
             - \"Is the recipient address final, or should the policy accept any recipient as long \
                as the cap holds?\" (If the latter, drop recipient match.)\n\
             - \"Cycle expiry: should the context rule auto-expire after N ledgers, or never?\"",
        ),
    ];

    Ok(GetPromptResult::new(messages)
        .with_description("Four-message wizard for synthesising a SEP-41 subscription policy."))
}

/// Renders `synthesize_delegated_trading` into a 4-message conversation
/// gathering the trading-bot bounds and proposing a Track-B
/// `function_allowlist` + `amount_range` slot under a
/// `CallContract(<router>)` rule.
fn render_synthesize_delegated_trading(
    args: &BTreeMap<String, String>,
) -> Result<GetPromptResult, ErrorData> {
    let router = require_arg(args, "router", PROMPT_SYNTHESIZE_DELEGATED_TRADING)?;
    let signer = require_arg(args, "agent_signer", PROMPT_SYNTHESIZE_DELEGATED_TRADING)?;
    let budget = require_arg(
        args,
        "daily_budget_stroops",
        PROMPT_SYNTHESIZE_DELEGATED_TRADING,
    )?;
    let assets = require_arg(args, "allowed_assets", PROMPT_SYNTHESIZE_DELEGATED_TRADING)?;
    let slippage = args
        .get("slippage_bps")
        .cloned()
        .unwrap_or_else(|| "(default: observed + 200 bps)".to_string());

    let messages = vec![
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            "You are the OZ Accounts Policy Builder skill in delegated-trading-wizard mode. \
             The user wants to give a trading bot a fenced daily budget on Soroswap. Plan to \
             generate a Track-B policy that combines `function_allowlist` (swap fns only), \
             `asset_allowlist` (user-approved tokens), `amount_range` (slippage cap), and \
             `call_frequency` (daily-budget enforcement window).",
        ),
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                "I want to delegate Soroswap trading to my agent.\n\n\
                 Router: {router}\n\
                 Agent signer: {signer}\n\
                 Daily budget: {budget} stroops\n\
                 Allowed assets: {assets}\n\
                 Slippage cap: {slippage}"
            ),
        ),
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            format!(
                "Plan:\n\
                 1. Call `synthesize_policy` with `mode: codegen_only`, `tightness: small_margin`, \
                    target router `{router}`, agent signer `{signer}`.\n\
                 2. The Track-B slot will encode: function_allowlist (Soroswap swap fns), \
                    asset_allowlist ({assets}), amount_range (per-leg cap derived from {budget} \
                    + slippage {slippage}), and call_frequency (window = 17280 ledgers ≈ 1 day, \
                    max_calls = derived from leg cap and budget).\n\
                 3. Call `simulate_policy` with the resulting `spec_id` and a deny-vector probe \
                    that tries (a) a non-allowlisted asset, (b) over-budget, (c) excessive slippage.\n\
                 4. `export_policy` returns the source, WASM, and install envelope.\n\n\
                 The wizard never auto-deploys — final step is always wallet-signed."
            ),
        ),
        PromptMessage::new_text(
            PromptMessageRole::Assistant,
            "Clarifications to ask the user before proceeding:\n\
             - \"Are these exactly the tokens you want the agent allowed to swap, or should \
                I narrow further (e.g. only stables)?\"\n\
             - \"Should the daily budget reset at midnight UTC, or 24h rolling from first use?\"\n\
             - \"Slippage cap defaults to observed + 2%. Override?\"\n\
             - \"If the agent is compromised, how should the user revoke? \
                (Recommended: short `valid_until` on the context rule.)\"",
        ),
    ];

    Ok(GetPromptResult::new(messages).with_description(
        "Four-message wizard for synthesising a Soroswap delegated-trading policy.",
    ))
}

// ----------------------------------------------------------------------
// Argument helpers
// ----------------------------------------------------------------------

/// Looks up a required argument or returns an `invalid_params` error
/// carrying both the argument name and the parent prompt. This is the
/// pattern Stream A's error_mapping module standardises later — we mirror
/// it inline so the prompts module compiles and tests run without that
/// dependency.
fn require_arg(
    args: &BTreeMap<String, String>,
    name: &'static str,
    prompt: &'static str,
) -> Result<String, ErrorData> {
    match args.get(name) {
        Some(v) if !v.is_empty() => Ok(v.clone()),
        Some(_) => Err(ErrorData::invalid_params(
            format!("prompt `{prompt}` requires non-empty argument `{name}`"),
            Some(serde_json::json!({ "prompt": prompt, "argument": name })),
        )),
        None => Err(ErrorData::invalid_params(
            format!("prompt `{prompt}` missing required argument `{name}`"),
            Some(serde_json::json!({ "prompt": prompt, "argument": name })),
        )),
    }
}

// ----------------------------------------------------------------------
// Tests
// ----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::{ErrorCode, PromptMessageContent};

    fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    /// `list_prompts` returns all three templates in the documented order.
    #[test]
    fn list_returns_all_three_prompts() {
        let p = Prompts::new();
        let list = p.list_prompts();
        assert_eq!(list.len(), 3);
        let names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&PROMPT_RECORD_AND_EXPLAIN));
        assert!(names.contains(&PROMPT_SYNTHESIZE_SUBSCRIPTION));
        assert!(names.contains(&PROMPT_SYNTHESIZE_DELEGATED_TRADING));
    }

    /// Every listed prompt declares the same arguments its renderer
    /// requires. Regression guard: a renderer added without updating the
    /// descriptor (or vice versa) would fail this round-trip.
    #[test]
    fn descriptors_match_renderers() {
        let p = Prompts::new();
        for prompt in p.list_prompts() {
            let argspec = prompt.arguments.clone().unwrap_or_default();
            // Construct a fully-populated argument map from the schema, then
            // assert the renderer accepts it.
            let map: BTreeMap<String, String> = argspec
                .iter()
                .map(|a| {
                    // Plausible values per argument name.
                    let v = match a.name.as_str() {
                        "mode" => "hash",
                        "value" => "abc123",
                        "network" => "testnet",
                        // Stellar StrKey samples are all 56 chars C…/G…; we
                        // don't validate strkey shape here so any non-empty
                        // string works.
                        _ => "placeholder",
                    };
                    (a.name.clone(), v.to_string())
                })
                .collect();
            let result = p.get_prompt(&prompt.name, Some(map));
            assert!(
                result.is_ok(),
                "renderer for {} rejected schema-derived args: {:?}",
                prompt.name,
                result.err()
            );
        }
    }

    /// `record_and_explain` renders exactly four messages in
    /// (assistant, user, assistant, assistant) order with the user-supplied
    /// values substituted in.
    #[test]
    fn record_and_explain_renders_four_messages() {
        let p = Prompts::new();
        let result = p
            .get_prompt(
                PROMPT_RECORD_AND_EXPLAIN,
                Some(args(&[
                    ("mode", "hash"),
                    ("value", "deadbeefcafe"),
                    ("network", "testnet"),
                ])),
            )
            .expect("render must succeed");
        assert_eq!(result.messages.len(), 4);
        let roles: Vec<_> = result.messages.iter().map(|m| m.role.clone()).collect();
        assert_eq!(
            roles,
            vec![
                PromptMessageRole::Assistant,
                PromptMessageRole::User,
                PromptMessageRole::Assistant,
                PromptMessageRole::Assistant,
            ]
        );
        // The user-message body must literally contain the supplied hash.
        match &result.messages[1].content {
            PromptMessageContent::Text { text } => {
                assert!(text.contains("deadbeefcafe"), "got: {text}");
                assert!(text.contains("testnet"), "got: {text}");
            }
            other => panic!("expected text content, got {other:?}"),
        }
    }

    /// Missing required argument → `invalid_params` (-32602), not panic.
    #[test]
    fn record_and_explain_missing_arg_is_invalid_params() {
        let p = Prompts::new();
        // omit "value"
        let err = p
            .get_prompt(
                PROMPT_RECORD_AND_EXPLAIN,
                Some(args(&[("mode", "hash"), ("network", "testnet")])),
            )
            .expect_err("must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(err.message.contains("value"), "msg: {}", err.message);
    }

    /// Bad enum value for `mode` is also invalid_params (not silently
    /// treated as one of the valid modes).
    #[test]
    fn record_and_explain_rejects_bad_mode() {
        let p = Prompts::new();
        let err = p
            .get_prompt(
                PROMPT_RECORD_AND_EXPLAIN,
                Some(args(&[
                    ("mode", "telepathy"),
                    ("value", "abc"),
                    ("network", "testnet"),
                ])),
            )
            .expect_err("must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// `synthesize_subscription` renders a 4-message conversation that
    /// includes the supplied token + recipient addresses verbatim.
    #[test]
    fn synthesize_subscription_renders_with_args() {
        let p = Prompts::new();
        let result = p
            .get_prompt(
                PROMPT_SYNTHESIZE_SUBSCRIPTION,
                Some(args(&[
                    (
                        "token",
                        "CCTOKENADDRESSCCTOKENADDRESSCCTOKENADDRESSCCTOKENADDRESS",
                    ),
                    (
                        "recipient",
                        "GRECIPIENTADDRESSGRECIPIENTADDRESSGRECIPIENTADDRESSGRECI",
                    ),
                    ("amount_per_period_stroops", "100000000"),
                    ("period_ledgers", "1209600"),
                ])),
            )
            .expect("render must succeed");
        assert_eq!(result.messages.len(), 4);
        // Token address surfaces in the user-message body.
        match &result.messages[1].content {
            PromptMessageContent::Text { text } => {
                assert!(text.contains("CCTOKENADDRESS"));
                assert!(text.contains("100000000"));
            }
            _ => panic!("expected text"),
        }
    }

    /// `synthesize_delegated_trading` defaults the slippage argument when
    /// it isn't supplied.
    #[test]
    fn synthesize_delegated_trading_defaults_slippage() {
        let p = Prompts::new();
        let result = p
            .get_prompt(
                PROMPT_SYNTHESIZE_DELEGATED_TRADING,
                Some(args(&[
                    ("router", "CROUTERADDRESS"),
                    ("agent_signer", "GAGENTSIGNER"),
                    ("daily_budget_stroops", "5000000000"),
                    ("allowed_assets", "CASSET1,CASSET2"),
                ])),
            )
            .expect("render must succeed");
        let joined = result
            .messages
            .iter()
            .map(|m| match &m.content {
                PromptMessageContent::Text { text } => text.clone(),
                _ => String::new(),
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(joined.contains("default: observed + 200 bps"));
    }

    /// Unknown prompt name → invalid_params (not method_not_found —
    /// `prompts/get` is itself a valid method, only the *prompt* is unknown).
    #[test]
    fn get_unknown_prompt_returns_invalid_params() {
        let p = Prompts::new();
        let err = p.get_prompt("not-a-prompt", None).expect_err("must error");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }
}
