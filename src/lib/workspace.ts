"use client";

import { create } from "zustand";
import { isTauri } from "./pi/client";
import { flushActiveSession, useSessions } from "./pi/sessions";

export interface RecentProject {
  path: string;
  name: string;
  lastOpenedAt: number;
}

async function tauriInvoke<T>(cmd: string, args?: Record<string, unknown>) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(cmd, args);
}

interface WorkspaceStore {
  root: string | null;
  mock: boolean;
  initialized: boolean;
  switching: boolean;
  recents: RecentProject[];
  loadError: string | null;

  init: () => Promise<void>;
  loadRecents: () => Promise<void>;
  openProject: (path: string) => Promise<void>;
  pickProject: () => Promise<void>;
  removeRecent: (path: string) => Promise<void>;
}

/** Project selection and Pi cwd lifecycle, without an embedded file editor. */
export const useWorkspace = create<WorkspaceStore>((set, get) => ({
  root: null,
  mock: !isTauri(),
  initialized: false,
  switching: false,
  recents: [],
  loadError: null,

  init: async () => {
    if (get().initialized) return;
    if (!isTauri()) {
      set({ root: "", mock: true, initialized: true });
      return;
    }

    try {
      const root = await tauriInvoke<string>("workspace_root");
      set({ root, mock: false, initialized: true, loadError: null });
    } catch (error) {
      set({
        initialized: true,
        loadError: error instanceof Error ? error.message : String(error),
      });
    }
    void get().loadRecents();
  },

  loadRecents: async () => {
    if (get().mock) return;
    try {
      const recents = await tauriInvoke<RecentProject[]>("projects_recent");
      set({ recents });
    } catch {
      // Recent projects are cosmetic; keep the existing list on failure.
    }
  },

  openProject: async (path) => {
    const { mock, switching } = get();
    if (mock || switching) return;

    set({ switching: true, loadError: null });
    try {
      const root = await tauriInvoke<string>("project_open", { path });
      await get().loadRecents();
      if (root === get().root) return;

      await flushActiveSession();
      set({ root });

      await useSessions.getState().switchProject(root);
    } catch (error) {
      set({ loadError: error instanceof Error ? error.message : String(error) });
    } finally {
      set({ switching: false });
    }
  },

  pickProject: async () => {
    if (get().mock || get().switching) return;
    try {
      const picked = await tauriInvoke<string | null>("project_pick");
      if (picked) await get().openProject(picked);
    } catch (error) {
      set({ loadError: error instanceof Error ? error.message : String(error) });
    }
  },

  removeRecent: async (path) => {
    if (get().mock) return;
    try {
      const recents = await tauriInvoke<RecentProject[]>("project_remove_recent", { path });
      set({ recents });
    } catch {
      // Keep the current list on failure.
    }
  },
}));

export function projectName(root: string | null): string {
  if (!root) return "";
  return root.split("/").filter(Boolean).pop() ?? root;
}
