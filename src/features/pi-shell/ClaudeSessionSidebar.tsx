"use client";

import { useMemo, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { ScrollArea } from "@appica/ui-react/scroll-area";
import {
  Archive,
  ArchiveRestore,
  ChevronRight,
  MessageSquare,
  Plus,
  RefreshCw,
  Trash2,
} from "lucide-react";

import { useSessionStore } from "@/features/sessions/session-store";
import type { SessionSummary } from "@/features/sessions/types";
import { useTranscriptStore } from "@/features/transcript/transcript-store";
import { useRuntimeStore } from "@/features/runtime/runtime-store";
import { DeleteSessionDialog } from "@/features/trash/DeleteSessionDialog";
import { TrashPanel } from "@/features/trash/TrashPanel";
import type { TrashDeletePreview } from "@/features/trash/types";
import { api } from "@/lib/tauri-api";
import { useWorkspace } from "@/lib/workspace";

const SESSION_COLLAPSE_THRESHOLD = 8;

export function ClaudeSessionSidebar() {
  const sessions = useSessionStore((state) => state.sessions);
  const activeKey = useSessionStore((state) => state.activeKey);
  const loading = useSessionStore((state) => state.loading);
  const error = useSessionStore((state) => state.error);
  const setActiveKey = useSessionStore((state) => state.setActiveKey);
  const scan = useSessionStore((state) => state.scan);
  const workspace = useWorkspace((state) => state.root) ?? "";
  const [expanded, setExpanded] = useState(false);
  const [showArchived, setShowArchived] = useState(false);
  const [showTrash, setShowTrash] = useState(false);
  const [deletePreview, setDeletePreview] = useState<TrashDeletePreview | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);

  const activeSessions = useMemo(
    () => sessions.filter((session) => !session.archived),
    [sessions],
  );
  const archivedSessions = useMemo(
    () => sessions.filter((session) => session.archived),
    [sessions],
  );
  const visibleSessions = useMemo(() => {
    if (expanded || activeSessions.length <= SESSION_COLLAPSE_THRESHOLD) return activeSessions;
    const head = activeSessions.slice(0, SESSION_COLLAPSE_THRESHOLD);
    if (activeKey && !head.some((session) => session.key === activeKey)) {
      const active = activeSessions.find((session) => session.key === activeKey);
      if (active) return [...head, active];
    }
    return head;
  }, [activeKey, activeSessions, expanded]);

  const selectSession = async (sessionKey: string) => {
    const runtime = useRuntimeStore.getState().snapshot;
    if (
      runtime?.status === "starting" ||
      runtime?.status === "running" ||
      runtime?.status === "waiting_permission" ||
      runtime?.status === "stopping"
    )
      return;
    setActiveKey(sessionKey);
    useRuntimeStore.getState().reset();
    await Promise.all([
      useTranscriptStore.getState().load(sessionKey),
      useTranscriptStore.getState().loadSnapshots(sessionKey),
      useTranscriptStore.getState().loadAudit(sessionKey),
    ]);
  };

  const newSession = () => {
    setActiveKey(null);
    useTranscriptStore.getState().clear();
    useRuntimeStore.getState().reset();
  };

  const refresh = async () => {
    if (workspace) await scan(workspace, true);
  };

  const archiveSession = async (session: SessionSummary, archived: boolean) => {
    await api.setSessionArchived({ sessionKey: session.key, archived });
    await refresh();
    if (archived && activeKey === session.key) newSession();
  };

  const renameSession = async (session: SessionSummary, title: string) => {
    const next = title.trim();
    if (!next || next === session.title) return;
    const detail = await api.getSession(session.key);
    await api.renameSession({
      sessionKey: session.key,
      expectedFileFingerprint: detail.fileFingerprint,
      title: next,
    });
    await refresh();
    if (activeKey === session.key) await useTranscriptStore.getState().refresh(session.key);
  };

  const requestDelete = async (session: SessionSummary) => {
    setDeletePreview(await api.previewTrashDelete(session.key));
  };

  const confirmDelete = async () => {
    if (!deletePreview) return;
    setDeleteBusy(true);
    try {
      const deletedSessionId = deletePreview.sessionId;
      await api.moveSessionToTrash({
        sessionKey: deletePreview.sessionKey,
        expectedPreviewFingerprint: deletePreview.previewFingerprint,
      });
      setDeletePreview(null);
      const refreshed = workspace ? await scan(workspace, true) : [];
      const current = sessions.find((session) => session.sessionId === deletedSessionId);
      if (current?.key === activeKey) {
        const next = refreshed.find((session) => !session.archived);
        if (next) await selectSession(next.key);
        else newSession();
      }
    } finally {
      setDeleteBusy(false);
    }
  };

  return (
    <>
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
              会话
            </span>
            <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
              <SidebarIconButton title="刷新会话" onClick={() => void refresh()} disabled={loading}>
                <RefreshCw size={13} className={loading ? "animate-spin" : undefined} />
              </SidebarIconButton>
              <SidebarIconButton title="新建会话" onClick={newSession}>
                <Plus size={14} />
              </SidebarIconButton>
            </div>
          </div>

          <button
            onClick={() => setShowTrash(true)}
            style={trashEntryStyle}
            title="打开 Claude 回收站"
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

          {visibleSessions.map((session) => (
            <SessionRow
              key={session.key}
              session={session}
              active={session.key === activeKey}
              onSelect={selectSession}
              onRename={renameSession}
              onArchive={(item) => archiveSession(item, true)}
              onDelete={requestDelete}
            />
          ))}

          {activeSessions.length > SESSION_COLLAPSE_THRESHOLD && (
            <button
              onClick={() => setExpanded((value) => !value)}
              style={collapseButtonStyle}
            >
              <ChevronRight
                size={12}
                style={{
                  transform: expanded ? "rotate(90deg)" : "none",
                  transition: "transform 0.15s ease",
                }}
              />
              {expanded ? "收起会话" : `显示全部 ${activeSessions.length} 个会话`}
            </button>
          )}

          {archivedSessions.length > 0 && (
            <>
              <button onClick={() => setShowArchived((value) => !value)} style={collapseButtonStyle}>
                <ChevronRight
                  size={12}
                  style={{
                    transform: showArchived ? "rotate(90deg)" : "none",
                    transition: "transform 0.15s ease",
                  }}
                />
                归档会话 · {archivedSessions.length}
              </button>
              <AnimatePresence initial={false}>
                {showArchived &&
                  archivedSessions.map((session) => (
                    <SessionRow
                      key={session.key}
                      session={session}
                      active={session.key === activeKey}
                      archived
                      onSelect={selectSession}
                      onRename={renameSession}
                      onArchive={(item) => archiveSession(item, false)}
                      onDelete={requestDelete}
                    />
                  ))}
              </AnimatePresence>
            </>
          )}
        </ScrollArea>
      </nav>

      {deletePreview && (
        <DeleteSessionDialog
          preview={deletePreview}
          mutating={deleteBusy}
          onCancel={() => setDeletePreview(null)}
          onConfirm={() => void confirmDelete()}
        />
      )}
      {showTrash && (
        <TrashPanel
          onClose={() => setShowTrash(false)}
          onRestored={() => void refresh()}
        />
      )}
    </>
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

function SessionRow({
  session,
  active,
  archived = false,
  onSelect,
  onRename,
  onArchive,
  onDelete,
}: {
  session: SessionSummary;
  active: boolean;
  archived?: boolean;
  onSelect: (key: string) => Promise<void>;
  onRename: (session: SessionSummary, title: string) => Promise<void>;
  onArchive: (session: SessionSummary) => Promise<void>;
  onDelete: (session: SessionSummary) => Promise<void>;
}) {
  const [hover, setHover] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(session.title);

  const commitRename = async () => {
    setEditing(false);
    await onRename(session, draft);
  };

  return (
    <motion.div
      layout
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{ position: "relative" }}
    >
      <button
        className="pi-row"
        onClick={() => !editing && void onSelect(session.key)}
        onDoubleClick={() => {
          setDraft(session.title);
          setEditing(true);
        }}
        title={session.preview || session.title}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 10,
          width: "calc(100% - 12px)",
          margin: "1px 6px",
          padding: "7px 10px",
          fontSize: 13,
          textAlign: "left",
          border: "none",
          borderRadius: 8,
          cursor: "pointer",
          color: active ? "var(--text-primary)" : "var(--text-secondary)",
          background: active ? "var(--accent-muted)" : "transparent",
          opacity: archived ? 0.72 : 1,
        }}
      >
        <span style={{ opacity: 0.7, width: 15, display: "grid", placeItems: "center" }}>
          <MessageSquare size={14} />
        </span>
        {editing ? (
          <input
            autoFocus
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
            onBlur={() => void commitRename()}
            onKeyDown={(event) => {
              if (event.key === "Enter") void commitRename();
              if (event.key === "Escape") setEditing(false);
            }}
            onClick={(event) => event.stopPropagation()}
            style={{
              flex: 1,
              minWidth: 0,
              border: "none",
              outline: "none",
              background: "transparent",
              fontSize: 13,
              color: "var(--text-primary)",
              fontFamily: "var(--font-ui)",
            }}
          />
        ) : (
          <span
            style={{
              overflow: "hidden",
              textOverflow: "ellipsis",
              whiteSpace: "nowrap",
              paddingRight: hover ? 42 : 0,
            }}
          >
            {session.title || "未命名会话"}
          </span>
        )}
      </button>

      {hover && !editing && (
        <div
          style={{
            position: "absolute",
            right: 12,
            top: "50%",
            transform: "translateY(-50%)",
            display: "flex",
            alignItems: "center",
          }}
        >
          <SidebarIconButton
            title={archived ? "取消归档" : "归档"}
            onClick={() => void onArchive(session)}
          >
            {archived ? <ArchiveRestore size={11} /> : <Archive size={11} />}
          </SidebarIconButton>
          <SidebarIconButton title="移入回收站" onClick={() => void onDelete(session)}>
            <Trash2 size={11} />
          </SidebarIconButton>
        </div>
      )}
    </motion.div>
  );
}

const collapseButtonStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 6,
  width: "calc(100% - 12px)",
  margin: "2px 6px",
  padding: "6px 10px",
  fontSize: 12,
  textAlign: "left",
  border: "none",
  borderRadius: 8,
  background: "transparent",
  color: "var(--text-tertiary)",
  cursor: "pointer",
};

const trashEntryStyle: React.CSSProperties = {
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
};
