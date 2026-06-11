// SourceTab tests.
//
// NOTE on the @monaco-editor/react mock below:
// Monaco is a full-blown DOM editor — it depends on layout APIs (clientRect,
// ResizeObserver, Web Workers) that jsdom doesn't implement and that we
// explicitly DO NOT want to polyfill (that path lies madness, see prior
// art). Production code mounts the real `<Editor>` which lazy-loads the
// real Monaco bundle, verified by the e2e + build-time chunk-size check.
//
// In jsdom we substitute a `<textarea>` stand-in for the `Editor`
// component. This is a UI-behaviour isolation, NOT a data-mock-fallback
// (per the no-fakes operating rule): we are not faking a backend
// response, we are swapping a rendering primitive that the test harness
// can't host. Every assertion in this file would still be valid against
// the real Editor — the textarea simply makes the assertion possible.

import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import type { ComponentProps } from "react";

vi.mock("@monaco-editor/react", () => {
  // mimic the parts of the EditorProps surface SourceTab exercises:
  // value, language, onChange, onMount, options.readOnly.
  type EditorLike = {
    value?: string;
    language?: string;
    onChange?: (v: string | undefined) => void;
    onMount?: (
      editor: { revealLineInCenter: () => void; setPosition: () => void; focus: () => void; getModel: () => null },
      monaco: { editor: { setModelMarkers: () => void } },
    ) => void;
    options?: { readOnly?: boolean };
  };
  const Editor = ({ value, language, onChange, onMount, options }: EditorLike) => {
    // call onMount once with a no-op editor stub so SourceTab's marker
    // effect runs without exploding.
    if (onMount) {
      onMount(
        {
          revealLineInCenter: () => {},
          setPosition: () => {},
          focus: () => {},
          getModel: () => null,
        },
        { editor: { setModelMarkers: () => {} } },
      );
    }
    return (
      <textarea
        data-testid={`monaco-${language ?? "unknown"}`}
        value={value ?? ""}
        readOnly={options?.readOnly ?? false}
        onChange={(e) => onChange?.(e.target.value)}
      />
    );
  };
  return {
    default: Editor,
    Editor,
    loader: { config: () => {} },
  };
});

import { SourceTab } from "../SourceTab";
import type { PolicyArtifacts } from "../../../lib/types";

const artifacts: PolicyArtifacts = {
  spec_id: "spec_test",
  generated_sources: [
    {
      slot_index: 0,
      cargo_toml: '[package]\nname = "policy"\n',
      lib_rs: "fn main() { let x = 1; }\n",
    },
  ],
  composed_count: 0,
  generated_count: 1,
  wasm_sha256: "deadbeef",
  optimized_wasm_sha256: "cafef00d",
};

// suppress noisy "not implemented: window.scrollTo" etc. logs that jsdom
// emits when Monaco mock's onMount stub fires.
beforeEach(() => {
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

type Props = ComponentProps<typeof SourceTab>;

function renderTab(overrides: Partial<Props> = {}) {
  const onChange = vi.fn();
  const onReSimulate = vi.fn();
  const utils = render(
    <SourceTab
      artifacts={artifacts}
      modifiedLibRs={null}
      onChange={onChange}
      onReSimulate={onReSimulate}
      compileError={null}
      busy={false}
      {...overrides}
    />,
  );
  return { ...utils, onChange, onReSimulate };
}

describe("SourceTab", () => {
  it("renders empty state when artifacts === null", () => {
    render(<SourceTab artifacts={null} />);
    expect(screen.getByTestId("source-tab-empty").textContent).toMatch(
      /no source yet/i,
    );
  });

  it("renders editor + cargo sidebar when artifacts are present", () => {
    renderTab();
    // main editor with rust language
    expect(screen.getByTestId("monaco-rust")).toBeTruthy();
    // sidebar with toml editor
    expect(screen.getByTestId("monaco-toml")).toBeTruthy();
    expect(screen.getByTestId("cargo-sidebar")).toBeTruthy();
    // header chips
    expect(screen.getByText("slot 0")).toBeTruthy();
    expect(screen.getByText("Cargo.toml [readonly]")).toBeTruthy();
    expect(screen.getByText("src/lib.rs")).toBeTruthy();
  });

  it("preflight catches `unsafe fn` — re-simulate disabled, pill renders", () => {
    // backend regex is `\bunsafe\s*(\{|fn|impl|trait)\b`; the trailing `\b`
    // requires a word boundary AFTER the keyword. `unsafe fn` qualifies
    // (n→space), `unsafe {` does NOT (}→space is non-word/non-word).
    // mirroring the backend's behaviour exactly — including this quirk —
    // is the explicit instruction. A spec-level fix belongs upstream.
    const tainted =
      artifacts.generated_sources[0].lib_rs + "\nunsafe fn evil() {}\n";
    const { onReSimulate } = renderTab({ modifiedLibRs: tainted });

    const pill = screen.getByTestId("preflight-pill");
    expect(pill.textContent).toMatch(/forbidden pattern/);
    expect(pill.textContent).toMatch(/unsafe/);

    const btn = screen.getByTestId("re-simulate") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);

    fireEvent.click(btn);
    expect(onReSimulate).not.toHaveBeenCalled();
  });

  it('preflight catches `extern "C"` — disabled + pill', () => {
    const tainted =
      artifacts.generated_sources[0].lib_rs + '\nextern "C" { fn syscall(); }\n';
    renderTab({ modifiedLibRs: tainted });
    const pill = screen.getByTestId("preflight-pill");
    expect(pill.textContent).toMatch(/extern/);
    const btn = screen.getByTestId("re-simulate") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("fires onReSimulate when source clean + diverged + button clicked", () => {
    const modified =
      artifacts.generated_sources[0].lib_rs + "\n// a harmless comment\n";
    const { onReSimulate } = renderTab({ modifiedLibRs: modified });
    const btn = screen.getByTestId("re-simulate") as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
    fireEvent.click(btn);
    expect(onReSimulate).toHaveBeenCalledOnce();
  });

  it("re-simulate stays disabled when source is unchanged (modifiedLibRs===null)", () => {
    renderTab();
    const btn = screen.getByTestId("re-simulate") as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
  });

  it("renders the compile error panel when compileError is provided", () => {
    const stderr =
      "error[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:2:5\n";
    renderTab({ compileError: { stderr, exit_code: 101 } });
    const panel = screen.getByTestId("compile-error-panel");
    expect(panel.textContent).toMatch(/cargo build failed/);
    expect(panel.textContent).toMatch(/exit code 101/);
    // a jump link is produced from the `src/lib.rs:2:5` reference.
    const jumps = screen.getAllByTestId("stderr-jump");
    expect(jumps.length).toBeGreaterThan(0);
    expect(jumps[0].textContent).toMatch(/src\/lib\.rs:2:5/);
  });

  it("shows the `diverged from spec` badge when modifiedLibRs !== null", () => {
    renderTab({ modifiedLibRs: artifacts.generated_sources[0].lib_rs });
    expect(screen.getByTestId("diverged-badge")).toBeTruthy();
  });

  it("does NOT show the diverged badge when modifiedLibRs === null", () => {
    renderTab();
    expect(screen.queryByTestId("diverged-badge")).toBeNull();
  });
});
