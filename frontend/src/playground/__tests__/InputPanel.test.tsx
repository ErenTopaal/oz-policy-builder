import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { InputPanel, type SubmitIntent } from "../InputPanel";
import type { PlaygroundState, Action } from "../hooks/usePlaygroundState";
import type { Presets } from "../hooks/usePresets";

const VALID_HASH_A = "a".repeat(64);
const VALID_HASH_B = "b".repeat(64);
const VALID_HASH_C = "c".repeat(64);

const dummyState: PlaygroundState = {
  recording: null,
  spec: null,
  artifacts: null,
  modifiedLibRs: null,
  latestReport: null,
  snapshotId: null,
};

function makePresets(overrides: Partial<Presets> = {}): Presets {
  return {
    sample: { hash: VALID_HASH_A, status: "fresh" },
    blend: { hash: VALID_HASH_B, status: "fresh" },
    sep41: { hash: VALID_HASH_C, status: "stale" },
    soroswap: { hash: null, status: "unavailable" },
    ...overrides,
  };
}

function renderPanel(
  opts: {
    busy?: boolean;
    backendDown?: boolean;
    presets?: Presets;
    onSubmit?: (intent: SubmitIntent) => void;
    onCancel?: () => void;
  } = {},
) {
  const onSubmit = opts.onSubmit ?? vi.fn();
  const onCancel = opts.onCancel ?? vi.fn();
  const dispatch = vi.fn() as unknown as React.Dispatch<Action>;
  const utils = render(
    <InputPanel
      state={dummyState}
      dispatch={dispatch}
      presets={opts.presets ?? makePresets()}
      busy={opts.busy ?? false}
      backendDown={opts.backendDown ?? false}
      onSubmit={onSubmit}
      onCancel={onCancel}
    />,
  );
  return { ...utils, onSubmit, onCancel };
}

describe("InputPanel", () => {
  it("renders all controls with hash mode by default", () => {
    renderPanel();
    expect(screen.getByLabelText("transaction hash")).toBeTruthy();
    expect(screen.queryByLabelText("envelope XDR")).toBeNull();
    expect(screen.getByTestId("preset-trigger")).toBeTruthy();
    expect(screen.getByLabelText("lifetime ledgers")).toBeTruthy();
    expect(screen.getByLabelText("rule name")).toBeTruthy();
    expect(screen.getByRole("button", { name: /synthesize/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: "hash" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "envelope XDR" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "testnet" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "mainnet" })).toBeTruthy();
  });

  it("switching to envelope XDR mode hides hash input and shows textarea", () => {
    renderPanel();
    fireEvent.click(screen.getByRole("button", { name: "envelope XDR" }));
    expect(screen.queryByLabelText("transaction hash")).toBeNull();
    const ta = screen.getByLabelText("envelope XDR") as HTMLTextAreaElement;
    expect(ta.tagName).toBe("TEXTAREA");
  });

  it("picking a preset auto-fills hash and switches input to hash mode", () => {
    renderPanel();
    fireEvent.click(screen.getByRole("button", { name: "envelope XDR" }));
    expect(screen.queryByLabelText("transaction hash")).toBeNull();

    fireEvent.click(screen.getByTestId("preset-trigger"));
    fireEvent.click(screen.getByTestId("preset-row-blend"));

    const input = screen.getByLabelText("transaction hash") as HTMLInputElement;
    expect(input.value).toBe(VALID_HASH_B);
  });

  it("disables the unavailable preset row in the dropdown", () => {
    renderPanel();
    fireEvent.click(screen.getByTestId("preset-trigger"));
    const row = screen.getByTestId("preset-row-soroswap") as HTMLButtonElement;
    expect(row.disabled).toBe(true);
    // chip carries the 'unavailable' status word
    const chip = screen.getByTestId("preset-chip-soroswap");
    expect(chip.textContent).toContain("unavailable");
    // hover reason on the row
    expect(row.title).toContain("no recent testnet activity");
  });

  it("marks stale presets with the 'stale' chip but keeps them selectable", () => {
    renderPanel();
    fireEvent.click(screen.getByTestId("preset-trigger"));
    const row = screen.getByTestId("preset-row-sep41") as HTMLButtonElement;
    expect(row.disabled).toBe(false);
    const chip = screen.getByTestId("preset-chip-sep41");
    expect(chip.textContent).toContain("stale");
  });

  it("dropdown footnote explains the 'unavailable' status meaning", () => {
    renderPanel();
    fireEvent.click(screen.getByTestId("preset-trigger"));
    const panel = screen.getByTestId("preset-panel");
    expect(panel.textContent).toContain(
      "unavailable = no recent testnet activity on that contract",
    );
  });

  it("submit calls onSubmit with the expected intent shape", () => {
    const onSubmit = vi.fn();
    renderPanel({ onSubmit });
    const input = screen.getByLabelText("transaction hash") as HTMLInputElement;
    fireEvent.change(input, { target: { value: VALID_HASH_A } });
    fireEvent.click(screen.getByRole("button", { name: /synthesize/i }));

    expect(onSubmit).toHaveBeenCalledTimes(1);
    const intent = onSubmit.mock.calls[0][0] as SubmitIntent;
    expect(intent.inputMode).toBe("hash");
    expect(intent.hash).toBe(VALID_HASH_A);
    expect(intent.envelope_xdr_base64).toBeUndefined();
    expect(intent.network).toBe("testnet");
    expect(intent.tightness).toBe("exact");
    expect(intent.mode).toBe("auto");
    expect(intent.lifetime).toBe(432000);
    expect(intent.ruleName).toBeUndefined();
  });

  it("preset selection sets the suggested tightness (sep41 → small_margin)", () => {
    const onSubmit = vi.fn();
    renderPanel({
      onSubmit,
      // sep41 needs a fresh status so we can submit
      presets: makePresets({ sep41: { hash: VALID_HASH_C, status: "fresh" } }),
    });
    fireEvent.click(screen.getByTestId("preset-trigger"));
    fireEvent.click(screen.getByTestId("preset-row-sep41"));
    fireEvent.click(screen.getByRole("button", { name: /synthesize/i }));
    const intent = onSubmit.mock.calls[0][0] as SubmitIntent;
    expect(intent.tightness).toBe("small_margin");
    expect(intent.hash).toBe(VALID_HASH_C);
  });

  it("submit disabled when hash is invalid", () => {
    const onSubmit = vi.fn();
    renderPanel({ onSubmit });
    fireEvent.change(screen.getByLabelText("transaction hash"), {
      target: { value: "not-a-hash" },
    });
    const btn = screen.getByRole("button", { name: /synthesize/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    fireEvent.click(btn);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("submit disabled when backend is down", () => {
    const onSubmit = vi.fn();
    renderPanel({ onSubmit, backendDown: true });
    fireEvent.change(screen.getByLabelText("transaction hash"), {
      target: { value: VALID_HASH_A },
    });
    const btn = screen.getByRole("button", { name: /live mode unavailable/i }) as HTMLButtonElement;
    expect(btn.disabled).toBe(true);
    fireEvent.click(btn);
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("shows cancel button while busy and calls onCancel", () => {
    const onCancel = vi.fn();
    renderPanel({ busy: true, onCancel });
    const cancel = screen.getByRole("button", { name: /cancel/i });
    fireEvent.click(cancel);
    expect(onCancel).toHaveBeenCalledTimes(1);
  });
});
