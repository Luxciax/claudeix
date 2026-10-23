"use client";

import { create } from "zustand";

export type ActiveEngine = "claude" | "pi";

const STORAGE_KEY = "claudeix.activeEngine";

type EngineSelectionState = {
  activeEngine: ActiveEngine;
  initialized: boolean;
  init: () => void;
  setActiveEngine: (engine: ActiveEngine) => void;
};

export const useEngineSelection = create<EngineSelectionState>((set) => ({
  activeEngine: "claude",
  initialized: false,
  init: () => {
    if (typeof window === "undefined") return;
    const stored = window.localStorage.getItem(STORAGE_KEY);
    set({
      activeEngine: stored === "pi" ? "pi" : "claude",
      initialized: true,
    });
  },
  setActiveEngine: (engine) => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(STORAGE_KEY, engine);
    }
    set({ activeEngine: engine, initialized: true });
  },
}));

