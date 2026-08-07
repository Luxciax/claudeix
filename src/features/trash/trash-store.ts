import { create } from "zustand";

import { api } from "@/lib/tauri-api";
import type {
  TrashItemSummary,
  TrashPurgePreview,
  TrashRestorePreview,
  TrashRestoreResult,
} from "./types";

type TrashState = {
  items: TrashItemSummary[];
  restorePreview: TrashRestorePreview | null;
  purgePreview: TrashPurgePreview | null;
  loading: boolean;
  mutating: boolean;
  error: string | null;
  load: () => Promise<TrashItemSummary[]>;
  previewRestore: (trashId: string) => Promise<TrashRestorePreview | null>;
  restore: (
    trashId: string,
    strategy: "safe" | "exchange",
    expectedPreviewFingerprint: string,
  ) => Promise<TrashRestoreResult | null>;
  previewPurge: (trashId: string) => Promise<TrashPurgePreview | null>;
  purge: (
    trashId: string,
    confirmationText: string,
    expectedPreviewFingerprint: string,
  ) => Promise<boolean>;
  clearPreviews: () => void;
};

export const useTrashStore = create<TrashState>((set, get) => ({
  items: [],
  restorePreview: null,
  purgePreview: null,
  loading: false,
  mutating: false,
  error: null,
  load: async () => {
    set({ loading: true, error: null });
    try {
      const items = await api.listTrash();
      set({ items, loading: false });
      return items;
    } catch (error) {
      set({ error: String(error), loading: false });
      return [];
    }
  },
  previewRestore: async (trashId) => {
    set({ error: null, purgePreview: null });
    try {
      const restorePreview = await api.previewTrashRestore(trashId);
      set({ restorePreview });
      return restorePreview;
    } catch (error) {
      set({ error: String(error), restorePreview: null });
      return null;
    }
  },
  restore: async (trashId, strategy, expectedPreviewFingerprint) => {
    set({ mutating: true, error: null });
    try {
      const result = await api.restoreTrash({
        trashId,
        strategy,
        expectedPreviewFingerprint,
      });
      await get().load();
      const restorePreview = await api.previewTrashRestore(trashId).catch(() => null);
      set({ restorePreview, mutating: false });
      return result;
    } catch (error) {
      set({ error: String(error), mutating: false });
      return null;
    }
  },
  previewPurge: async (trashId) => {
    set({ error: null });
    try {
      const purgePreview = await api.previewTrashPurge(trashId);
      set({ purgePreview });
      return purgePreview;
    } catch (error) {
      set({ error: String(error), purgePreview: null });
      return null;
    }
  },
  purge: async (trashId, confirmationText, expectedPreviewFingerprint) => {
    set({ mutating: true, error: null });
    try {
      await api.purgeTrash({ trashId, confirmationText, expectedPreviewFingerprint });
      await get().load();
      set({ restorePreview: null, purgePreview: null, mutating: false });
      return true;
    } catch (error) {
      set({ error: String(error), mutating: false });
      return false;
    }
  },
  clearPreviews: () => set({ restorePreview: null, purgePreview: null, error: null }),
}));
