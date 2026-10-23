import type { TranscriptRecord } from "@/features/sessions/types";

export type NativeStorageKind = "jsonl" | "json" | "sqlite" | "directoryTree" | "custom";

export type EngineCapabilities = {
  readSessions: boolean;
  writeSessions: boolean;
  editMessage: boolean;
  deleteMessage: boolean;
  truncateSession: boolean;
  deleteSession: boolean;
  restoreSession: boolean;
  createSession: boolean;
  resumeSession: boolean;
  stream: boolean;
  interrupt: boolean;
  permissions: boolean;
  modelDiscovery: boolean;
};

export type EngineDescriptor = {
  id: string;
  displayName: string;
  storage: NativeStorageKind;
  capabilities: EngineCapabilities;
};

export type NativeSessionSummary = {
  engineId: string;
  sessionKey: string;
  nativeSessionId: string;
  title: string;
  preview: string;
  workspaceRoot: string;
  sourcePath: string;
  updatedAt: number;
};

export type NativeSessionDetail = {
  summary: NativeSessionSummary;
  records: TranscriptRecord[];
  sourceRevision: string;
};

export type EngineMutationSnapshot = {
  transactionId: string;
  engineId: string;
  nativeSessionId: string;
  sourcePath: string;
  backupPath: string;
  createdAt: number;
  sourceRevisionBefore: string;
  sourceRevisionAfter: string;
};

export type EngineEditResult = {
  detail: NativeSessionDetail;
  snapshot: EngineMutationSnapshot;
};

export type EngineRestoreResult = {
  detail: NativeSessionDetail;
  snapshot: EngineMutationSnapshot;
  restoredTransactionId: string;
};

export type EngineMutationRecoveryItem = {
  transactionId: string;
  engineId: string;
  sessionKey: string;
  nativeSessionId: string;
  sourcePath: string;
  operation: string;
  status: string;
  backupPath: string | null;
  sourceRevisionBefore: string;
  sourceRevisionAfter: string;
  createdAt: number;
  detectedState: "source_missing" | "source_before" | "source_after" | "source_unknown";
  backupValid: boolean;
  canKeepCurrent: boolean;
  canRestoreBackup: boolean;
};

export type EngineMutationRecoveryResolveResult = {
  transactionId: string;
  status: string;
  sourcePath: string;
  restoreTransactionId: string | null;
};

export type EngineTrashFilePreview = {
  originalPath: string;
  purpose: string;
  size: number;
  sha256: string;
};

export type EngineTrashDeletePreview = {
  engineId: string;
  sessionKey: string;
  nativeSessionId: string;
  title: string;
  workspaceRoot: string;
  files: EngineTrashFilePreview[];
  totalFiles: number;
  totalSize: number;
  previewFingerprint: string;
  runtimeActive: boolean;
  externalRuntimeActive: boolean;
  externalRuntimePid: number | null;
};

export type EngineTrashItemSummary = {
  trashId: string;
  engineId: string;
  sessionKey: string;
  nativeSessionId: string;
  title: string;
  workspaceRoot: string;
  deletedAt: number;
  restoredAt: number | null;
  status: string;
  totalFiles: number;
  totalSize: number;
};

export type EngineTrashRestoreFileState = {
  originalPath: string;
  payloadPath: string;
  purpose: string;
  size: number;
  sha256: string;
  state: string;
  currentSha256: string | null;
};

export type EngineTrashRestorePreview = {
  trashId: string;
  engineId: string;
  nativeSessionId: string;
  title: string;
  files: EngineTrashRestoreFileState[];
  canRestoreSafely: boolean;
  hasConflicts: boolean;
  corrupt: boolean;
  previewFingerprint: string;
};

export type EngineTrashRestoreResult = {
  trashId: string;
  status: string;
  exchangeTrashId: string | null;
};

export type EngineTrashPurgeFile = {
  path: string;
  size: number;
  purpose: string;
};

export type EngineTrashPurgePreview = {
  trashId: string;
  engineId: string;
  nativeSessionId: string;
  title: string;
  files: EngineTrashPurgeFile[];
  totalFiles: number;
  totalSize: number;
  confirmationText: string;
  previewFingerprint: string;
};
