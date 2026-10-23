"use client";

import { motion } from "motion/react";
import { ScrollArea } from "@appica/ui-react/scroll-area";
import { useState } from "react";
import { MessageSquare, Plus, RefreshCw, Trash2 } from "lucide-react";

import type { EngineTrashDeletePreview, NativeSessionSummary } from "@/features/engines/types";
import { usePiNativeStore } from "@/features/engines/pi-native-store";
import { DeleteSessionDialog } from "@/features/trash/DeleteSessionDialog";
import { EngineTrashPanel } from "@/features/trash/EngineTrashPanel";
import { api } from "@/lib/tauri-api";
import { useWorkspace } from "@/lib/workspace";

export function PiSessionSidebar() {
  const workspace = useWorkspace((state) => state.root) ?? "";
  const sessions = usePiNativeStore((state) => state.sessions);
  const activeKey = usePiNativeStore((state) => state.activeKey);
  const loading = usePiNativeStore((state) => state.loading);
  const error = usePiNativeStore((state) => state.error);
  const [showTrash, setShowTrash] = useState(false);
  const [deletePreview, setDeletePreview] = useState<EngineTrashDeletePreview | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);

  const requestDelete = async (session: NativeSessionSummary) => {
    if (!workspace) return;
    try {
      setDeletePreview(
        await api.previewEngineTrashDelete("pi", session.sessionKey, workspace),
      );
    } catch (reason) {
      usePiNativeStore.setState({ error: String(reason) });
    }
  };

  const confirmDelete = async () => {
    if (!deletePreview || !workspace) return;
    setDeleteBusy(true);
    try {
      await api.moveEngineSessionToTrash({
        engineId: "pi",
        sessionKey: deletePreview.sessionKey,
        workspaceRoot: workspace,
        expectedPreviewFingerprint: deletePreview.previewFingerprint,
      });
      const removedKey = deletePreview.sessionKey;
      setDeletePreview(null);
      const sessions = await usePiNativeStore.getState().refresh(workspace);
      if (usePiNativeStore.getState().activeKey === removedKey) {
        usePiNativeStore.setState({ activeKey: null, detail: null });
        const next = sessions[0];
        if (next) await usePiNativeStore.getState().select(next.sessionKey, workspace);
        else await usePiNativeStore.getState().newSession(workspace);
      }
    } catch (reason) {
      usePiNativeStore.setState({ error: String(reason) });
    } finally {
      setDeleteBusy(false);
    }
  };

  return (
    <nav
      className="material"
      style={{
        width: 248,
        height: "100%",
        borderRight: "1px solid var(--separator)",
        flexShrink: 0,
        overflow: "hidden",
      }}
    >
      <ScrollArea
        orientation="vertical"
        scrollShadow
        style={{ height: "100%" }}
        viewportProps={{ style: { paddingBottom: 16, overscrollBehaviorY: "contain" } }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "14px 10px 6px 16px",
          }}
        >
          <span
            style={{
              fontSize: 11,
              fontWeight: 600,
              letterSpacing: "0.04em",
              textTransform: "uppercase",
              color: "var(--text-tertiary)",
            }}
          >
            Pi 原生会话
          </span>
          <div style={{ display: "flex", gap: 2 }}>
            <SidebarIconButton
              title="刷新 Pi 会话"
              disabled={loading}
              onClick={() => void usePiNativeStore.getState().refresh(workspace)}
            >
              <RefreshCw size={13} className={loading ? "animate-spin" : undefined} />
            </SidebarIconButton>
            <SidebarIconButton
              title="新建 Pi 会话"
              onClick={() => void usePiNativeStore.getState().newSession(workspace)}
            >
              <Plus size={14} />
            </SidebarIconButton>
          </div>
        </div>

        <button
          onClick={() => setShowTrash(true)}
          title="打开 Pi 回收站"
          style={{
            display: "flex",
            alignItems: "center",
            gap: 7,
            width: "calc(100% - 16px)",
            margin: "2px 8px 7px",
            padding: "7px 9px",
            border: "1px solid var(--separator)",
            borderRadius: 8,
            background: "var(--bg-elevated)",
            color: "var(--text-secondary)",
            fontSize: 11,
            cursor: "pointer",
          }}
        >
          <Trash2 size={13} />
          <span>回收站</span>
          <span style={{ marginLeft: "auto", fontSize: 10, color: "var(--text-tertiary)" }}>
            可恢复已删除会话
          </span>
        </button>

        {error && (
          <div style={{ padding: "5px 16px 8px", color: "var(--danger)", fontSize: 11 }}>
            {error}
          </div>
        )}

        {sessions.length === 0 && !loading && (
          <div style={{ padding: "12px 16px", fontSize: 12, color: "var(--text-tertiary)" }}>
            当前工作区没有发现 Pi 原生会话。
          </div>
        )}

        {sessions.map((session) => (
          <div
            key={session.sessionKey}
            style={{
              display: "flex",
              alignItems: "center",
              gap: 2,
              width: "calc(100% - 12px)",
              margin: "1px 6px",
              borderRadius: 8,
              background:
                activeKey === session.sessionKey ? "var(--accent-muted)" : "transparent",
            }}
          >
            <button
              className="pi-row"
              onClick={() => void usePiNativeStore.getState().select(session.sessionKey, workspace)}
              title={session.preview || session.title}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                minWidth: 0,
                flex: 1,
                padding: "8px 8px 8px 10px",
                fontSize: 13,
                textAlign: "left",
                border: "none",
                borderRadius: 8,
                cursor: "pointer",
                color:
                  activeKey === session.sessionKey ? "var(--text-primary)" : "var(--text-secondary)",
                background: "transparent",
              }}
            >
              <MessageSquare size={14} style={{ flexShrink: 0, opacity: 0.72 }} />
              <span style={{ minWidth: 0, flex: 1 }}>
                <span style={{ display: "block", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                  {session.title || session.nativeSessionId}
                </span>
                {session.preview && session.preview !== session.title && (
                  <span style={{ display: "block", marginTop: 2, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", fontSize: 10.5, color: "var(--text-tertiary)" }}>
                    {session.preview}
                  </span>
                )}
              </span>
            </button>
            <SidebarIconButton title="移入回收站" onClick={() => void requestDelete(session)}>
              <Trash2 size={12} />
            </SidebarIconButton>
          </div>
        ))}
      </ScrollArea>

      {deletePreview && (
        <DeleteSessionDialog
          preview={deletePreview}
          mutating={deleteBusy}
          agentLabel="Pi"
          onCancel={() => setDeletePreview(null)}
          onConfirm={() => void confirmDelete()}
        />
      )}
      {showTrash && (
        <EngineTrashPanel
          engineId="pi"
          engineLabel="Pi"
          onClose={() => setShowTrash(false)}
          onRestored={() => void usePiNativeStore.getState().refresh(workspace)}
        />
      )}
    </nav>
  );
}

function SidebarIconButton({
  title,
  onClick,
  disabled,
  children,
}: {
  title: string;
  onClick: () => void;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <motion.button
      title={title}
      aria-label={title}
      onClick={onClick}
      disabled={disabled}
      whileTap={{ scale: 0.85 }}
      className="pi-row"
      style={{
        display: "grid",
        placeItems: "center",
        width: 22,
        height: 22,
        border: "none",
        borderRadius: 6,
        background: "transparent",
        color: "var(--text-tertiary)",
        cursor: disabled ? "default" : "pointer",
        opacity: disabled ? 0.55 : 1,
      }}
    >
      {children}
    </motion.button>
  );
}
