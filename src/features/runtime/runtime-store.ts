import { listen } from "@tauri-apps/api/event";
import { create } from "zustand";

import { api } from "@/lib/tauri-api";
import type {
  RuntimeEvent,
  RuntimeSnapshot,
  RuntimeTimelineItem,
  RuntimeUsage,
} from "./types";

type StartRequest = {
  workspaceRoot: string;
  sessionKey: string | null;
  prompt: string;
  permissionMode: string;
  model?: string | null;
  effort?: string | null;
};

type RuntimeState = {
  snapshot: RuntimeSnapshot | null;
  timeline: RuntimeTimelineItem[];
  usage: RuntimeUsage;
  completionVersion: number;
  bound: boolean;
  bindEvents: () => Promise<() => void>;
  start: (request: StartRequest) => Promise<RuntimeSnapshot | null>;
  stop: () => Promise<void>;
  clearTimeline: () => void;
  reset: () => void;
};

const emptyUsage: RuntimeUsage = {
  inputTokens: null,
  outputTokens: null,
  costUsd: null,
};

function belongsToCurrent(event: RuntimeEvent, snapshot: RuntimeSnapshot | null): boolean {
  return !snapshot || event.runtimeId === snapshot.runtimeId;
}

export const useRuntimeStore = create<RuntimeState>((set, get) => ({
  snapshot: null,
  timeline: [],
  usage: emptyUsage,
  completionVersion: 0,
  bound: false,
  bindEvents: async () => {
    if (get().bound) return () => undefined;
    set({ bound: true });
    const unlisten = await listen<RuntimeEvent>("claudeix://runtime-event", ({ payload }) => {
      if (!belongsToCurrent(payload, get().snapshot) && payload.type !== "runtime_started") {
        return;
      }
      switch (payload.type) {
        case "runtime_started":
          set((state) => ({
            snapshot: {
              runtimeId: payload.runtimeId,
              sessionKey: payload.sessionKey,
              sessionId: state.snapshot?.sessionId ?? null,
              workspaceRoot: state.snapshot?.workspaceRoot ?? "",
              status: "running",
              pid: payload.pid,
              startedAt: state.snapshot?.startedAt ?? Date.now(),
              exitCode: null,
              error: null,
            },
          }));
          break;
        case "session_identified":
          set((state) => ({
            snapshot: state.snapshot
              ? {
                  ...state.snapshot,
                  sessionId: payload.sessionId,
                  sessionKey: payload.sessionKey ?? state.snapshot.sessionKey,
                  status: "running",
                }
              : null,
          }));
          break;
        case "text_delta":
          set((state) => {
            const last = state.timeline[state.timeline.length - 1];
            if (last?.type === "assistant" && last.messageId === payload.messageId) {
              return {
                timeline: [
                  ...state.timeline.slice(0, -1),
                  { ...last, text: last.text + payload.text },
                ],
              };
            }
            return {
              timeline: [
                ...state.timeline,
                {
                  id: `assistant-${payload.messageId}-${state.timeline.length}`,
                  type: "assistant",
                  messageId: payload.messageId,
                  text: payload.text,
                },
              ],
            };
          });
          break;
        case "tool_started":
          set((state) => {
            const existing = state.timeline.findIndex(
              (item) => item.type === "tool" && item.toolUseId === payload.toolUseId,
            );
            const item: RuntimeTimelineItem = {
              id: `tool-${payload.toolUseId}`,
              type: "tool",
              toolUseId: payload.toolUseId,
              name: payload.name,
              input: payload.input,
              output: null,
              status: "running",
            };
            if (existing < 0) return { timeline: [...state.timeline, item] };
            const timeline = [...state.timeline];
            timeline[existing] = item;
            return { timeline };
          });
          break;
        case "tool_finished":
          set((state) => {
            const existing = state.timeline.findIndex(
              (item) => item.type === "tool" && item.toolUseId === payload.toolUseId,
            );
            if (existing < 0) {
              return {
                timeline: [
                  ...state.timeline,
                  {
                    id: `tool-${payload.toolUseId}`,
                    type: "tool",
                    toolUseId: payload.toolUseId,
                    name: "Tool",
                    input: null,
                    output: payload.output,
                    status: payload.isError ? "failed" : "completed",
                  },
                ],
              };
            }
            const timeline = [...state.timeline];
            const item = timeline[existing];
            if (item.type === "tool") {
              timeline[existing] = {
                ...item,
                output: payload.output,
                status: payload.isError ? "failed" : "completed",
              };
            }
            return { timeline };
          });
          break;
        case "usage":
          set({
            usage: {
              inputTokens: payload.inputTokens,
              outputTokens: payload.outputTokens,
              costUsd: payload.costUsd,
            },
          });
          break;
        case "stderr":
          set((state) => ({
            timeline: [
              ...state.timeline,
              {
                id: `stderr-${state.timeline.length}-${Date.now()}`,
                type: "system",
                text: payload.text,
                tone: "error",
              },
            ],
          }));
          break;
        case "runtime_completed":
          set((state) => ({
            snapshot: state.snapshot
              ? {
                  ...state.snapshot,
                  status: "completed",
                  exitCode: payload.exitCode,
                  sessionId: payload.sessionId ?? state.snapshot.sessionId,
                  sessionKey: payload.sessionKey ?? state.snapshot.sessionKey,
                }
              : null,
            completionVersion: state.completionVersion + 1,
          }));
          break;
        case "runtime_failed":
          set((state) => ({
            snapshot: state.snapshot
              ? {
                  ...state.snapshot,
                  status: "failed",
                  error: payload.message,
                  sessionId: payload.sessionId ?? state.snapshot.sessionId,
                  sessionKey: payload.sessionKey ?? state.snapshot.sessionKey,
                }
              : null,
            timeline: [
              ...state.timeline,
              {
                id: `runtime-failed-${Date.now()}`,
                type: "system",
                text: payload.message,
                tone: "error",
              },
            ],
            completionVersion: state.completionVersion + 1,
          }));
          break;
      }
    });
    return () => {
      unlisten();
      set({ bound: false });
    };
  },
  start: async (request) => {
    set({
      snapshot: null,
      usage: emptyUsage,
      timeline: [{ id: `user-${Date.now()}`, type: "user", text: request.prompt }],
    });
    try {
      const snapshot = await api.startRuntime(request);
      set((state) => ({
        snapshot: {
          ...snapshot,
          status:
            state.snapshot?.runtimeId === snapshot.runtimeId
              ? state.snapshot.status
              : snapshot.status,
        },
      }));
      return snapshot;
    } catch (error) {
      set((state) => ({
        snapshot: null,
        timeline: [
          ...state.timeline,
          {
            id: `start-error-${Date.now()}`,
            type: "system",
            text: String(error),
            tone: "error",
          },
        ],
      }));
      return null;
    }
  },
  stop: async () => {
    const runtimeId = get().snapshot?.runtimeId;
    if (!runtimeId) return;
    const snapshot = await api.stopRuntime(runtimeId);
    set({ snapshot });
  },
  clearTimeline: () => set({ timeline: [], usage: emptyUsage }),
  reset: () => {
    const status = get().snapshot?.status;
    if (status === "starting" || status === "running" || status === "stopping") return;
    set({ snapshot: null, timeline: [], usage: emptyUsage });
  },
}));
