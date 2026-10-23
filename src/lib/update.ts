"use client";

/**
 * Software-update state.
 * Claudeix delegates update checks to Rust (`update_check`), which reads semver
 * tags from the Claudeix GitHub repository. Automatic binary replacement is
 * deliberately disabled until the signed release pipeline is wired up.
 * In the browser preview the store runs a mock so the page is explorable.
 */

import { create } from "zustand";
import { isTauri } from "./pi/client";

import pkg from "../../package.json";
const REPO_URL = "https://github.com/Luxciax/claudeix";

/** Version from package.json — kept in sync automatically. */
export const APP_VERSION = pkg.version;

export interface UpdateInfo {
  configured: boolean;
  repoUrl: string;
  currentVersion: string;
  latestVersion: string | null;
  latestCommit: string | null;
  updateAvailable: boolean;
}

export type UpdatePhase =
  | "idle"
  | "checking"
  | "unconfigured"
  | "upToDate"
  | "available"
  | "applying"
  | "error";

/** Sentinel error meaning "install is desktop-only" in the browser preview. */
export const MOCK_APPLY_ERROR = "mock-preview";

interface UpdateState {
  phase: UpdatePhase;
  info: UpdateInfo | null;
  error: string | null;
  lastCheckedAt: number | null;
  mock: boolean;
  /** User closed the top-bar reminder — stays hidden until the next launch. */
  dismissed: boolean;
  check: () => Promise<void>;
  apply: () => Promise<void>;
  dismiss: () => void;
}

const MOCK_INFO: UpdateInfo = {
  configured: true,
  repoUrl: REPO_URL,
  currentVersion: APP_VERSION,
  latestVersion: "v0.2.3",
  latestCommit: "9f3ab12",
  updateAvailable: false,
};

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));

export const useUpdate = create<UpdateState>((set, get) => ({
  phase: "idle",
  info: null,
  error: null,
  lastCheckedAt: null,
  mock: !isTauri(),
  dismissed: false,

  check: async () => {
    const { phase } = get();
    if (phase === "checking" || phase === "applying") return;
    set({ phase: "checking", error: null });
    try {
      let info: UpdateInfo;
      if (get().mock) {
        await delay(900);
        info = MOCK_INFO;
      } else {
        const { invoke } = await import("@tauri-apps/api/core");
        info = await invoke<UpdateInfo>("update_check");
      }
      set({
        info,
        lastCheckedAt: Date.now(),
        phase: !info.configured
          ? "unconfigured"
          : info.updateAvailable
            ? "available"
            : "upToDate",
      });
    } catch (e) {
      set({ phase: "error", error: String(e), lastCheckedAt: Date.now() });
    }
  },

  apply: async () => {
    if (get().phase !== "available") return;
    set({ phase: "applying", error: null });
    try {
      if (get().mock) {
        await delay(1600);
        throw new Error(MOCK_APPLY_ERROR);
      }
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke<void>("update_apply");
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      set({ phase: "available", error: msg });
    }
  },

  dismiss: () => set({ dismissed: true }),
}));
