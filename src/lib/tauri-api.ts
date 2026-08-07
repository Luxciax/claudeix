import { invoke } from "@tauri-apps/api/core";

import type {
  BackupSnapshot,
  ClaudeConfig,
  ClaudeStatus,
  EditTarget,
  SessionDetail,
  SessionSummary,
  TranscriptEditResult,
  TranscriptRestoreResult,
} from "@/features/sessions/types";
import type { ExternalRuntimeInfo, RuntimeSnapshot } from "@/features/runtime/types";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export const api = {
  claudeStatus: () => invoke<ClaudeStatus>("claude_status"),
  claudeConfig: () => invoke<ClaudeConfig>("claude_config"),
  workspaceRoot: () => invoke<string>("workspace_root"),
  pickWorkspace: () => invoke<string | null>("project_pick"),
  openWorkspace: (path: string) => invoke<string>("project_open", { path }),
  scanSessions: (workspaceRoot: string, force = false) =>
    invoke<SessionSummary[]>("session_scan", { workspaceRoot, force }),
  getSession: (sessionKey: string) =>
    invoke<SessionDetail>("session_get", { sessionKey }),
  refreshSession: (sessionKey: string) =>
    invoke<SessionDetail>("session_refresh", { sessionKey }),
  editTranscript: (request: {
    sessionKey: string;
    target: EditTarget;
    expectedFileFingerprint: string;
    newText: string;
    allowAssistantText: boolean;
  }) => invoke<TranscriptEditResult>("transcript_edit", { request }),
  listTranscriptSnapshots: (sessionKey: string) =>
    invoke<BackupSnapshot[]>("transcript_snapshots_list", { sessionKey }),
  restoreTranscriptSnapshot: (request: {
    sessionKey: string;
    snapshotId: string;
    expectedFileFingerprint: string;
  }) => invoke<TranscriptRestoreResult>("transcript_snapshot_restore", { request }),
  startRuntime: (request: {
    workspaceRoot: string;
    sessionKey: string | null;
    prompt: string;
    permissionMode: string;
    model?: string | null;
    effort?: string | null;
  }) => invoke<RuntimeSnapshot>("runtime_start", { request }),
  stopRuntime: (runtimeId: string) =>
    invoke<RuntimeSnapshot>("runtime_stop", { runtimeId }),
  decidePermission: (request: {
    runtimeId: string;
    requestId: string;
    decision: "allow" | "deny";
    message?: string | null;
  }) => invoke<RuntimeSnapshot>("runtime_permission_decide", { request }),
  getRuntime: (runtimeId: string) =>
    invoke<RuntimeSnapshot>("runtime_get", { runtimeId }),
  listRuntimes: () => invoke<RuntimeSnapshot[]>("runtime_list"),
  probeExternalRuntime: (sessionKey: string) =>
    invoke<ExternalRuntimeInfo>("runtime_probe_external", { sessionKey }),
};
