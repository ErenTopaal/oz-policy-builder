import { useReducer } from "react";
import type { Dispatch } from "react";
import type {
  PolicyArtifacts,
  PolicySpec,
  Recording,
  SimReport,
} from "../../lib/types";

// spec §4.1 — single PlaygroundState shape, all panels read from this.
export interface PlaygroundState {
  recording: Recording | null;
  spec: PolicySpec | null;
  artifacts: PolicyArtifacts | null;
  // user-edited lib.rs. null = "no edit yet, use artifacts.generated_sources[0].lib_rs"
  modifiedLibRs: string | null;
  latestReport: SimReport | null;
  snapshotId: string | null;
}

export const initialState: PlaygroundState = {
  recording: null,
  spec: null,
  artifacts: null,
  modifiedLibRs: null,
  latestReport: null,
  snapshotId: null,
};

export type Action =
  | { type: "setRecording"; recording: Recording | null }
  | { type: "setSpec"; spec: PolicySpec | null }
  | { type: "setArtifacts"; artifacts: PolicyArtifacts | null }
  | { type: "setModifiedLibRs"; modifiedLibRs: string | null }
  | { type: "setReport"; report: SimReport | null }
  | { type: "setSnapshotId"; snapshotId: string | null }
  | { type: "reset" };

export function reducer(state: PlaygroundState, action: Action): PlaygroundState {
  switch (action.type) {
    case "setRecording":
      return { ...state, recording: action.recording };
    case "setSpec":
      return { ...state, spec: action.spec };
    case "setArtifacts":
      return { ...state, artifacts: action.artifacts };
    case "setModifiedLibRs":
      return { ...state, modifiedLibRs: action.modifiedLibRs };
    case "setReport":
      return { ...state, latestReport: action.report };
    case "setSnapshotId":
      return { ...state, snapshotId: action.snapshotId };
    case "reset":
      return initialState;
  }
}

export interface UsePlaygroundStateResult {
  state: PlaygroundState;
  dispatch: Dispatch<Action>;
}

export function usePlaygroundState(): UsePlaygroundStateResult {
  const [state, dispatch] = useReducer(reducer, initialState);
  return { state, dispatch };
}
