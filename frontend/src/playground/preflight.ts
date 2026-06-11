// client-side mirror of `crates/oz-policy-mcp/src/tools/simulate_custom_source.rs::check_forbidden`.
//
// The backend re-runs this exact same regex sweep (defense in depth, per
// spec §6.1 + §6.3). The two implementations MUST agree on accept/reject —
// they share a corpus. If you change a pattern here, change it in the Rust
// source too, and rerun the corpus tests.
//
// pattern labels are copied verbatim from the backend (the `label` field
// of `PatternEntry`). The frontend surfaces the label to the user; the
// backend surfaces the same label in the `E_PREFLIGHT_FORBIDDEN_PATTERN`
// error payload. Keeping them identical means our UI can render the
// server's error the same way it renders a client-side hit.

export type ForbiddenHit = {
  ok: false;
  /** stable identifier — same string as the backend's `pattern` field. */
  pattern: string;
  /** 1-indexed line number of the first matching line. */
  line: number;
  /** the offending line, trimmed to 120 chars (UI display ceiling). */
  lineText: string;
};

export type PreflightResult = { ok: true } | ForbiddenHit;

type CompiledPattern = {
  label: string;
  re: RegExp;
};

// IMPORTANT: this list mirrors `PATTERNS` in
// `crates/oz-policy-mcp/src/tools/simulate_custom_source.rs`. Labels are
// the raw regex strings — exactly what the backend uses for `PatternEntry::label`.
// Backend rationale: pattern == label means a single source of truth and
// the error surface is identical client/server.
const PATTERNS: CompiledPattern[] = [
  {
    // trailing `\b` refuses to match `unsafe { ... }` because `{` is non-word
    // and the next char is space/non-word — boundary fails. anchor `fn`/`impl`/`trait`
    // with `\b` individually; leave `{` unanchored.
    label: String.raw`\bunsafe\s*(\{|\bfn|\bimpl|\btrait)`,
    re: /\bunsafe\s*(\{|\bfn|\bimpl|\btrait)/,
  },
  {
    label: String.raw`\bextern\s+"[A-Za-z]+"`,
    re: /\bextern\s+"[A-Za-z]+"/,
  },
  {
    label: String.raw`#\[\s*proc_macro(_derive|_attribute)?\s*[\(\]]`,
    re: /#\[\s*proc_macro(_derive|_attribute)?\s*[\(\]]/,
  },
  {
    label: String.raw`#\[\s*link(_name)?\b`,
    re: /#\[\s*link(_name)?\b/,
  },
  {
    // backend uses a single combined regex for include_bytes! / include_str!.
    // mirror that — splitting into two here would cause a label mismatch
    // between client and server even though they would still both reject.
    label: String.raw`\binclude_(bytes|str)!\s*\(`,
    re: /\binclude_(bytes|str)!\s*\(/,
  },
  {
    // literal `"build.rs"` substring. backend's regex is `"build\.rs"`
    // — i.e. matched as a Rust string literal containing build.rs.
    label: String.raw`"build.rs"`,
    re: /"build\.rs"/,
  },
];

const MAX_LINE_DISPLAY = 120;

/**
 * Scan `src` for forbidden patterns. Returns `{ ok: true }` if clean, or
 * the first matching pattern with location. Lines are split on `\n` and
 * a trailing `\r` is stripped so CRLF inputs report the same line text
 * a unix user would see — same behaviour as the backend's split + strip.
 */
export function checkForbidden(src: string): PreflightResult {
  const lines = src.split("\n");
  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const line = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    for (const { re, label } of PATTERNS) {
      if (re.test(line)) {
        return {
          ok: false,
          pattern: label,
          line: i + 1,
          lineText:
            line.length > MAX_LINE_DISPLAY
              ? line.slice(0, MAX_LINE_DISPLAY) + "…"
              : line,
        };
      }
    }
  }
  return { ok: true };
}

/**
 * Expose the pattern label list for tests that want to verify parity
 * with the backend's `PATTERNS` array.
 */
export const FORBIDDEN_PATTERN_LABELS: ReadonlyArray<string> = PATTERNS.map(
  (p) => p.label,
);
