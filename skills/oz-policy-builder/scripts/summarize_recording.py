#!/usr/bin/env python3
"""summarize_recording.py — produce a plain-English summary of a Recording JSON.

The skill workflow (see ../SKILL.md, step 3) reads a `Recording` produced by
`record_transaction`, then asks the user to confirm the recorded flow before
synthesis begins. This script does that summarization. It is **pure
formatting**: no policy logic, no clarification triggers (see
`propose_clarifications.py` for those), and no third-party dependencies.

Input  : a `Recording` JSON document on stdin (matches
         `crates/oz-policy-core/src/recording.rs::Recording`, schema URI
         `oz-policy-builder/recording/v1`).
Output : 3–5 sentences on stdout describing what the transaction does in
         plain English.

Exit codes:
  0 — summary written to stdout
  2 — stdin was not valid JSON OR did not look like a Recording document
"""

import json
import sys
from typing import Any, Dict, List, Optional


def _shorten_addr(addr: str) -> str:
    """Truncate StrKey addresses to `<first6>…<last4>` for readability."""
    if not isinstance(addr, str) or len(addr) <= 12:
        return addr
    return f"{addr[:6]}…{addr[-4:]}"


def _format_amount(raw: str) -> str:
    """Format an i128 stroop amount with thousands separators."""
    try:
        n = int(raw)
    except (TypeError, ValueError):
        return str(raw)
    return f"{n:,}"


def _arg_to_str(arg: Any) -> str:
    """Render one decoded ArgValue as a short string for the summary line."""
    if not isinstance(arg, dict):
        return str(arg)
    t = arg.get("type")
    v = arg.get("value")
    if t == "address" and isinstance(v, str):
        return _shorten_addr(v)
    if t == "i128" and isinstance(v, str):
        return _format_amount(v) + " stroops"
    if t == "vec" and isinstance(v, list):
        if len(v) > 4:
            return f"[{len(v)} items]"
        return "[" + ", ".join(_arg_to_str(item) for item in v) + "]"
    if t in {"u32", "u64", "i32", "i64", "bool"}:
        return str(v)
    if t == "symbol" and isinstance(v, str):
        return f"sym:{v}"
    if t in {"string", "bytes"}:
        return f"{t}({v!r})"
    return f"{t}={v!r}"


def _format_contract(c: Dict[str, Any]) -> str:
    addr = c.get("address", "?")
    fn = c.get("function", "?")
    args = c.get("args", [])
    rendered = ", ".join(_arg_to_str(a) for a in args)
    return f"`{fn}({rendered})` on contract `{_shorten_addr(addr)}`"


def _signer_summary(auth_tree: Dict[str, Any]) -> str:
    roots = auth_tree.get("roots", []) if isinstance(auth_tree, dict) else []
    if not roots:
        return "no auth entries observed"
    signers: List[str] = []
    has_source_only = False
    for entry in roots:
        creds = entry.get("credentials", {}) if isinstance(entry, dict) else {}
        kind = creds.get("kind")
        if kind == "source_account":
            has_source_only = True
        elif kind == "address":
            signer = creds.get("signer")
            if isinstance(signer, str):
                signers.append(_shorten_addr(signer))
    if signers:
        uniq = sorted(set(signers))
        joined = ", ".join(uniq)
        return f"signed by {len(uniq)} delegated address(es): {joined}"
    if has_source_only:
        return "signed by the source account itself (no delegated signer)"
    return f"{len(roots)} auth entries"


def _event_summary(events: List[Any]) -> str:
    n = len(events) if isinstance(events, list) else 0
    if n == 0:
        return "no events emitted"
    topic_names: List[str] = []
    for ev in events[:3]:
        topics = ev.get("topics", []) if isinstance(ev, dict) else []
        if topics and isinstance(topics[0], dict) and topics[0].get("type") == "symbol":
            topic_names.append(str(topics[0].get("value")))
    if topic_names:
        return f"{n} event(s) emitted (first topics: {', '.join(topic_names)})"
    return f"{n} event(s) emitted"


def _state_summary(state_changes: List[Any]) -> str:
    n = len(state_changes) if isinstance(state_changes, list) else 0
    if n == 0:
        return "no state changes recorded"
    return f"{n} state entry change(s) observed"


def _network_label(passphrase: Optional[str]) -> str:
    if not isinstance(passphrase, str):
        return "an unknown network"
    if "Test" in passphrase:
        return "Stellar testnet"
    if "Public" in passphrase:
        return "Stellar mainnet"
    return f"the network with passphrase {passphrase!r}"


def summarize(rec: Dict[str, Any]) -> str:
    """Render a 3–5 sentence summary of a Recording document."""
    network = _network_label(rec.get("network_passphrase"))
    ingest = rec.get("ingest", {}) if isinstance(rec.get("ingest"), dict) else {}
    ingest_kind = ingest.get("kind")
    if ingest_kind == "hash":
        ingest_phrase = f"recorded from on-chain hash `{ingest.get('hash', '?')[:16]}…`"
    elif ingest_kind == "simulation":
        ingest_phrase = "recorded via local simulation of a caller-supplied envelope"
    else:
        ingest_phrase = "recorded from an unknown ingest source"

    contracts = rec.get("contracts", []) if isinstance(rec.get("contracts"), list) else []
    if not contracts:
        contract_phrase = "It invokes no contracts."
    elif len(contracts) == 1:
        contract_phrase = f"It invokes {_format_contract(contracts[0])}."
    else:
        lines = "; ".join(_format_contract(c) for c in contracts[:3])
        more = ""
        if len(contracts) > 3:
            more = f" (and {len(contracts) - 3} more)"
        contract_phrase = f"It invokes {len(contracts)} contracts: {lines}{more}."

    signer_phrase = _signer_summary(rec.get("auth_tree", {}))
    state_phrase = _state_summary(rec.get("state_changes", []))
    event_phrase = _event_summary(rec.get("events", []))

    return (
        f"This transaction on {network} was {ingest_phrase}. "
        f"{contract_phrase} "
        f"It is {signer_phrase}. "
        f"During execution {state_phrase} and {event_phrase}."
    )


def main() -> int:
    try:
        raw = sys.stdin.read()
    except OSError as exc:
        sys.stderr.write(f"summarize_recording: failed to read stdin: {exc}\n")
        return 2
    try:
        rec = json.loads(raw)
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"summarize_recording: stdin is not valid JSON: {exc}\n")
        return 2
    if not isinstance(rec, dict) or rec.get("schema") != "oz-policy-builder/recording/v1":
        sys.stderr.write(
            "summarize_recording: stdin does not look like a Recording document "
            "(missing schema=oz-policy-builder/recording/v1)\n"
        )
        return 2
    sys.stdout.write(summarize(rec) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
