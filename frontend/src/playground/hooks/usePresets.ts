// fetches the four hash presets that seed the InputPanel preset dropdown.
//
// each preset file lives in `frontend/public/` and is refreshed by a
// server-side job. we hit them with `cache: 'no-store'` so we never serve
// a stale hash from the browser cache. honesty rules:
//   - if the fetch fails (404, network error, etc.) the entry is
//     `unavailable` — never a placeholder hash, never a synthetic value.
//   - if the body isn't a 64-char hex string, the entry is `unavailable`
//     (treated as invalid the same way as a missing file).
//   - if the `Last-Modified` header is missing OR older than 6h, the entry
//     is `stale`: the hash is still surfaced so users can pick it, but the
//     dropdown will annotate it so they understand the freshness signal.
//   - otherwise the entry is `fresh`.
//
// no in-memory fallback, no synthesized hash — the production code path
// here is the only source of truth. tests stub `fetch` to simulate network
// outcomes; nothing inside this module fabricates data.

import { useEffect, useState } from "react";

export type PresetKey = "sample" | "blend" | "sep41" | "soroswap";

export type PresetStatus = "fresh" | "stale" | "unavailable";

export interface PresetEntry {
  /** 64-char lowercase hex tx hash, or null when status === 'unavailable'. */
  hash: string | null;
  status: PresetStatus;
}

export interface Presets {
  sample: PresetEntry;
  blend: PresetEntry;
  sep41: PresetEntry;
  soroswap: PresetEntry;
}

export interface UsePresetsResult {
  presets: Presets;
  loading: boolean;
}

const PRESET_URLS: Record<PresetKey, string> = {
  sample: "/sample-hash.txt",
  blend: "/preset-blend.txt",
  sep41: "/preset-sep41.txt",
  soroswap: "/preset-soroswap.txt",
};

const STALE_AFTER_MS = 6 * 60 * 60 * 1000; // 6h

const UNAVAILABLE: PresetEntry = { hash: null, status: "unavailable" };

const HEX64 = /^[0-9a-f]{64}$/;

async function fetchPreset(url: string, now: number): Promise<PresetEntry> {
  try {
    const r = await fetch(url, { cache: "no-store" });
    if (!r.ok) return UNAVAILABLE;
    const text = (await r.text()).trim().toLowerCase();
    if (!HEX64.test(text)) return UNAVAILABLE;
    const lastModified = r.headers.get("Last-Modified");
    if (!lastModified) return { hash: text, status: "stale" };
    const ts = Date.parse(lastModified);
    if (Number.isNaN(ts)) return { hash: text, status: "stale" };
    const ageMs = now - ts;
    if (ageMs > STALE_AFTER_MS) return { hash: text, status: "stale" };
    return { hash: text, status: "fresh" };
  } catch {
    return UNAVAILABLE;
  }
}

export function usePresets(): UsePresetsResult {
  const [presets, setPresets] = useState<Presets>({
    sample: UNAVAILABLE,
    blend: UNAVAILABLE,
    sep41: UNAVAILABLE,
    soroswap: UNAVAILABLE,
  });
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    const now = Date.now();
    Promise.all([
      fetchPreset(PRESET_URLS.sample, now),
      fetchPreset(PRESET_URLS.blend, now),
      fetchPreset(PRESET_URLS.sep41, now),
      fetchPreset(PRESET_URLS.soroswap, now),
    ]).then(([sample, blend, sep41, soroswap]) => {
      if (cancelled) return;
      setPresets({ sample, blend, sep41, soroswap });
      setLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return { presets, loading };
}
