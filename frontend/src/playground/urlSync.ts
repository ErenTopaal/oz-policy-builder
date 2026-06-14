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
