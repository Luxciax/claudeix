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
  providerId: string | null;
  model: string | null;
  loaded: boolean;
  setModel: (model: string | null, providerId?: string | null) => void;
  load: (workspaceRoot?: string | null) => Promise<void>;
}

let generation = 0;

export const useClaudeModel = create<ClaudeModelState>((set) => ({
  providerId: null,
  model: null,
  loaded: false,
  setModel: (model, providerId = null) => { ++generation; set({ model, providerId }); },
  load: async (workspaceRoot) => {
    const current = ++generation;
    try {
      const [global, project] = await Promise.all([
        api.readClaudeSettings("global", workspaceRoot),
        workspaceRoot
          ? api.readClaudeSettings("project", workspaceRoot).catch(() => null)
          : Promise.resolve(null),
      ]);
      if (current !== generation) return;
      set({
        providerId: null,
        model: modelFromContent(project?.content ?? "") ?? modelFromContent(global.content),
        loaded: true,
      });
    } catch {
      if (current !== generation) return;
      set({ loaded: true });
    }
  },
}));
