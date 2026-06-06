#!/usr/bin/env python3
"""emit clarification questions for a recording.

triggers (mirrored from `prompts.rs`):
  1. single observed i128 amount → cap-or-weekly?
  2. delegated signer → reuse or new agent key?
  3. soroswap router → slippage cap default + 200 bps; override?
  4. `Default` context rule → switch to `CallContract(<target>)`?

input: Recording json on stdin. output: array of `{question, default}` on stdout.
exit: 0 ok (possibly empty), 2 not a valid recording.
"""

import json
import sys
from typing import Any, Dict, List, Optional


def _shorten_addr(addr: str) -> str:
    if not isinstance(addr, str) or len(addr) <= 12:
        return addr
    return f"{addr[:6]}…{addr[-4:]}"


def _collect_i128_args(contracts: List[Any]) -> List[str]:
    out: List[str] = []
    for c in contracts:
        if not isinstance(c, dict):
            continue
        for a in c.get("args", []) or []:
            if isinstance(a, dict) and a.get("type") == "i128":
                v = a.get("value")
                if isinstance(v, str):
                    out.append(v)
    return out


def _delegated_signers(auth_tree: Dict[str, Any]) -> List[str]:
    out: List[str] = []
    roots = auth_tree.get("roots", []) if isinstance(auth_tree, dict) else []
    for entry in roots:
        creds = entry.get("credentials", {}) if isinstance(entry, dict) else {}
        if creds.get("kind") == "address":
            signer = creds.get("signer")
            if isinstance(signer, str):
                out.append(signer)
    return out


def _contract_targets(contracts: List[Any]) -> List[str]:
    out: List[str] = []
    for c in contracts:
        if isinstance(c, dict):
            addr = c.get("address")
            if isinstance(addr, str):
                out.append(addr)
    return out


def _has_swap_invocation(contracts: List[Any]) -> Optional[Dict[str, str]]:
    """Return `{address, function}` for the first swap-like invocation."""
    for c in contracts:
        if not isinstance(c, dict):
            continue
        fn = c.get("function")
        if isinstance(fn, str) and "swap" in fn.lower():
            return {
                "address": str(c.get("address", "")),
                "function": fn,
            }
    return None


def propose(rec: Dict[str, Any]) -> List[Dict[str, str]]:
    contracts = rec.get("contracts", []) if isinstance(rec.get("contracts"), list) else []
    auth_tree = rec.get("auth_tree", {}) if isinstance(rec.get("auth_tree"), dict) else {}

    questions: List[Dict[str, str]] = []

    # trigger 1: single observed amount → cap vs total.
    amounts = _collect_i128_args(contracts)
    if len(amounts) == 1:
        questions.append({
            "question": (
                f"The recording contains a single observed amount of {amounts[0]} stroops. "
                "Should the policy cap **each call** at that amount, or accept up to that "
                "amount as a **weekly/monthly total** across many calls?"
            ),
            "default": "weekly_total",
        })

    # trigger 2: delegated signer present.
    delegated = _delegated_signers(auth_tree)
    if delegated:
        first = _shorten_addr(delegated[0])
        questions.append({
            "question": (
                f"The transaction was authorised by a delegated signer ({first}). "
                "Should the policy keep using **this same address** as the agent, or "
                "should we **generate a fresh agent key** so the existing key keeps its "
                "current scope unchanged?"
            ),
            "default": "generate_new_agent_key",
        })

    # trigger 3: Soroswap / swap router invocation.
    swap = _has_swap_invocation(contracts)
    if swap is not None:
        addr = _shorten_addr(swap["address"])
        questions.append({
            "question": (
                f"Detected a swap invocation: `{swap['function']}` on `{addr}`. "
                "Slippage cap defaults to **observed + 200 bps (2%)**. Override?"
            ),
            "default": "observed_plus_200bps",
        })

    # trigger 4: Default context rule (zero or >1 contract targets).
    targets = _contract_targets(contracts)
    distinct = sorted(set(targets))
    if len(distinct) != 1:
        if len(distinct) == 0:
            detail = "no contract targets are present in the recording"
        else:
            detail = f"{len(distinct)} distinct contract targets are present"
        questions.append({
            "question": (
                f"The synthesizer will fall back to a `Default` context rule because {detail}. "
                "`Default` rules authorise any context and are the broadest possible scope. "
                "Pick one specific contract and switch to `CallContract(<target>)` for "
                "least-privilege?"
            ),
            "default": "switch_to_call_contract",
        })

    return questions


def main() -> int:
    try:
        raw = sys.stdin.read()
    except OSError as exc:
        sys.stderr.write(f"propose_clarifications: failed to read stdin: {exc}\n")
        return 2
    try:
        rec = json.loads(raw)
    except json.JSONDecodeError as exc:
        sys.stderr.write(f"propose_clarifications: stdin is not valid JSON: {exc}\n")
        return 2
    if not isinstance(rec, dict) or rec.get("schema") != "oz-policy-builder/recording/v1":
        sys.stderr.write(
            "propose_clarifications: stdin does not look like a Recording document "
            "(missing schema=oz-policy-builder/recording/v1)\n"
        )
        return 2
    out = propose(rec)
    sys.stdout.write(json.dumps(out, indent=2) + "\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())
