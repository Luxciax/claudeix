export type ClaudeStatus = {
  installed: boolean;
  version: string;
  executablePath: string | null;
};

export type ClaudeConfig = {
  configDir: string;
  projectsDir: string;
  projectsDirExists: boolean;
  source: string;
};

export type SessionSummary = {
  key: string;
  sessionId: string;
  title: string;
  preview: string;
  workspaceRoot: string;
  sourcePath: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
  model: string | null;
  archived: boolean;
  favorite: boolean;
  trashed: boolean;
};

export type EditTarget = {
  lineIndex: number;
  recordUuid: string | null;
  contentIndex: number | null;
  field: string;
  expectedRecordHash: string;
};

export type ContentBlock =
  | { type: "text"; text: string; editable: boolean }
  | {
      type: "thinking";
      text: string;
      editable: boolean;
      redacted: boolean;
    }
  | {
      type: "tool_use";
      id: string;
      name: string;
      input: unknown;
    }
  | {
      type: "tool_result";
      tool_use_id: string;
      output: unknown;
      is_error: boolean;
    }
  | { type: "image"; media_type: string | null }
  | { type: "system_event"; name: string; text: string }
  | { type: "unknown"; kind: string; raw: unknown };

export type TranscriptRecord = {
  lineIndex: number;
  rawType: string;
  uuid: string | null;
  parentUuid: string | null;
  timestamp: string | null;
  role: string | null;
  blocks: ContentBlock[];
  editableTargets: EditTarget[];
  rawHash: string;
};

export type ParseDiagnostic = {
  lineIndex: number;
  message: string;
};

export type SessionDetail = {
  summary: SessionSummary;
  records: TranscriptRecord[];
  diagnostics: ParseDiagnostic[];
  fileFingerprint: string;
  hasBranches: boolean;
};

export type BackupSnapshot = {
  id: string;
  sessionId: string;
  sourcePath: string;
  backupPath: string;
  createdAt: number;
  sourceHash: string;
  fileSize: number;
  reason: string;
  target: EditTarget | null;
  oldTextHash: string | null;
  newTextHash: string | null;
};

export type TranscriptEditResult = {
  detail: SessionDetail;
  snapshot: BackupSnapshot;
};

export type TranscriptRestoreResult = {
  detail: SessionDetail;
  currentSnapshot: BackupSnapshot;
  restoredSnapshotId: string;
};

export type TranscriptAuditEntry = {
  id: number;
  transactionId: string;
  sessionId: string;
  sourcePath: string;
  operation: string;
  lineIndex: number | null;
  contentIndex: number | null;
  fieldName: string | null;
  oldContent: string | null;
  newContent: string | null;
  backupPath: string;
  sourceHashBefore: string;
  sourceHashAfter: string;
  createdAt: number;
};

export type EditRecoveryItem = {
  transactionId: string;
  sessionId: string;
  sourcePath: string;
  operation: string;
  status: string;
  backupPath: string | null;
  sourceHashBefore: string;
  sourceHashAfter: string;
  createdAt: number;
  detectedState: string;
  backupValid: boolean;
  canKeepCurrent: boolean;
  canRestoreBackup: boolean;
};

export type EditRecoveryResolveResult = {
  transactionId: string;
  status: string;
  sourcePath: string;
};
