// /playground E2E against LIVE production. Single spec, multiple tests.
//
// Honest source policy (carried in from the task prompt + standing
// feedback-honesty-no-fakes):
//   - We hit the REAL production deployment via E2E_BASE_URL. No localhost
//     spawn, no mock backend. Snapshot writes are real records (30-day TTL).
//   - The fresh preset is picked at test time by probing the public preset
//     files. If none are fresh (cron stalled), the whole spec test.skips with
//     an explicit reason — never a hardcoded fallback hash.
//   - If the backend health endpoint is down, all tests skip with the real
//     HTTP code surfaced. We never paper over a transport failure as pass.
//
// Selectors come from data-testid / aria-label hooks already in the panel
// source. Where Monaco is involved we drive the textarea inside the editor
// directly (the editor mounts a contenteditable textarea per Monaco contract).

import { expect, test, type Page } from "@playwright/test";

const BASE_URL = process.env.E2E_BASE_URL ?? "https://policy.erentopal.xyz";
const STALE_AFTER_MS = 6 * 60 * 60 * 1000; // 6h — mirrors usePresets.ts

type PresetKey = "sample" | "blend" | "sep41" | "soroswap";

const PRESET_URLS: Record<PresetKey, string> = {
  sample: "/sample-hash.txt",
  blend: "/preset-blend.txt",
  sep41: "/preset-sep41.txt",
  soroswap: "/preset-soroswap.txt",
};

// Preset dropdown labels mirror InputPanel.tsx PRESET_LABELS so we can
// select via <option> text without depending on internal enum values.
const PRESET_LABELS: Record<PresetKey, string> = {
  sample: "Current sample",
  blend: "Blend yield-claim",
  sep41: "SEP-41 transfer",
  soroswap: "Soroswap swap",
};

const HEX64 = /^[0-9a-f]{64}$/;

interface FreshPreset {
  key: PresetKey;
  label: string;
  hash: string;
}

/** Probe public preset files; return the first fresh one, or null. */
async function findFreshPreset(): Promise<FreshPreset | null> {
  const now = Date.now();
  // Order matches usePresets fetch order; sample first, then named presets.
  const order: PresetKey[] = ["sample", "sep41", "blend", "soroswap"];
  for (const key of order) {
    const url = `${BASE_URL}${PRESET_URLS[key]}`;
    try {
      const r = await fetch(url, { cache: "no-store" });
      if (!r.ok) continue;
      const text = (await r.text()).trim().toLowerCase();
      if (!HEX64.test(text)) continue;
      const lm = r.headers.get("last-modified");
      if (!lm) continue;
      const ts = Date.parse(lm);
      if (Number.isNaN(ts)) continue;
      if (now - ts > STALE_AFTER_MS) continue;
      return { key, label: PRESET_LABELS[key], hash: text };
    } catch {
      // continue
    }
  }
  return null;
}

/** Probe backend reachability — surfaces real HTTP code on failure. */
async function backendStatus(): Promise<{ ok: boolean; code: number | string }> {
  try {
    const r = await fetch(`${BASE_URL}/playground`, { cache: "no-store" });
    return { ok: r.ok, code: r.status };
  } catch (e) {
    return { ok: false, code: e instanceof Error ? e.message : "unknown" };
  }
}

let FRESH: FreshPreset | null = null;
let BACKEND_OK = false;
let BACKEND_CODE: number | string = 0;

test.beforeAll(async () => {
  const bs = await backendStatus();
  BACKEND_OK = bs.ok;
  BACKEND_CODE = bs.code;
  if (!BACKEND_OK) return;
  FRESH = await findFreshPreset();
});

test.beforeEach(async () => {
  if (!BACKEND_OK) {
    test.skip(true, `backend unavailable: ${BACKEND_CODE}`);
  }
  if (!FRESH) {
    test.skip(true, "no fresh preset available — verify cron is running");
  }
});

// Each Playwright test gets its own `page` fixture (fresh browser context),
// so we can't share synthesize state across tests via module-level flags.
// We run synth in every test that needs it; the cached preset (sample) is
// fast on the backend (<10s end-to-end), so the extra cost is small and
// the test honestly reproduces the full flow each time.

/**
 * Append text to the first (editable) Monaco editor on the SourceTab.
 *
 * Monaco doesn't expose a public way to drive its model from the page
 * surface without holding a reference to the editor instance, and the
 * @monaco-editor/react wrapper doesn't put the instance on window. The
 * stable path that survives across Monaco versions is:
 *   1. Click into `.monaco-editor .view-lines` — Monaco binds its
 *      internal focus + caret state on a click event in the view-lines
 *      region (NOT on a programmatic textarea.focus(), which leaves the
 *      editor's internal "focused" flag false and drops key input).
 *   2. Drive Cmd/Ctrl+End to move the cursor to end-of-file.
 *   3. Use `keyboard.type` — each keystroke hits Monaco's textarea via
 *      Chromium's real key-event pipeline, the same path real users get.
 *
 * The probe edit3 experiment in this branch's history verified this
 * works on Chromium + monaco-editor 0.55: the appended comment shows in
 * `.view-line` text and the diverged badge / re-simulate button update.
 */
async function appendToMonaco(page: Page, suffix: string): Promise<void> {
  const firstEditor = page.locator(".monaco-editor").first();
  // .view-lines is the click target Monaco listens on for focus + caret
  // placement. Clicking the wrapper directly often lands on a margin /
  // gutter element and Monaco refuses keyboard input.
  await firstEditor.locator(".view-lines").click();
  // Move caret to end of buffer. On macOS the "go to end of document"
  // chord is Cmd+ArrowDown; on Win/Linux it's Ctrl+End. The probe in
  // this branch's history verified Meta+ArrowDown lands at the true
  // EOF on macOS (Ctrl+End does NOT — Chromium intercepts it).
  const isMac = process.platform === "darwin";
  await page.keyboard.press(isMac ? "Meta+ArrowDown" : "Control+End");
  await page.keyboard.press("End");
  // insertText fires a single `beforeinput`+`input` pair carrying the
  // full string. Monaco treats that as one edit batch.
  await page.keyboard.insertText(suffix);
}

/**
 * Remove a previously-inserted single-line suffix from the editor.
 * We DON'T re-click the .view-lines region (clicking moves the caret
 * to wherever the cursor coordinates land, which on a slow machine
 * can be mid-buffer). Instead we re-anchor to end of file with the
 * platform's "cursor to end of document" chord, then Home → Shift+End
 * → Backspace ×2 to wipe the line + the leading newline.
 */
async function stripForbiddenLine(page: Page): Promise<void> {
  const isMac = process.platform === "darwin";
  await page.keyboard.press(isMac ? "Meta+ArrowDown" : "Control+End");
  await page.keyboard.press("End");
  await page.keyboard.press("Home");
  await page.keyboard.press("Shift+End");
  await page.keyboard.press("Backspace");
  await page.keyboard.press("Backspace"); // drop the leading newline
}

async function ensureSynthesized(page: Page): Promise<void> {
  await selectPresetAndSynthesize(page);
}

async function selectPresetAndSynthesize(page: Page): Promise<void> {
  const preset = FRESH!;
  await page.goto("/playground");
  // The preset dropdown is the <select aria-label="preset">. Selecting by
  // label is robust against the value enum (sample / sep41 / blend / soroswap).
  await page.getByLabel("preset").selectOption({ label: preset.label });
  // Hash input auto-fills from the preset; assert that to lock in determinism.
  await expect(page.getByLabel("transaction hash")).toHaveValue(preset.hash, {
    timeout: 5_000,
  });
  const synthBtn = page.getByRole("button", { name: /^synthesize$/i });
  await expect(synthBtn).toBeEnabled();
  await synthBtn.click();

  // Wait for synth to complete by switching to Simulate tab and asserting
  // simulate-status appears. That's a positive signal — it only renders
  // once `state.latestReport` is set, i.e. simulatePolicy resolved with a
  // real report from the backend. Waiting for the button label alone is
  // not enough: a cached preset can race through phases in <1s and the
  // `name: "synthesize"` matcher would match before the click registers.
  await page.getByRole("tab", { name: "Simulate" }).click();
  await expect(page.getByTestId("simulate-status")).toBeVisible({
    timeout: 90_000,
  });
  // Also ensure source artifacts have loaded — Promise.allSettled in the
  // controller fires both getPolicyArtifacts and simulatePolicy in
  // parallel, so the report can resolve before the artifacts. Tests
  // 3/4 then race the Source tab against an empty editor.
  await page.getByRole("tab", { name: "Source" }).click();
  // The empty state has testid source-tab-empty; the loaded state has
  // source-tab. Wait for the latter then for at least one .view-line.
  await expect(page.getByTestId("source-tab")).toBeVisible({
    timeout: 60_000,
  });
  await page.waitForFunction(
    () => {
      const lines = document.querySelectorAll(".monaco-editor .view-line");
      // First Monaco is the lib.rs editor — verify it has SOME non-empty
      // content. The Cargo.toml sidebar always has content too, so we
      // can't just check >0 globally; we check the first editor.
      const firstEditor = document.querySelector(".monaco-editor");
      if (!firstEditor) return false;
      const firstLines = firstEditor.querySelectorAll(".view-line");
      return Array.from(firstLines).some(
        (l) => (l.textContent ?? "").trim().length > 0,
      );
    },
    { timeout: 60_000 },
  );
}

test.describe("playground — live production end-to-end", () => {
  test("Test 1: loads playground with fresh preset", async ({ page }) => {
    await page.goto("/playground");

    // The /playground page heading. The Vite app's <title> is the global
    // landing-page title ("oz-policy-builder · record a tx, …"), so the
    // page-level title for /playground is the H1 inside the page header
    // (PlaygroundPage.tsx Header) which reads "playground". Assert that
    // heading is present and contains "playground".
    await expect(
      page.getByRole("heading", { level: 1, name: /playground/i }),
      "/playground page should render an H1 heading containing 'playground'",
    ).toBeVisible({ timeout: 10_000 });

    // The fresh preset's <option> must be enabled. usePresets fetches the
    // public preset files asynchronously, so we wait for the dropdown to
    // settle on a state where at least one named preset is enabled.
    const optionLabel = FRESH!.label;
    const presetSelect = page.getByLabel("preset");
    await expect(presetSelect).toBeVisible();
    // Poll the live DOM until the fresh-preset option is no longer disabled.
    await expect(async () => {
      const status = await presetSelect.evaluate((sel, label) => {
        const opts = Array.from(
          (sel as HTMLSelectElement).querySelectorAll("option"),
        ) as HTMLOptionElement[];
        const match = opts.find((o) => (o.textContent ?? "").includes(label));
        return match ? { found: true, disabled: match.disabled } : { found: false };
      }, optionLabel);
      if (!status.found) {
        throw new Error(`option for "${optionLabel}" not found in dropdown`);
      }
      if (status.disabled) {
        throw new Error(
          `option for "${optionLabel}" is still disabled (presets still loading?)`,
        );
      }
    }).toPass({ timeout: 10_000 });

    // All four tab labels visible.
    for (const tab of ["Spec", "Source", "Simulate", "Bundle"]) {
      await expect(
        page.getByRole("tab", { name: tab }),
        `tab "${tab}" should be visible`,
      ).toBeVisible();
    }
  });

  test("Test 2: synthesize → inspect spec → inspect source", async ({
    page,
  }) => {
    await ensureSynthesized(page);

    // Spec tab — tree should show "rule" glyph + ":" then context_rule.name.
    await page.getByRole("tab", { name: "Spec" }).click();
    const specTabPanel = page.locator('[role="tabpanel"][aria-label="spec"]');
    await expect(specTabPanel).toBeVisible({ timeout: 10_000 });
    await expect(
      specTabPanel,
      "Spec tree should render with rule: prefix",
    ).toContainText(/rule\s*:/);

    // At least one constraint kind name (e.g. AssetEq / FunctionEq / Range).
    // We assert a generic capitalised CamelCase identifier shows up in the
    // tree to avoid coupling the test to a specific spec shape.
    await expect(specTabPanel).toContainText(/[A-Z][a-zA-Z]{2,}/);

    // Source tab — Monaco renders Rust. Look for the canonical Soroban
    // header line (either #![no_std] or use soroban_sdk). Use viewport text
    // because Monaco hides off-screen lines in its DOM-virtualised view.
    await page.getByRole("tab", { name: "Source" }).click();
    const sourceTab = page.getByTestId("source-tab");
    await expect(sourceTab).toBeVisible({ timeout: 30_000 });
    // Wait for Monaco to finish loading + render at least one ".view-line".
    await page.waitForSelector(".monaco-editor .view-line", {
      timeout: 30_000,
    });
    // There are two Monaco editors in the SourceTab — the main lib.rs
    // editor and the read-only Cargo.toml sidebar. We want the first
    // (main editor); .first() picks it deterministically.
    const editorText = await sourceTab
      .locator(".monaco-editor")
      .first()
      .innerText();
    expect(
      editorText,
      "Source editor must contain Soroban policy code markers",
    ).toMatch(/#!\[no_std\]|use\s+soroban_sdk/);

    // Simulate tab — permit row + at least one deny vector card.
    await page.getByRole("tab", { name: "Simulate" }).click();
    await expect(page.getByTestId("permit-row")).toBeVisible({
      timeout: 15_000,
    });
    const denyCards = page.getByTestId("deny-card");
    const denyCount = await denyCards.count();
    expect(
      denyCount,
      `expected at least one deny vector card, got ${denyCount}`,
    ).toBeGreaterThanOrEqual(1);
  });

  test("Test 3: edit source → re-simulate", async ({ page }) => {
    test.setTimeout(6 * 60 * 1000); // 6 min — first sandbox compile is slow.
    await ensureSynthesized(page);

    await page.getByRole("tab", { name: "Source" }).click();
    await expect(page.getByTestId("source-tab")).toBeVisible({
      timeout: 30_000,
    });
    await page.waitForSelector(".monaco-editor .view-line", {
      timeout: 30_000,
    });

    // Append a comment line to Monaco's main editor. Monaco's input goes
    // through a hidden textarea + an internal model; the most reliable way
    // to inject text without flaking on focus / key-binding races is to
    // drive the model via Monaco's exported API (window.monaco) and let
    // React's onChange listener fire. See `appendToMonaco` helper below.
    const stamp = new Date().toISOString();
    await appendToMonaco(page, `\n// playground-e2e edit at ${stamp}`);

    // re-simulate button should become enabled once the buffer diverges
    // from the synthesized lib.rs.
    const reSim = page.getByTestId("re-simulate");
    await expect(reSim, "re-simulate should enable after edit").toBeEnabled({
      timeout: 10_000,
    });

    await reSim.click();

    // First custom compile in the sandbox can take ~3-5 min cold. After
    // completion the button label flips back to "re-simulate" and the
    // Simulate tab gets a fresh report.
    await expect(
      page.getByTestId("re-simulate"),
      "re-simulate should reset to 're-simulate' label after run completes",
    ).toHaveText(/re-simulate/i, { timeout: 5 * 60 * 1000 });

    await page.getByRole("tab", { name: "Simulate" }).click();
    await expect(page.getByTestId("simulate-status")).toBeVisible({
      timeout: 30_000,
    });
    const statusDot = page.getByTestId("status-dot").first();
    await expect(statusDot).toBeVisible();
    // We don't insist on green — the comment shouldn't break logic but
    // upstream synth could flake. We DO insist that no resim error banner
    // surfaced, which would indicate the request errored.
    const resimError = page.getByTestId("resim-error-banner");
    expect(
      await resimError.count(),
      "resim-error-banner should not appear after a clean comment edit",
    ).toBe(0);
  });

  test("Test 4: pre-flight rejects unsafe", async ({ page }) => {
    await ensureSynthesized(page);

    await page.getByRole("tab", { name: "Source" }).click();
    await expect(page.getByTestId("source-tab")).toBeVisible({
      timeout: 30_000,
    });
    await page.waitForSelector(".monaco-editor .view-line", {
      timeout: 30_000,
    });

    // Type an unsafe block at the bottom — preflight should reject within
    // 500ms (client-side regex, no network). We assert ≤2s as a generous
    // budget that still catches a regression to server-side preflight.
    await appendToMonaco(page, "\nunsafe { let _ = 1; }");

    const pill = page.getByTestId("preflight-pill");
    await expect(
      pill,
      "preflight pill should appear after unsafe block is typed",
    ).toBeVisible({ timeout: 2_000 });

    const reSim = page.getByTestId("re-simulate");
    await expect(
      reSim,
      "re-simulate should be disabled while preflight has a forbidden pattern",
    ).toBeDisabled();

    // Now remove the unsafe block. The caret is at end-of-buffer after
    // appendToMonaco, so we re-anchor (Meta+ArrowDown / Ctrl+End) and
    // kill the current line + the leading newline we inserted.
    await stripForbiddenLine(page);

    await expect(
      pill,
      "preflight pill should vanish once unsafe block is removed",
    ).toHaveCount(0, { timeout: 2_000 });
  });

  test("Test 5: share snapshot → reopen URL", async ({ page, browser }) => {
    test.setTimeout(6 * 60 * 1000);
    await ensureSynthesized(page);

    // Click share. URL should transition to /playground/s/<8-char-id>.
    await page.getByTestId("share-button").click();

    await page.waitForURL(/\/playground\/s\/[0-9a-zA-Z_-]{8,}$/, {
      timeout: 30_000,
    });

    const url = page.url();
    const match = url.match(/\/playground\/s\/([0-9a-zA-Z_-]{8,})$/);
    expect(match, `URL did not match snapshot pattern: ${url}`).not.toBeNull();
    const snapshotId = match![1];

    // Clipboard should contain the share URL. Permission is granted in the
    // playwright config so we can read it directly.
    const clipboardText = await page.evaluate(() =>
      navigator.clipboard.readText(),
    );
    expect(
      clipboardText,
      "clipboard should contain the share URL after click",
    ).toContain(snapshotId);

    // Reopen in a fresh context to isolate cookies + clipboard state.
    const fresh = await browser.newContext();
    const freshPage = await fresh.newPage();
    await freshPage.goto(`/playground/s/${snapshotId}`);

    // Snapshot hydrate should populate Spec / Source / Simulate. Spec is
    // the cheapest to assert (no Monaco load required).
    await freshPage.getByRole("tab", { name: "Spec" }).click();
    const specPanel = freshPage.locator('[role="tabpanel"][aria-label="spec"]');
    await expect(specPanel).toBeVisible({ timeout: 15_000 });
    await expect(specPanel).toContainText(/rule\s*:/, { timeout: 15_000 });

    await freshPage.getByRole("tab", { name: "Simulate" }).click();
    await expect(freshPage.getByTestId("simulate-status")).toBeVisible({
      timeout: 15_000,
    });
    await expect(freshPage.getByTestId("permit-row")).toBeVisible();

    await freshPage.getByRole("tab", { name: "Source" }).click();
    await freshPage.waitForSelector(".monaco-editor .view-line", {
      timeout: 30_000,
    });

    // The share badge in the header acts as the read-only indicator —
    // it persists the snapshot id and is the visible signal the user is
    // on a shared link. Assert it shows the same id.
    const shareBadge = freshPage.getByTestId("share-badge");
    await expect(
      shareBadge,
      "share badge should display the snapshot id on hydrated reopen",
    ).toContainText(snapshotId);

    await fresh.close();
  });
});
