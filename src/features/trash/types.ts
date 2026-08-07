export type TrashFilePreview = {
  originalPath: string;
  purpose: string;
  size: number;
  sha256: string;
};

export type TrashDeletePreview = {
  sessionKey: string;
  sessionId: string;
  title: string;
  workspaceRoot: string;
  configDir: string;
  files: TrashFilePreview[];
  totalFiles: number;
  totalSize: number;
  previewFingerprint: string;
  runtimeActive: boolean;
  externalRuntimeActive: boolean;
  externalRuntimePid: number | null;
};

export type TrashItemSummary = {
  trashId: string;
  sessionId: string;
  title: string;
  workspaceRoot: string;
  configDir: string;
  deletedAt: number;
  restoredAt: number | null;
  status: string;
  totalFiles: number;
  totalSize: number;
};

export type TrashRestoreFileState = {
  originalPath: string;
  payloadPath: string;
  purpose: string;
  size: number;
  sha256: string;
  state:
    | "free"
    | "same"
    | "different"
    | "already_restored"
    | "corrupt_payload"
    | "missing_payload"
    | string;
  currentSha256: string | null;
};

export type TrashRestorePreview = {
  trashId: string;
  sessionId: string;
  title: string;
  files: TrashRestoreFileState[];
  canRestoreSafely: boolean;
  hasConflicts: boolean;
  corrupt: boolean;
  previewFingerprint: string;
};

export type TrashRestoreResult = {
  trashId: string;
  status: string;
  exchangeTrashId: string | null;
};

export type TrashPurgeFile = {
  path: string;
  size: number;
  purpose: string;
};

export type TrashPurgePreview = {
  trashId: string;
  sessionId: string;
  title: string;
  files: TrashPurgeFile[];
  totalFiles: number;
  totalSize: number;
  confirmationText: string;
  previewFingerprint: string;
};
