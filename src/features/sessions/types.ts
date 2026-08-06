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
