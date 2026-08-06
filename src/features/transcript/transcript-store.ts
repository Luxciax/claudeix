import { create } from "zustand";

import type { SessionDetail } from "@/features/sessions/types";
import { api } from "@/lib/tauri-api";

type TranscriptState = {
  detail: SessionDetail | null;
  loading: boolean;
  error: string | null;
  load: (sessionKey: string) => Promise<SessionDetail | null>;
  refresh: (sessionKey: string) => Promise<SessionDetail | null>;
  clear: () => void;
};

export const useTranscriptStore = create<TranscriptState>((set) => ({
  detail: null,
  loading: false,
  error: null,
  load: async (sessionKey) => {
    set({ loading: true, error: null, detail: null });
    try {
      const detail = await api.getSession(sessionKey);
      set({ detail, loading: false });
      return detail;
    } catch (error) {
      set({ error: String(error), loading: false });
      return null;
    }
  },
  refresh: async (sessionKey) => {
    try {
      const detail = await api.refreshSession(sessionKey);
      set({ detail, error: null });
      return detail;
    } catch (error) {
      set({ error: String(error) });
      return null;
    }
  },
  clear: () => set({ detail: null, loading: false, error: null }),
}));
