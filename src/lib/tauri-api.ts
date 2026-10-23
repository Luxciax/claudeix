import { invoke } from "@tauri-apps/api/core";

import type {
  BackupSnapshot,
  ClaudeConfig,
  ClaudeStatus,
  EditRecoveryItem,
  EditRecoveryResolveResult,
  EditTarget,
  SessionDetail,
  SessionRenameResult,
  SessionSummary,
  TranscriptAuditEntry,
  TranscriptEditResult,
  TranscriptRestoreResult,
} from "@/features/sessions/types";
import type { ExternalRuntimeInfo, RuntimeSnapshot } from "@/features/runtime/types";
import type {
  EngineEditResult,
  EngineDescriptor,
  EngineMutationRecoveryItem,
  EngineMutationRecoveryResolveResult,
  NativeSessionDetail,
  NativeSessionSummary,
  EngineRestoreResult,
  EngineTrashDeletePreview,
  EngineTrashItemSummary,
  EngineTrashPurgePreview,
  EngineTrashRestorePreview,
  EngineTrashRestoreResult,
} from "@/features/engines/types";
import type {
  TrashDeletePreview,
  TrashItemSummary,
  TrashPurgePreview,
  TrashRestorePreview,
  TrashRestoreResult,
} from "@/features/trash/types";

export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export type ClaudeSettingsFile = {
  path: string;
  exists: boolean;
  content: string;
};

export type ClaudeProviderKind = "anthropic" | "gateway" | "bedrock" | "vertex" | "foundry";

export type ClaudeProviderProfile = {
  id: string;
  name: string;
  kind: ClaudeProviderKind;
  baseUrl: string;
  apiKey: string;
  authToken: string;
  model: string;
  opusModel: string;
  sonnetModel: string;
  haikuModel: string;
  subagentModel: string;
  enableModelDiscovery: boolean;
  customHeaders: string;
  extraEnv: Record<string, string>;
};

export type ClaudeProviderStore = {
  version: number;
  activeProfileId: string | null;
  profiles: ClaudeProviderProfile[];
  path?: string | null;
};

export const api = {
  listEngines: () => invoke<EngineDescriptor[]>("engine_list"),
  scanEngineSessions: (engineId: string, workspaceRoot: string) =>
    invoke<NativeSessionSummary[]>("engine_session_scan", { engineId, workspaceRoot }),
  getEngineSession: (engineId: string, sessionKey: string, workspaceRoot: string) =>
    invoke<NativeSessionDetail>("engine_session_get", {
      engineId,
      sessionKey,
      workspaceRoot,
    }),
  editEngineMessage: (request: {
    engineId: string;
    sessionKey: string;
    workspaceRoot: string;
    expectedSourceRevision: string;
    target: EditTarget;
    newText: string;
  }) => invoke<EngineEditResult>("engine_edit_message", { request }),
  restoreEngineMutation: (request: {
    engineId: string;
    sessionKey: string;
    workspaceRoot: string;
    transactionId: string;
    expectedSourceRevision: string;
  }) => invoke<EngineRestoreResult>("engine_restore_mutation", { request }),
  listEngineMutationRecovery: () =>
    invoke<EngineMutationRecoveryItem[]>("engine_mutation_recovery_list"),
  resolveEngineMutationRecovery: (request: {
    transactionId: string;
    action: "keep_current" | "restore_backup";
  }) =>
    invoke<EngineMutationRecoveryResolveResult>("engine_mutation_recovery_resolve", { request }),
  previewEngineTrashDelete: (engineId: string, sessionKey: string, workspaceRoot: string) =>
    invoke<EngineTrashDeletePreview>("engine_trash_delete_preview", {
      engineId,
      sessionKey,
      workspaceRoot,
    }),
  moveEngineSessionToTrash: (request: {
    engineId: string;
    sessionKey: string;
    workspaceRoot: string;
    expectedPreviewFingerprint: string;
  }) => invoke<EngineTrashItemSummary>("engine_trash_move", { request }),
  listEngineTrash: (engineId?: string | null) =>
    invoke<EngineTrashItemSummary[]>("engine_trash_list", { engineId: engineId ?? null }),
  previewEngineTrashRestore: (trashId: string) =>
    invoke<EngineTrashRestorePreview>("engine_trash_restore_preview", { trashId }),
  restoreEngineTrash: (request: {
    trashId: string;
    strategy: "safe" | "exchange";
    expectedPreviewFingerprint: string;
  }) => invoke<EngineTrashRestoreResult>("engine_trash_restore", { request }),
  previewEngineTrashPurge: (trashId: string) =>
    invoke<EngineTrashPurgePreview>("engine_trash_purge_preview", { trashId }),
  purgeEngineTrash: (request: {
    trashId: string;
    confirmationText: string;
    expectedPreviewFingerprint: string;
  }) => invoke<void>("engine_trash_purge", { request }),
  claudeStatus: () => invoke<ClaudeStatus>("claude_status"),
  claudeConfig: () => invoke<ClaudeConfig>("claude_config"),
  readClaudeProviderProfiles: () => invoke<ClaudeProviderStore>("claude_provider_profiles_read"),
  writeClaudeProviderProfiles: (store: ClaudeProviderStore) =>
    invoke<ClaudeProviderStore>("claude_provider_profiles_write", { store }),
  readClaudeSettings: (scope: "global" | "project", workspaceRoot?: string | null) =>
    invoke<ClaudeSettingsFile>("claude_settings_read", {
      scope,
      workspaceRoot: workspaceRoot ?? null,
    }),
  writeClaudeSettings: (
    scope: "global" | "project",
    content: string,
    workspaceRoot?: string | null,
  ) =>
    invoke<ClaudeSettingsFile>("claude_settings_write", {
      scope,
      workspaceRoot: workspaceRoot ?? null,
      content,
    }),
  listClaudePlugins: (available = false) =>
    invoke<unknown>("claude_plugins_list", { available }),
  runClaudePluginAction: (
    action: "enable" | "disable" | "install" | "uninstall" | "update",
    plugin: string,
  ) => invoke<{ action: string; plugin: string; output: string }>("claude_plugin_action", { action, plugin }),
  workspaceRoot: () => invoke<string>("workspace_root"),
  pickWorkspace: () => invoke<string | null>("project_pick"),
  openWorkspace: (path: string) => invoke<string>("project_open", { path }),
  scanSessions: (workspaceRoot: string, force = false) =>
    invoke<SessionSummary[]>("session_scan", { workspaceRoot, force }),
  getSession: (sessionKey: string) =>
    invoke<SessionDetail>("session_get", { sessionKey }),
  refreshSession: (sessionKey: string) =>
    invoke<SessionDetail>("session_refresh", { sessionKey }),
  renameSession: (request: {
    sessionKey: string;
    expectedFileFingerprint: string;
    title: string;
  }) => invoke<SessionRenameResult>("session_rename", { request }),
  setSessionArchived: (request: { sessionKey: string; archived: boolean }) =>
    invoke<SessionDetail>("session_set_archived", { request }),
  editTranscript: (request: {
    sessionKey: string;
    target: EditTarget;
    expectedFileFingerprint: string;
    newText: string;
    allowAssistantText: boolean;
  }) => invoke<TranscriptEditResult>("transcript_edit", { request }),
  listTranscriptSnapshots: (sessionKey: string) =>
    invoke<BackupSnapshot[]>("transcript_snapshots_list", { sessionKey }),
  listTranscriptAudit: (sessionKey: string) =>
    invoke<TranscriptAuditEntry[]>("transcript_audit_list", { sessionKey }),
  listTranscriptRecovery: () =>
    invoke<EditRecoveryItem[]>("transcript_recovery_list"),
  resolveTranscriptRecovery: (request: {
    transactionId: string;
    action: "keep_current" | "restore_backup";
  }) => invoke<EditRecoveryResolveResult>("transcript_recovery_resolve", { request }),
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
  previewTrashDelete: (sessionKey: string) =>
    invoke<TrashDeletePreview>("trash_delete_preview", { sessionKey }),
  moveSessionToTrash: (request: {
    sessionKey: string;
    expectedPreviewFingerprint: string;
  }) => invoke<TrashItemSummary>("trash_move", { request }),
  listTrash: () => invoke<TrashItemSummary[]>("trash_list"),
  previewTrashRestore: (trashId: string) =>
    invoke<TrashRestorePreview>("trash_restore_preview", { trashId }),
  restoreTrash: (request: {
    trashId: string;
    strategy: "safe" | "exchange";
    expectedPreviewFingerprint: string;
  }) => invoke<TrashRestoreResult>("trash_restore", { request }),
  previewTrashPurge: (trashId: string) =>
    invoke<TrashPurgePreview>("trash_purge_preview", { trashId }),
  purgeTrash: (request: {
    trashId: string;
    confirmationText: string;
    expectedPreviewFingerprint: string;
  }) => invoke<void>("trash_purge", { request }),
};
