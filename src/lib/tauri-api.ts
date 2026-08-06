import { invoke } from "@tauri-apps/api/core";

import type {
  ClaudeConfig,
  ClaudeStatus,
  SessionDetail,
  SessionSummary,
} from "@/features/sessions/types";

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
  runClaude: (request: {
    cwd: string;
    prompt: string;
    sessionId: string | null;
    permissionMode: string;
  }) => invoke<void>("claude_run", request),
  stopClaude: () => invoke<void>("claude_stop"),
};
