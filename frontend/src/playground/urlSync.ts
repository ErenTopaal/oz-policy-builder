// Tiny URL-sync helper used by PlaygroundPage to flip the address bar to
// /playground/s/<snapshotId> after a successful createSnapshot, without
// remounting the page.
//
// Why not just useNavigate? We deliberately want history.pushState semantics
// so that opening a share link in a new tab still works (full reload),
// while the *current* tab keeps its in-memory state. react-router v7's
// `navigate(...)` would do the same here, but exporting this as a thin
// helper lets tests stub it (and keeps the page module slim).

export function pushSnapshotUrl(snapshotId: string): string {
  const url = `/playground/s/${snapshotId}`;
  // jsdom + happy-dom both expose history.pushState; guard for SSR.
  if (typeof window !== "undefined" && window.history) {
    window.history.pushState({}, "", url);
  }
  return url;
}

/**
 * Returns the absolute URL the user should see in their address bar after
 * a snapshot is created — used for clipboard copy. Falls back to the
 * relative path if window.location is unavailable.
 */
export function snapshotShareUrl(snapshotId: string): string {
  if (typeof window !== "undefined" && window.location) {
    const origin = window.location.origin;
    return `${origin}/playground/s/${snapshotId}`;
  }
  return `/playground/s/${snapshotId}`;
}
