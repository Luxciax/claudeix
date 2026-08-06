import { create } from "zustand";

import { api } from "@/lib/tauri-api";
import type { SessionSummary } from "./types";

type SessionState = {
  workspaceRoot: string;
  sessions: SessionSummary[];
  activeKey: string | null;
  loading: boolean;
  error: string | null;
  setActiveKey: (activeKey: string | null) => void;
  scan: (workspaceRoot: string, force?: boolean) => Promise<SessionSummary[]>;
};

export const useSessionStore = create<SessionState>((set, get) => ({
  workspaceRoot: "",
  sessions: [],
  activeKey: null,
  loading: false,
  error: null,
  setActiveKey: (activeKey) => set({ activeKey }),
  scan: async (workspaceRoot, force = false) => {
    if (!workspaceRoot) {
      set({ workspaceRoot: "", sessions: [], activeKey: null, error: null });
      return [];
    }
    set({ workspaceRoot, loading: true, error: null });
    try {
      const sessions = await api.scanSessions(workspaceRoot, force);
      const activeKey = get().activeKey;
      set({
        sessions,
        loading: false,
        activeKey:
          activeKey && sessions.some((session) => session.key === activeKey)
            ? activeKey
            : null,
      });
      return sessions;
    } catch (error) {
      set({ loading: false, error: String(error) });
      return [];
    }
  },
}));
