// Tests for SimulateTab — empty state, all-pass, permit failure, deny vector
// failure, re-simulate button enablement/wiring, resim error surfacing.
//
// Fixtures use the real SimReport + DenyResult interfaces from lib/types.ts
// — no partials, no any. This guarantees test data drift is caught by tsc.

import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { SimulateTab } from "../SimulateTab";
import type { SimReport, DenyResult } from "../../../lib/types";

function makeDeny(overrides: DenyResult): DenyResult {
  return overrides;
}

function makeReport(overrides: SimReport): SimReport {
  return overrides;
}

const ALL_PASS_REPORT: SimReport = makeReport({
  spec_id: "spec_test",
  permit: { passed: true, error: null },
  deny_results: [
    makeDeny({
      name: "deny_unknown_fn",
      passed: true,
      expected_error_code: 1010,
      actual_error_code: 1010,
    }),
    makeDeny({
      name: "deny_over_cap",
      passed: true,
      expected_error_code: 1011,
      actual_error_code: 1011,
    }),
  ],
  total_vectors: 2,
  passed: 2,
  timestamp_ledger: 12345,
});

const PERMIT_FAILED_REPORT: SimReport = makeReport({
  spec_id: "spec_test",
  permit: { passed: false, error: "host_fn returned ScErrorCode(1010)" },
  deny_results: [
    makeDeny({
      name: "deny_unknown_fn",
      passed: true,
      expected_error_code: 1010,
      actual_error_code: 1010,
    }),
  ],
  total_vectors: 1,
  passed: 1,
  timestamp_ledger: 12345,
});

const DENY_FAILED_REPORT: SimReport = makeReport({
  spec_id: "spec_test",
  permit: { passed: true, error: null },
  deny_results: [
    makeDeny({
      name: "deny_over_cap",
      passed: false,
      expected_error_code: 1011,
      actual_error_code: null,
    }),
    makeDeny({
      name: "deny_unknown_fn",
      passed: true,
      expected_error_code: 1010,
      actual_error_code: 1010,
    }),
  ],
  total_vectors: 2,
  passed: 1,
  timestamp_ledger: 12345,
});

describe("SimulateTab", () => {
  it("renders empty state when report is null", () => {
    render(
      <SimulateTab
        report={null}
        modified={false}
        onReSimulate={() => {}}
        busy={false}
        resimError={null}
      />
    );
    expect(screen.getByTestId("simulate-empty").textContent).toBe(
      "no simulation yet — synthesize first"
    );
    expect(screen.queryByTestId("permit-row")).toBeNull();
    expect(screen.queryByTestId("synthesizer-bug-banner")).toBeNull();
  });

  it("renders green status + permit success text + no bug banner on all-pass report", () => {
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        modified={false}
        onReSimulate={() => {}}
        busy={false}
        resimError={null}
      />
    );
    const status = screen.getByTestId("simulate-status");
    expect(status.textContent).toContain("all passed");
    const statusDot = within(status).getByTestId("status-dot");
    expect(statusDot.getAttribute("data-passed")).toBe("true");

    expect(
      screen.getByText("policy permits the recorded transaction (as expected)")
    ).toBeTruthy();
    expect(screen.queryByTestId("synthesizer-bug-banner")).toBeNull();
    expect(screen.queryByTestId("deny-fail-footer")).toBeNull();
  });

  it("surfaces permit failure as red banner + raw error text + error code chip", () => {
    render(
      <SimulateTab
        report={PERMIT_FAILED_REPORT}
        modified={false}
        onReSimulate={() => {}}
        busy={false}
        resimError={null}
      />
    );
    // synthesizer-bug banner appears because permit.passed === false
    const banner = screen.getByTestId("synthesizer-bug-banner");
    expect(banner.textContent).toContain("rejected the recorded tx");

    // permit row shows raw error string
    const permitErr = screen.getByTestId("permit-error-text");
    expect(permitErr.textContent).toBe("host_fn returned ScErrorCode(1010)");

    // chip with E_SIM_PERMIT_DENIED code
    expect(screen.getByText("E_SIM_PERMIT_DENIED")).toBeTruthy();

    // status dot is red
    const status = screen.getByTestId("simulate-status");
    const statusDot = within(status).getByTestId("status-dot");
    expect(statusDot.getAttribute("data-passed")).toBe("false");
  });

  it("flags a failing deny vector with red dot + failure footer + suggestion + banner", () => {
    render(
      <SimulateTab
        report={DENY_FAILED_REPORT}
        modified={false}
        onReSimulate={() => {}}
        busy={false}
        resimError={null}
      />
    );
    // banner appears because deny passed=false && actual=null
    expect(screen.getByTestId("synthesizer-bug-banner")).toBeTruthy();

    // two deny cards: one failed, one passed
    const cards = screen.getAllByTestId("deny-card");
    expect(cards).toHaveLength(2);
    const failed = cards.find((c) => c.getAttribute("data-passed") === "false");
    expect(failed).toBeTruthy();
    if (!failed) throw new Error("no failed deny card");

    expect(within(failed).getByText("policy failed to deny this vector")).toBeTruthy();
    // 1011 maps to "amount over cap" in the hint table
    expect(within(failed).getByText(/amount over cap/)).toBeTruthy();

    // header shows X/Y format
    const status = screen.getByTestId("simulate-status");
    expect(status.textContent).toContain("1/2 vectors passed");
  });

  it("disables re-simulate button when modified=false", () => {
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        modified={false}
        onReSimulate={() => {}}
        busy={false}
        resimError={null}
      />
    );
    const btn = screen.getByRole("button", { name: "re-simulate from source" });
    expect((btn as HTMLButtonElement).disabled).toBe(true);
  });

  it("fires onReSimulate when clicked + modified=true", () => {
    const spy = vi.fn();
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        modified={true}
        onReSimulate={spy}
        busy={false}
        resimError={null}
      />
    );
    const btn = screen.getByRole("button", { name: "re-simulate from source" });
    expect((btn as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(btn);
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("surfaces resimError with the error code chip and human description", () => {
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        modified={true}
        onReSimulate={() => {}}
        busy={false}
        resimError={{
          code: "E_CODEGEN_COMPILE_FAILED",
          detail: "error[E0432]: unresolved import `foo`",
        }}
      />
    );
    const banner = screen.getByTestId("resim-error-banner");
    expect(within(banner).getByText("E_CODEGEN_COMPILE_FAILED")).toBeTruthy();
    expect(banner.textContent).toContain("policy code generation failed");
    expect(banner.textContent).toContain("error[E0432]: unresolved import `foo`");
  });

  it("shows simulating… label when busy=true", () => {
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        modified={true}
        onReSimulate={() => {}}
        busy={true}
        resimError={null}
      />
    );
    const btn = screen.getByRole("button", { name: "simulating…" });
    expect((btn as HTMLButtonElement).disabled).toBe(true);
  });
});
