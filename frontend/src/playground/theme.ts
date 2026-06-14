export const T = {
  page: "#1c1c20",
  ink: "#f4f4f5",
  ink2: "#b2b2b8",
  faint: "#8e8e96",
  faint2: "#6b6b73",
  stone: "#2f2f35",
  surface: "#26262b",
  surfaceHi: "#34343b",
  toned: "#1f1f24",
  dark: "#f0f0f3", // light pill / primary button bg
  darkInk: "#1c1c20",
  line: "rgba(255,255,255,0.1)",
  line2: "rgba(255,255,255,0.06)",
  codeBg: "#141417",
  codeInk: "#f6f6f8",
  codeFaint: "#9aa0a8",
  // json highlighter
  kKey: "#f3f3f8",
  kStr: "#cfcfd6",
  kNum: "#e4e4ec",
  kBool: "#c6c6d0",
  kPunc: "#9a9aa0",
  kCmt: "#7d7d86",
  // status
  danger: "#e0846e",
  dangerBg: "rgba(224,132,110,0.14)",
  okChip: "rgba(255,255,255,0.12)",
  // fonts
  mono: "'JetBrains Mono', monospace",
  disp: "'Bricolage Grotesque', sans-serif",
  body: "'Hanken Grotesk', sans-serif",
} as const;

// JSON syntax highlighter — returns an array of <span> nodes. Ported from
// the design's hlJson(obj) helper. Caller wraps in a <pre>.
import type { ReactNode } from "react";
import { Fragment, createElement } from "react";

export function hlJson(obj: unknown): ReactNode {
  const s = JSON.stringify(obj, null, 2);
  if (typeof s !== "string") return null;
  const re = /("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false|null)\b|(-?\d+(?:\.\d+)?)/g;
  const out: ReactNode[] = [];
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;
  while ((m = re.exec(s)) !== null) {
    if (m.index > last) {
      out.push(
        createElement(
          "span",
          { key: k++, style: { color: T.kPunc } },
          s.slice(last, m.index),
        ),
      );
    }
    if (m[1] !== undefined) {
      out.push(
        createElement(
          "span",
          { key: k++, style: { color: m[2] ? T.kKey : T.kStr } },
          m[1],
        ),
      );
      if (m[2]) {
        out.push(
          createElement(
            "span",
            { key: k++, style: { color: T.kPunc } },
            m[2],
          ),
        );
      }
    } else if (m[3] !== undefined) {
      out.push(
        createElement("span", { key: k++, style: { color: T.kBool } }, m[3]),
      );
    } else if (m[4] !== undefined) {
      out.push(
        createElement("span", { key: k++, style: { color: T.kNum } }, m[4]),
      );
    }
    last = re.lastIndex;
  }
  if (last < s.length) {
    out.push(
      createElement(
        "span",
        { key: k++, style: { color: T.kPunc } },
        s.slice(last),
      ),
    );
  }
  return createElement(Fragment, null, out);
}

export function bytesOf(s: string): number {
  try {
    return new TextEncoder().encode(s).length;
  } catch {
    return s.length;
  }
}

export function fmtBytes(n: number): string {
  if (n === 0) return "0 B";
  if (n < 1024) return `${n} B`;
  return `${(n / 1024).toFixed(1)} kB`;
}
