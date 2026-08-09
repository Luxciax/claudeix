import { useUI } from "./store";
import { isTauri } from "./pi/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

let nativeQuitInProgress = false;

export function isNativeQuitInProgress() {
  return nativeQuitInProgress;
}

/** Hide the main window into the system tray (used by the "minimize" behavior). */
export async function minimizeToTray() {
  if (!isTauri()) return;
  try {
    await getCurrentWindow().hide();
  } catch {
    // window may already be hidden or unavailable
  }
}

/**
 * Restore the main window from the system tray — show, unminimize and focus.
 * This is the frontend counterpart to minimizeToTray() and the Rust-side
 * tray-icon click handler. Used by notification click to bring Pi back.
 */
export async function restoreFromTray() {
  if (!isTauri()) return;
  try {
    const w = getCurrentWindow();
    await w.show();
    await w.unminimize();
    await w.setFocus();
  } catch {
    // window may be unavailable
  }
}

/** Fully quit the application (used by the "quit" behavior). */
export async function quitApp() {
  if (isTauri()) {
    if (nativeQuitInProgress) return;
    nativeQuitInProgress = true;
    try {
      await invoke("app_quit");
    } catch (error) {
      nativeQuitInProgress = false;
      console.error("[window-close] native quit failed:", error);
      // Calling window.close() here would recurse into onCloseRequested, which
      // intentionally prevents the native close. Keep the app alive and show
      // the close dialog again instead of leaving a blank WebView behind.
      useUI.getState().setCloseDialogOpen(true);
    }
  } else {
    window.close();
  }
}

/**
 * Central entry point for any "close" intent — the caption close button,
 * Alt+F4, or the OS native close. Routes to the user's saved behavior:
 * ask (open a dialog), minimize (to tray), or quit.
 */
export function requestClose() {
  const behavior = useUI.getState().closeBehavior;
  if (behavior === "minimize") {
    void minimizeToTray();
  } else if (behavior === "quit") {
    void quitApp();
  } else {
    useUI.getState().setCloseDialogOpen(true);
  }
}
