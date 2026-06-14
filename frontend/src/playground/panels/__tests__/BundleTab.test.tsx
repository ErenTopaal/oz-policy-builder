import { describe, it, expect, beforeEach, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { BundleTab } from "../BundleTab";
import type {
  PolicyArtifacts,
  PolicySpec,
  SimReport,
} from "../../../lib/types";

function makeSpec(overrides: Partial<PolicySpec> = {}): PolicySpec {
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
    recording_ref: {
      hash: "abcdef0123456789".repeat(4), // 64 hex
      schema: "oz_policy_core.recording.v1",
    },
    ...overrides,
  };
}

function makeArtifacts(libRs: string): PolicyArtifacts {
  return {
    spec_id: "spec_test",
    generated_sources: [
      {
        slot_index: 0,
        cargo_toml: `[package]
name = "oz-policy-generated-slot-0"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]
`,
        lib_rs: libRs,
      },
    ],
    composed_count: 0,
    generated_count: 1,
    wasm_sha256: "deadbeef",
    optimized_wasm_sha256: "deadbeef",
  };
}

function makeReport(): SimReport {
  return {
    spec_id: "spec_test",
    permit: { passed: true, error: null },
    deny_results: [
      {
        name: "rejects_wrong_function",
        passed: true,
        expected_error_code: 1001,
        actual_error_code: 1001,
      },
    ],
    total_vectors: 2,
    passed: 2,
    timestamp_ledger: 1234567,
  };
}

const ORIGINAL_LIB_RS = `#![no_std]
use soroban_sdk::contract;

#[contract]
pub struct Policy;
`;

describe("BundleTab", () => {
  beforeEach(() => {
    // jsdom does not implement these; stub once per test.
    (URL as unknown as { createObjectURL: () => string }).createObjectURL = vi.fn(
      () => "blob:fake-url",
    );
    (URL as unknown as { revokeObjectURL: () => void }).revokeObjectURL = vi.fn();
  });

  describe("empty state", () => {
    it("renders empty when no props passed", () => {
      render(<BundleTab />);
      expect(screen.getByTestId("bundle-empty")).toBeTruthy();
    });

    it("renders empty when artifacts is null", () => {
      render(
        <BundleTab
          artifacts={null}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="my_rule"
        />,
      );
      expect(screen.getByTestId("bundle-empty")).toBeTruthy();
    });

    it("renders empty when spec is null", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={null}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="my_rule"
        />,
      );
      expect(screen.getByTestId("bundle-empty")).toBeTruthy();
    });

    it("renders empty when report is null", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={null}
          modifiedLibRs={null}
          ruleName="my_rule"
        />,
      );
      expect(screen.getByTestId("bundle-empty")).toBeTruthy();
    });
  });

  describe("file tree", () => {
    it("lists all expected entries with non-zero sizes", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="my_rule"
        />,
      );
      const expected = [
        "README.md",
        "src/lib.rs",
        "Cargo.toml",
        "spec.json",
        "sim-report.md",
      ];
      for (const path of expected) {
        const entry = screen.getByTestId(`bundle-entry-${path}`);
        expect(entry).toBeTruthy();
        // size column never says "0 B" — every entry has real content.
        expect(entry.textContent).not.toMatch(/\(0 B/);
      }
    });

    it("zip filename uses short spec id derived from recording hash", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="my_rule"
        />,
      );
      const tree = screen.getByTestId("bundle-tree");
      // first 8 hex chars of "abcdef0123456789..." = "abcdef01"
      expect(tree.textContent).toContain("oz-policy-bundle-abcdef01.zip");
    });

    it("omits DIVERGENCE.md when modifiedLibRs is null", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="my_rule"
        />,
      );
      expect(screen.queryByTestId("bundle-entry-DIVERGENCE.md")).toBeNull();
      // and "edited" tag is absent from src/lib.rs row.
      const libRow = screen.getByTestId("bundle-entry-src/lib.rs");
      expect(libRow.textContent).not.toContain("edited");
    });

    it("includes DIVERGENCE.md and edited tag when modifiedLibRs is set", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={ORIGINAL_LIB_RS + "\n// edited\n"}
          ruleName="my_rule"
        />,
      );
      expect(screen.getByTestId("bundle-entry-DIVERGENCE.md")).toBeTruthy();
      const libRow = screen.getByTestId("bundle-entry-src/lib.rs");
      expect(libRow.textContent).toContain("edited");
    });
  });

  describe("install snippet", () => {
    it("contains the rule name when provided", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="claim_yield_only"
        />,
      );
      const snippet = screen.getByTestId("install-snippet");
      expect(snippet.textContent).toContain('--rule-name "claim_yield_only"');
      expect(snippet.textContent).toContain("oz-policy-cli install");
      expect(snippet.textContent).toContain("stellar contract build");
    });

    it("falls back to 'auto' when ruleName is null", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName={null}
        />,
      );
      const snippet = screen.getByTestId("install-snippet");
      expect(snippet.textContent).toContain('--rule-name "auto"');
    });

    it("derives wasm name from Cargo.toml package name", () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="r"
        />,
      );
      const snippet = screen.getByTestId("install-snippet");
      // dashes from the cargo name become underscores in the wasm artifact name.
      expect(snippet.textContent).toContain("oz_policy_generated_slot_0.wasm");
    });
  });

  describe("download", () => {
    it("clicking download triggers zip generation without throwing", async () => {
      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={ORIGINAL_LIB_RS + "\n// tweak\n"}
          ruleName="r"
        />,
      );
      const btn = screen.getByTestId("bundle-download") as HTMLButtonElement;
      fireEvent.click(btn);
      await waitFor(() => {
        expect((URL.createObjectURL as unknown as ReturnType<typeof vi.fn>)).toHaveBeenCalled();
      });
    });

    it("generates a zip containing exactly the expected entries", async () => {
      // dynamically import JSZip in the test to verify behaviour against
      // the real library — same code path the panel uses.
      const { default: JSZip } = await import("jszip");
      const captured: Record<string, string> = {};
      const origFile = JSZip.prototype.file;
      const spy = vi
        .spyOn(JSZip.prototype, "file")
        // @ts-expect-error overload soup
        .mockImplementation(function (this: JSZip, path: string, content: string) {
          captured[path] = content;
          // @ts-expect-error overload soup
          return origFile.call(this, path, content);
        });

      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={ORIGINAL_LIB_RS + "\n// tweak\n"}
          ruleName="r"
        />,
      );
      const btn = screen.getByTestId("bundle-download") as HTMLButtonElement;
      fireEvent.click(btn);
      await waitFor(() => {
        expect(Object.keys(captured).sort()).toEqual(
          [
            "Cargo.toml",
            "DIVERGENCE.md",
            "README.md",
            "spec.json",
            "src/lib.rs",
            "sim-report.md",
          ].sort(),
        );
      });
      // sim-report.md mentions the single deny vector we built.
      expect(captured["sim-report.md"]).toContain("rejects_wrong_function");
      // DIVERGENCE.md present because modifiedLibRs !== null.
      expect(captured["DIVERGENCE.md"]).toContain("Divergence from synthesizer");
      spy.mockRestore();
    });

    it("omits DIVERGENCE.md from zip when source not edited", async () => {
      const { default: JSZip } = await import("jszip");
      const captured: Record<string, string> = {};
      const origFile = JSZip.prototype.file;
      const spy = vi
        .spyOn(JSZip.prototype, "file")
        // @ts-expect-error overload soup
        .mockImplementation(function (this: JSZip, path: string, content: string) {
          captured[path] = content;
          // @ts-expect-error overload soup
          return origFile.call(this, path, content);
        });

      render(
        <BundleTab
          artifacts={makeArtifacts(ORIGINAL_LIB_RS)}
          spec={makeSpec()}
          report={makeReport()}
          modifiedLibRs={null}
          ruleName="r"
        />,
      );
      fireEvent.click(screen.getByTestId("bundle-download"));
      await waitFor(() => {
        expect(Object.keys(captured)).toContain("README.md");
      });
      expect(Object.keys(captured)).not.toContain("DIVERGENCE.md");
      spy.mockRestore();
    });
  });
});
