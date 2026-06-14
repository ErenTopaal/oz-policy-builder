import { useCallback, useMemo, useState } from "react";
import { McpClient, type McpConfig } from "../../lib/mcp";
import { McpError, type CreateSnapshotInput, type Snapshot, type SnapshotRef } from "../../lib/types";

export interface UseSnapshotResult {
  /** the loaded snapshot, populated by `loadSnapshot`. */
  snapshot: Snapshot | null;
  /** true while either createSnapshot or loadSnapshot is in flight. */
  loading: boolean;
  /** the most recent error from either call, or null. Verbatim McpError. */
  error: McpError | null;
  /**
   * persist a snapshot. resolves to the SnapshotRef on success, or null
   * on failure (caller can read `error` for details). Real network call.
   */
  createSnapshot: (input: CreateSnapshotInput) => Promise<SnapshotRef | null>;
  /**
   * load a snapshot by id. on success, `snapshot` is set and the Snapshot
   * record is returned. on failure, returns null and `error` is set.
   * E_SNAPSHOT_NOT_FOUND is the canonical "expired or never created" path.
   */
  loadSnapshot: (snapshotId: string) => Promise<Snapshot | null>;
}

/**
 * useSnapshot accepts an optional pre-built client. PlaygroundPage passes
 * its own client (which may be the test-seam stub) so we don't create a
 * second MCP session per page. With no `client` arg, the hook builds a
 * real McpClient from cfg — that is the production path.
 */
export function useSnapshot(
  cfg: McpConfig,
  injectedClient?: McpClient,
): UseSnapshotResult {
  const [snapshot, setSnapshot] = useState<Snapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<McpError | null>(null);

  // one client per cfg identity. cfg is normally a stable readConfig()
  // return; if it changes we rebuild the client (which also restarts the
  // MCP session — desirable, since a config change means a new endpoint).
  const client = useMemo(() => {
    if (injectedClient) return injectedClient;
    try {
      return new McpClient(cfg);
    } catch (e) {
      // CLIENT_NOT_CONFIGURED — defer surfacing until the caller actually
      // tries to use the hook; cache the error here.
      if (e instanceof McpError) return e;
      throw e;
    }
  }, [cfg, injectedClient]);

  const createSnapshot = useCallback(
    async (input: CreateSnapshotInput): Promise<SnapshotRef | null> => {
      if (client instanceof McpError) {
        setError(client);
        return null;
      }
      setLoading(true);
      setError(null);
      try {
        const ref = await client.createSnapshot(input);
        return ref;
      } catch (e) {
        const err = e instanceof McpError ? e : new McpError("E_UNKNOWN", String(e), -32099);
        setError(err);
        return null;
      } finally {
        setLoading(false);
      }
    },
    [client],
  );

  const loadSnapshot = useCallback(
    async (snapshotId: string): Promise<Snapshot | null> => {
      if (client instanceof McpError) {
        setError(client);
        return null;
      }
      setLoading(true);
      setError(null);
      try {
        const s = await client.getSnapshot(snapshotId);
        setSnapshot(s);
        return s;
      } catch (e) {
        const err = e instanceof McpError ? e : new McpError("E_UNKNOWN", String(e), -32099);
        setError(err);
        setSnapshot(null);
        return null;
      } finally {
        setLoading(false);
      }
    },
    [client],
  );

  return { snapshot, loading, error, createSnapshot, loadSnapshot };
}
