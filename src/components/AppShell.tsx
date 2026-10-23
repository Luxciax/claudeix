"use client";

import { useEffect, useState } from "react";
import { usePathname } from "next/navigation";
import { TooltipProvider } from "@appica/ui-react/tooltip";
import { NavRail } from "./NavRail";
import { ExtensionSheet } from "./ExtensionSheet";
import { SubagentDetail } from "./Subagents";
import { useWorkspace } from "@/lib/workspace";
import { useI18n } from "@/lib/i18n";
import { useAppearance } from "@/lib/appearance";
import { useUI } from "@/lib/store";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isNativeQuitInProgress, requestClose } from "@/lib/window-close";
import { CloseConfirmDialog } from "./CloseConfirmDialog";
import { RecoveryDialog, type RecoveryDialogItem } from "@/features/transcript/RecoveryDialog";
import { useRuntimeStore } from "@/features/runtime/runtime-store";
import { useClaudeModel } from "@/features/runtime/model-store";
import { useSessionStore } from "@/features/sessions/session-store";
import { api, isTauriRuntime } from "@/lib/tauri-api";
import { useEngineSelection } from "@/features/engines/engine-selection";

/** Pi Desktop chrome retained as Claudeix's root shell. */
export function AppShell({ children }: { children: React.ReactNode }) {
  const pathname = usePathname();
  // The pet companion runs in its own tiny transparent window — no app chrome,
  // and no pi connection (the main window owns the pi process).
  if (pathname?.startsWith("/pet")) {
    return <>{children}</>;
  }
  return <MainShell>{children}</MainShell>;
}

function MainShell({ children }: { children: React.ReactNode }) {
  const [recoveryItems, setRecoveryItems] = useState<RecoveryDialogItem[]>([]);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);
  const [resolvingId, setResolvingId] = useState<string | null>(null);

  useEffect(() => {
    useI18n.getState().initLocale();
    useUI.getState().initTheme();
    useUI.getState().initCloseBehavior();
    useAppearance.getState().init();
    useEngineSelection.getState().init();

    let disposeRuntime: () => void = () => undefined;
    let closeUnlisten: (() => void) | undefined;
    const preventDefaultCtx = (event: MouseEvent) => event.preventDefault();

    void useRuntimeStore.getState().bindEvents().then((dispose) => {
      disposeRuntime = dispose;
    });
    void useWorkspace.getState().init().then(async () => {
      const root = useWorkspace.getState().root ?? "";
      await useClaudeModel.getState().load(root || null);
      if (root) await useSessionStore.getState().scan(root, true);
    });

    if (isTauriRuntime()) {
      document.addEventListener("contextmenu", preventDefaultCtx);
      void Promise.all([api.listTranscriptRecovery(), api.listEngineMutationRecovery()])
        .then(([claudeItems, engineItems]) => {
          setRecoveryItems([...claudeItems, ...engineItems]);
          setRecoveryError(null);
        })
        .catch((error) => setRecoveryError(`无法检查未完成会话事务：${String(error)}`));

      getCurrentWindow()
        .onCloseRequested((event) => {
          if (isNativeQuitInProgress()) return;
          event.preventDefault();
          requestClose();
        })
        .then((fn) => {
          closeUnlisten = fn;
        })
        .catch(() => {});
    }

    return () => {
      document.removeEventListener("contextmenu", preventDefaultCtx);
      disposeRuntime();
      closeUnlisten?.();
    };
  }, []);

  const resolveRecovery = async (
    item: RecoveryDialogItem,
    action: "keep_current" | "restore_backup",
  ) => {
    setResolvingId(item.transactionId);
    setRecoveryError(null);
    try {
      if ("engineId" in item) {
        await api.resolveEngineMutationRecovery({ transactionId: item.transactionId, action });
      } else {
        await api.resolveTranscriptRecovery({ transactionId: item.transactionId, action });
      }
      const [claudeItems, engineItems] = await Promise.all([
        api.listTranscriptRecovery(),
        api.listEngineMutationRecovery(),
      ]);
      setRecoveryItems([...claudeItems, ...engineItems]);
      const root = useWorkspace.getState().root;
      if (root) await useSessionStore.getState().scan(root, true);
    } catch (error) {
      setRecoveryError(String(error));
    } finally {
      setResolvingId(null);
    }
  };

  return (
    <TooltipProvider delay={400}>
      <div style={{ display: "flex", height: "100vh", overflow: "hidden" }} data-testid="app-shell">
        <NavRail />
        <div style={{ flex: 1, minWidth: 0, height: "100%" }}>{children}</div>
        <ExtensionSheet />
        <SubagentDetail />
        <CloseConfirmDialog />
        {(recoveryItems.length > 0 || recoveryError) && (
          <RecoveryDialog
            items={recoveryItems}
            resolvingId={resolvingId}
            error={recoveryError}
            onResolve={(item, action) => void resolveRecovery(item, action)}
          />
        )}
      </div>
    </TooltipProvider>
  );
}
