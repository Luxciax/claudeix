"use client";

import { create } from "zustand";
import { api } from "@/lib/tauri-api";

function modelFromContent(content: string): string | null {
  try {
    const value = JSON.parse(content) as unknown;
    if (!value || typeof value !== "object" || Array.isArray(value)) return null;
    const model = (value as Record<string, unknown>).model;
    return typeof model === "string" && model.trim() ? model.trim() : null;
  } catch {
    return null;
  }
}

interface ClaudeModelState {
  model: string | null;
  loaded: boolean;
  setModel: (model: string | null) => void;
  load: (workspaceRoot?: string | null) => Promise<void>;
}

export const useClaudeModel = create<ClaudeModelState>((set) => ({
  model: null,
  loaded: false,
  setModel: (model) => set({ model }),
  load: async (workspaceRoot) => {
    try {
      const [global, project] = await Promise.all([
        api.readClaudeSettings("global", workspaceRoot),
        workspaceRoot
          ? api.readClaudeSettings("project", workspaceRoot).catch(() => null)
          : Promise.resolve(null),
      ]);
      set({
        model: modelFromContent(project?.content ?? "") ?? modelFromContent(global.content),
        loaded: true,
      });
    } catch {
      set({ loaded: true });
    }
  },
}));
