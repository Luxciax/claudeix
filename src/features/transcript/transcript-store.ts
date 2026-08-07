import { create } from "zustand";

import type {
  BackupSnapshot,
  EditTarget,
  SessionDetail,
  TranscriptAuditEntry,
  TranscriptEditResult,
  TranscriptRestoreResult,
} from "@/features/sessions/types";
import { api } from "@/lib/tauri-api";

type TranscriptState = {
  detail: SessionDetail | null;
  snapshots: BackupSnapshot[];
  auditEntries: TranscriptAuditEntry[];
  loading: boolean;
  mutating: boolean;
  error: string | null;
  mutationError: string | null;
  load: (sessionKey: string) => Promise<SessionDetail | null>;
  refresh: (sessionKey: string) => Promise<SessionDetail | null>;
  loadSnapshots: (sessionKey: string) => Promise<BackupSnapshot[]>;
  loadAudit: (sessionKey: string) => Promise<TranscriptAuditEntry[]>;
  edit: (
    sessionKey: string,
    target: EditTarget,
    expectedFileFingerprint: string,
    newText: string,
    allowAssistantText: boolean,
  ) => Promise<TranscriptEditResult | null>;
  restore: (
    sessionKey: string,
    snapshotId: string,
    expectedFileFingerprint: string,
  ) => Promise<TranscriptRestoreResult | null>;
  clear: () => void;
};

export const useTranscriptStore = create<TranscriptState>((set) => ({
  detail: null,
  snapshots: [],
  auditEntries: [],
  loading: false,
  mutating: false,
  error: null,
  mutationError: null,
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
  loadSnapshots: async (sessionKey) => {
    try {
      const snapshots = await api.listTranscriptSnapshots(sessionKey);
      set({ snapshots, mutationError: null });
      return snapshots;
    } catch (error) {
      set({ mutationError: String(error) });
      return [];
    }
  },
  loadAudit: async (sessionKey) => {
    try {
      const auditEntries = await api.listTranscriptAudit(sessionKey);
      set({ auditEntries, mutationError: null });
      return auditEntries;
    } catch (error) {
      set({ mutationError: String(error) });
      return [];
    }
  },
  edit: async (
    sessionKey,
    target,
    expectedFileFingerprint,
    newText,
    allowAssistantText,
  ) => {
    set({ mutating: true, mutationError: null });
    try {
      const result = await api.editTranscript({
        sessionKey,
        target,
        expectedFileFingerprint,
        newText,
        allowAssistantText,
      });
      set((state) => ({
        detail: result.detail,
        snapshots: [
          result.snapshot,
          ...state.snapshots.filter((snapshot) => snapshot.id !== result.snapshot.id),
        ],
        mutating: false,
      }));
      return result;
    } catch (error) {
      set({ mutating: false, mutationError: String(error) });
      return null;
    }
  },
  restore: async (sessionKey, snapshotId, expectedFileFingerprint) => {
    set({ mutating: true, mutationError: null });
    try {
      const result = await api.restoreTranscriptSnapshot({
        sessionKey,
        snapshotId,
        expectedFileFingerprint,
      });
      set((state) => ({
        detail: result.detail,
        snapshots: [
          result.currentSnapshot,
          ...state.snapshots.filter(
            (snapshot) => snapshot.id !== result.currentSnapshot.id,
          ),
        ],
        mutating: false,
      }));
      return result;
    } catch (error) {
      set({ mutating: false, mutationError: String(error) });
      return null;
    }
  },
  clear: () =>
    set({
      detail: null,
      snapshots: [],
      auditEntries: [],
      loading: false,
      mutating: false,
      error: null,
      mutationError: null,
    }),
}));
