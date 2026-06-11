// Tests for SpecTab. We build real PolicySpec + Recording fixtures from
// the actual type definitions (no partials, no mock components) so the
// tests reflect reality. No mocks per feedback-no-mock-fallback.

import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { SpecTab } from "../SpecTab";
import type { PolicySpec, Recording } from "../../../lib/types";

function emptyRecording(): Recording {
  return {
    schema: "oz_policy_core.recording.v1",
    network_passphrase: "Test SDF Network ; September 2015",
    ingest: { kind: "hash", hash: "0".repeat(64) },
    ledger: 1234567,
    contracts: [],
    auth_tree: { roots: [] },
    state_changes: [],
    events: [],
  };
}

function emptySpec(overrides: Partial<PolicySpec> = {}): PolicySpec {
  return {
    schema: "oz_policy_core.spec.v1",
    synthesis_mode: "auto",
    context_rule: {
      name: "demo_rule",
      context_type: { kind: "default" },
      valid_until: null,
    },
    signers: [],
    policies: [],
    lifetime_ledgers: 432000,
    recording_ref: { hash: null, schema: "oz_policy_core.recording.v1" },
    ...overrides,
  };
}

describe("SpecTab", () => {
  it("renders empty state when spec is null", () => {
    render(<SpecTab spec={null} recording={null} diverged={false} />);
    expect(
      screen.getByText(/no spec yet — synthesize a transaction first/i),
    ).toBeTruthy();
  });

  it("renders rule name and context type for a non-null spec", () => {
    const spec = emptySpec({
      context_rule: {
        name: "blend_yield_claim",
        context_type: {
          kind: "call_contract",
          address: "CAYIP67UABCDEFGHIJKLMNOPQRSTUVWXYZ012345last4",
        },
        valid_until: null,
      },
    });
    const { container } = render(
      <SpecTab spec={spec} recording={emptyRecording()} diverged={false} />,
    );
    expect(container.textContent).toContain("blend_yield_claim");
    expect(container.textContent).toContain("call_contract");
    // address truncated middle: head + ellipsis + tail
    expect(container.textContent).toContain("CAYIP6");
    expect(container.textContent).toContain("ast4");
  });

  it("renders existing-primitive slot with primitive name + params JSON", () => {
    const spec = emptySpec({
      policies: [
        {
          kind: "existing",
          primitive: "spending_limit",
          params: { max: "1000", asset: "USDC" },
        },
      ],
    });
    const { container } = render(
      <SpecTab spec={spec} recording={emptyRecording()} diverged={false} />,
    );
    expect(container.textContent).toContain("primitive:spending_limit");
    expect(container.textContent).toContain('"max":"1000"');
    expect(container.textContent).toContain('"asset":"USDC"');
  });

  it("renders generated slot with its constraint list", () => {
    const spec = emptySpec({
      policies: [
        {
          kind: "generated",
          template_family: "function_allowlist",
          constraints: [
            { kind: "function_allowlist", functions: ["transfer", "approve"] },
            {
              kind: "amount_range",
              fn_name: "transfer",
              arg_index: 2,
              min_string: "1",
              max_string: "1000",
            },
          ],
        },
      ],
    });
    const { container } = render(
      <SpecTab spec={spec} recording={emptyRecording()} diverged={false} />,
    );
    expect(container.textContent).toContain("generated:function_allowlist");
    expect(container.textContent).toContain("function_allowlist: [transfer, approve]");
    expect(container.textContent).toContain("amount_range: transfer#2 [1..1000]");
  });

  it("renders reasoning trace for function_allowlist from recording", () => {
    const recording: Recording = {
      ...emptyRecording(),
      contracts: [
        {
          address: "CABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789xyz",
          function: "transfer",
          args: [],
        },
      ],
    };
    const spec = emptySpec({
      policies: [
        {
          kind: "generated",
          template_family: "function_allowlist",
          constraints: [{ kind: "function_allowlist", functions: ["transfer"] }],
        },
      ],
    });
    const { container } = render(
      <SpecTab spec={spec} recording={recording} diverged={false} />,
    );
    const text = container.textContent ?? "";
    expect(text).toContain("↳ derived from");
    expect(text).toContain("transfer");
    // address truncated (middle ellipsis); head appears, raw full string does not.
    expect(text).toContain("CABCDE");
    expect(text).not.toContain("CABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789xyz");
  });

  it("renders divergence warning when diverged === true", () => {
    const spec = emptySpec();
    render(<SpecTab spec={spec} recording={emptyRecording()} diverged={true} />);
    const warning = screen.getByTestId("spec-divergence-warning");
    expect(warning).toBeTruthy();
    expect(warning.textContent).toContain(
      "⚠ source diverges from spec — bundle will note divergence",
    );
  });

  it("omits reasoning trace when recording has no matching source data", () => {
    // function_allowlist references 'transfer' but recording has 'mint'
    const recording: Recording = {
      ...emptyRecording(),
      contracts: [
        {
          address: "CXYZ1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ12",
          function: "mint",
          args: [],
        },
      ],
    };
    const spec = emptySpec({
      policies: [
        {
          kind: "generated",
          template_family: "function_allowlist",
          constraints: [{ kind: "function_allowlist", functions: ["transfer"] }],
        },
      ],
    });
    const { container } = render(
      <SpecTab spec={spec} recording={recording} diverged={false} />,
    );
    expect(container.textContent ?? "").not.toContain("↳ derived from");
  });
});
