export type RuntimeStatus =
  | "starting"
  | "running"
  | "waiting_permission"
  | "stopping"
  | "completed"
  | "failed";

export type RuntimeSnapshot = {
  runtimeId: string;
  sessionKey: string | null;
  sessionId: string | null;
  workspaceRoot: string;
  status: RuntimeStatus;
  pid: number;
  startedAt: number;
  exitCode: number | null;
  error: string | null;
};

export type ExternalRuntimeInfo = {
  active: boolean;
  pid: number | null;
  executablePath: string | null;
  matchKind: string | null;
  warning: string | null;
};

export type RuntimeEvent =
  | { type: "runtime_started"; runtimeId: string; sessionKey: string | null; pid: number }
  | {
      type: "session_identified";
      runtimeId: string;
      sessionId: string;
      sessionKey: string | null;
    }
  | { type: "text_delta"; runtimeId: string; messageId: string; text: string }
  | {
      type: "tool_started";
      runtimeId: string;
      toolUseId: string;
      name: string;
      input: unknown;
    }
  | {
      type: "tool_finished";
      runtimeId: string;
      toolUseId: string;
      output: unknown;
      isError: boolean;
    }
  | {
      type: "permission_requested";
      runtimeId: string;
      requestId: string;
      toolUseId: string | null;
      tool: string;
      input: unknown;
      suggestions: unknown;
      title: string | null;
      description: string | null;
      decisionReason: string | null;
    }
  | {
      type: "usage";
      runtimeId: string;
      inputTokens: number | null;
      outputTokens: number | null;
      costUsd: number | null;
    }
  | { type: "stderr"; runtimeId: string; text: string }
  | {
      type: "runtime_completed";
      runtimeId: string;
      exitCode: number | null;
      sessionId: string | null;
      sessionKey: string | null;
    }
  | {
      type: "runtime_failed";
      runtimeId: string;
      message: string;
      recoverable: boolean;
      sessionId: string | null;
      sessionKey: string | null;
    };

export type RuntimeTimelineItem =
  | { id: string; type: "user"; text: string }
  | { id: string; type: "assistant"; messageId: string; text: string }
  | { id: string; type: "system"; text: string; tone: "info" | "error" }
  | {
      id: string;
      type: "tool";
      toolUseId: string;
      name: string;
      input: unknown;
      output: unknown;
      status: "running" | "completed" | "failed";
    }
  | {
      id: string;
      type: "permission";
      requestId: string;
      toolUseId: string | null;
      tool: string;
      input: unknown;
      title: string | null;
      description: string | null;
      decisionReason: string | null;
      status: "pending" | "allowed" | "denied" | "failed";
      error: string | null;
    };

export type RuntimeUsage = {
  inputTokens: number | null;
  outputTokens: number | null;
  costUsd: number | null;
};
