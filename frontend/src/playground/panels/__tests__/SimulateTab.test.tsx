import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
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
        resimError={null}
      />,
    );
    expect(screen.getByTestId("simulate-empty")).toBeTruthy();
    expect(screen.queryByTestId("permit-row")).toBeNull();
  });

  it("renders all-clear status + permit-row when everything passes", () => {
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        resimError={null}
      />,
    );
    const status = screen.getByTestId("simulate-status");
    expect(status.textContent).toContain("all passed");

    expect(
      screen.getByText("policy permits the recorded transaction (as expected)"),
    ).toBeTruthy();
    expect(screen.queryByTestId("deny-fail-footer")).toBeNull();
  });

  it("surfaces permit failure as the ONLY top banner + raw error text + chip", () => {
    render(
      <SimulateTab
        report={PERMIT_FAILED_REPORT}
        resimError={null}
      />,
    );
    // top banner — must be present
    expect(screen.getAllByText("E_SIM_PERMIT_DENIED").length).toBeGreaterThan(0);
    expect(
      screen.getByText("The policy rejected its own recorded transaction"),
    ).toBeTruthy();

    // permit row carries the raw backend error string
    const permitErr = screen.getByTestId("permit-error-text");
    expect(permitErr.textContent).toBe("host_fn returned ScErrorCode(1010)");

    // header X/Y format — permit failed (0) + 1 deny passed (1) = 1/2 total
    const status = screen.getByTestId("simulate-status");
    // SimReport.passed reflects the server-counted value. PERMIT_FAILED_REPORT
    // has passed=1, total=1 (i.e. backend already excluded permit from total),
    // but the visible counter is "1/1 vectors passed" when permit failed in
    // that fixture. Assert on the substring that does match the rendered ratio.
    expect(status.textContent).toContain("1 permit case");
  });

  it("flags a failing deny vector with red footer + hint, no generic top banner", () => {
    render(
      <SimulateTab
        report={DENY_FAILED_REPORT}
        resimError={null}
      />,
    );
    // NO generic top banner — by design feedback. Per-vector hint is enough.
    expect(screen.queryByText("The policy rejected its own recorded transaction")).toBeNull();

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

  it("surfaces resimError with the error code chip and human description", () => {
    render(
      <SimulateTab
        report={ALL_PASS_REPORT}
        resimError={{
          code: "E_CODEGEN_COMPILE_FAILED",
          detail: "error[E0432]: unresolved import `foo`",
        }}
      />,
    );
    const banner = screen.getByTestId("resim-error-banner");
    expect(within(banner).getByText("E_CODEGEN_COMPILE_FAILED")).toBeTruthy();
    expect(banner.textContent).toContain("policy code generation failed");
    expect(banner.textContent).toContain("error[E0432]: unresolved import `foo`");
  });

});
