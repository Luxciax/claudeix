"use client";

import { create } from "zustand";

import type { EditTarget } from "@/features/sessions/types";
import type { NativeSessionDetail, NativeSessionSummary } from "./types";
import { api } from "@/lib/tauri-api";
import { getPiClient } from "@/lib/pi/client";
import { useChat, type ChatMessage, type ChatToolCall } from "@/lib/pi/chat";
import { useExtUi } from "@/lib/pi/ext-ui";
import { initAgentBridge } from "@/lib/pi/agent-bridge";
import { usePi } from "@/lib/pi/store";

type PiNativeStore = {
  workspaceRoot: string;
  sessions: NativeSessionSummary[];
  activeKey: string | null;
  detail: NativeSessionDetail | null;
  loading: boolean;
  mutating: boolean;
  error: string | null;
  initialized: boolean;
  boot: (workspaceRoot: string) => Promise<void>;
  refresh: (workspaceRoot?: string) => Promise<NativeSessionSummary[]>;
  select: (sessionKey: string, workspaceRoot?: string) => Promise<void>;
  newSession: (workspaceRoot?: string) => Promise<void>;
  edit: (target: EditTarget, newText: string) => Promise<boolean>;
  syncFromRuntime: () => Promise<void>;
};

let runtimeHooksBound = false;

function normalizePath(value: string): string {
  return value.replace(/\\/g, "/").replace(/\/+$/, "").toLowerCase();
}

function textFromBlocks(record: NativeSessionDetail["records"][number]): string {
  return record.blocks
    .filter((block): block is Extract<(typeof record.blocks)[number], { type: "text" }> => block.type === "text")
    .map((block) => block.text)
    .filter(Boolean)
    .join("\n\n");
}

function thinkingFromBlocks(record: NativeSessionDetail["records"][number]): string {
  return record.blocks
    .filter(
      (block): block is Extract<(typeof record.blocks)[number], { type: "thinking" }> =>
        block.type === "thinking" && !block.redacted,
    )
    .map((block) => block.text)
    .filter(Boolean)
    .join("\n\n");
}

function nativeMessages(detail: NativeSessionDetail): ChatMessage[] {
  const toolResults = new Map<string, { output: unknown; isError: boolean }>();
  for (const record of detail.records) {
    for (const block of record.blocks) {
      if (block.type === "tool_result") {
        toolResults.set(block.tool_use_id, { output: block.output, isError: block.is_error });
      }
    }
  }

  return detail.records.flatMap((record): ChatMessage[] => {
    if (record.role !== "user" && record.role !== "assistant") return [];
    const text = textFromBlocks(record);
    const thinking = thinkingFromBlocks(record);
    const tools: ChatToolCall[] = record.blocks
      .filter(
        (block): block is Extract<(typeof record.blocks)[number], { type: "tool_use" }> =>
          block.type === "tool_use",
      )
      .map((block) => ({
        id: block.id,
        name: block.name,
        args: block.input,
        status: toolResults.get(block.id)?.isError ? "error" : "done",
      }));
    if (!text && !thinking && tools.length === 0) return [];
    return [
      {
        id: record.uuid ?? `pi-line-${record.lineIndex}`,
        role: record.role,
        text,
        thinking,
        tools,
        streaming: false,
      },
    ];
  });
}

function loadNativeConversation(detail: NativeSessionDetail | null) {
  useChat.getState().load(detail ? nativeMessages(detail) : []);
}

export const usePiNativeStore = create<PiNativeStore>((set, get) => ({
  workspaceRoot: "",
  sessions: [],
  activeKey: null,
  detail: null,
  loading: false,
  mutating: false,
  error: null,
  initialized: false,

  boot: async (workspaceRoot) => {
    if (!workspaceRoot) return;
    useChat.getState().init();
    useExtUi.getState().init();
    initAgentBridge();

    if (!runtimeHooksBound) {
      runtimeHooksBound = true;
      const client = getPiClient();
      const sync = () => {
        window.setTimeout(() => void usePiNativeStore.getState().syncFromRuntime(), 80);
      };
      client.on("agent_end", sync);
      client.on("agent_settled", sync);
    }

    set({ workspaceRoot, initialized: true, error: null });
    const sessions = await get().refresh(workspaceRoot);
    const current = get().activeKey;
    const selected = current && sessions.some((session) => session.sessionKey === current)
      ? current
      : sessions[0]?.sessionKey ?? null;
    if (selected) {
      await get().select(selected, workspaceRoot);
    } else {
      loadNativeConversation(null);
      if (usePi.getState().status === "disconnected") {
        await usePi.getState().connect({ cwd: workspaceRoot });
      }
    }
  },

  refresh: async (workspaceRoot) => {
    const root = workspaceRoot ?? get().workspaceRoot;
    if (!root) return [];
    set({ loading: true, error: null, workspaceRoot: root });
    try {
      const sessions = await api.scanEngineSessions("pi", root);
      set({ sessions, loading: false });
      return sessions;
    } catch (error) {
      set({ loading: false, error: String(error) });
      return [];
    }
  },

  select: async (sessionKey, workspaceRoot) => {
    const root = workspaceRoot ?? get().workspaceRoot;
    if (!root || useChat.getState().streaming) return;
    set({ loading: true, error: null, workspaceRoot: root });
    try {
      const detail = await api.getEngineSession("pi", sessionKey, root);
      set({ activeKey: sessionKey, detail, loading: false });
      loadNativeConversation(detail);
      await usePi.getState().restart(root, detail.summary.sourcePath);
    } catch (error) {
      set({ loading: false, error: String(error) });
    }
  },

  newSession: async (workspaceRoot) => {
    const root = workspaceRoot ?? get().workspaceRoot;
    if (!root || useChat.getState().streaming) return;
    set({ activeKey: null, detail: null, error: null, workspaceRoot: root });
    loadNativeConversation(null);
    try {
      if (usePi.getState().status === "disconnected") {
        await usePi.getState().connect({ cwd: root });
      }
      const response = await getPiClient().request({ type: "new_session" });
      if (!response.success) throw new Error(response.error ?? "Pi new_session failed");
    } catch (error) {
      set({ error: String(error) });
    }
  },

  edit: async (target, newText) => {
    const { detail, activeKey, workspaceRoot } = get();
    if (!detail || !activeKey || !workspaceRoot || useChat.getState().streaming) return false;
    set({ mutating: true, error: null });
    try {
      // The RPC process keeps the native session open even while idle. Stop it
      // before source mutation so the same external-writer guard used for truly
      // external Pi processes remains strict instead of learning an unsafe exception.
      if (usePi.getState().status !== "disconnected") {
        await getPiClient().stop();
        usePi.setState({ status: "disconnected" });
      }
      const result = await api.editEngineMessage({
        engineId: "pi",
        sessionKey: activeKey,
        workspaceRoot,
        expectedSourceRevision: detail.sourceRevision,
        target,
        newText,
      });
      set({ detail: result.detail, mutating: false });
      loadNativeConversation(result.detail);
      await usePi.getState().connect({
        cwd: workspaceRoot,
        resumePath: result.detail.summary.sourcePath,
      });
      return true;
    } catch (error) {
      set({ mutating: false, error: String(error) });
      if (usePi.getState().status === "disconnected") {
        void usePi.getState().connect({
          cwd: workspaceRoot,
          resumePath: detail.summary.sourcePath,
        });
      }
      return false;
    }
  },

  syncFromRuntime: async () => {
    const root = get().workspaceRoot;
    if (!root || useChat.getState().streaming) return;
    try {
      const state = await getPiClient().request<{
        sessionFile?: string;
        sessionPath?: string;
        sessionId?: string;
      }>({ type: "get_state" }, 30_000);
      const sessionPath = state.data?.sessionFile ?? state.data?.sessionPath ?? "";
      const sessionId = state.data?.sessionId ?? "";
      const sessions = await api.scanEngineSessions("pi", root);
      const pathKey = normalizePath(sessionPath);
      const active = sessions.find(
        (session) =>
          (pathKey && normalizePath(session.sourcePath) === pathKey) ||
          (sessionId && session.nativeSessionId === sessionId),
      );
      if (!active) {
        set({ sessions });
        return;
      }
      const detail = await api.getEngineSession("pi", active.sessionKey, root);
      set({ sessions, activeKey: active.sessionKey, detail, error: null });
      loadNativeConversation(detail);
    } catch (error) {
      set({ error: String(error) });
    }
  },
}));
