"use client";

import { memo, useMemo, useState } from "react";
import { ChevronRight, MessageSquare, Plus, Trash2 } from "lucide-react";
import { motion } from "motion/react";
import { ScrollArea } from "@appica/ui-react/scroll-area";
import { useSessions, type ChatSessionMeta } from "@/lib/pi/sessions";
import { useT } from "@/lib/i18n";

const SESSION_COLLAPSE_THRESHOLD = 8;

/** Compact left rail for conversation history only. */
export function SessionSidebar() {
  const { sessions, activeId, newSession } = useSessions();
  const [expanded, setExpanded] = useState(false);
  const t = useT();

  const visibleSessions = useMemo(() => {
    if (expanded || sessions.length <= SESSION_COLLAPSE_THRESHOLD) return sessions;
    const head = sessions.slice(0, SESSION_COLLAPSE_THRESHOLD);
    if (activeId && !head.some((session) => session.id === activeId)) {
      const active = sessions.find((session) => session.id === activeId);
      if (active) return [...head, active];
    }
    return head;
  }, [activeId, expanded, sessions]);

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
        viewportProps={{ style: { paddingBottom: 16 } }}
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
            {t("sidebar.sessions")}
          </span>
          <motion.button
            title={t("sidebar.newSession")}
            aria-label={t("sidebar.newSession")}
            onClick={() => newSession()}
            whileTap={{ scale: 0.85 }}
            transition={{ type: "spring", stiffness: 500, damping: 24 }}
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
              cursor: "pointer",
            }}
          >
            <Plus size={14} />
          </motion.button>
        </div>

        {visibleSessions.map((session) => (
          <SessionRow key={session.id} session={session} active={session.id === activeId} />
        ))}

        {sessions.length > SESSION_COLLAPSE_THRESHOLD && (
          <motion.button
            whileTap={{ scale: 0.97 }}
            transition={{ type: "spring", stiffness: 500, damping: 30 }}
            onClick={() => setExpanded((value) => !value)}
            style={{
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
            }}
          >
            <ChevronRight
              size={12}
              style={{
                transform: expanded ? "rotate(90deg)" : "none",
                transition: "transform 0.15s ease",
              }}
            />
            <span>
              {expanded
                ? t("sidebar.collapseSessions")
                : t("sidebar.showAllSessions", { count: sessions.length })}
            </span>
          </motion.button>
        )}
      </ScrollArea>
    </nav>
  );
}

const SessionRow = memo(function SessionRow({
  session,
  active,
}: {
  session: ChatSessionMeta;
  active: boolean;
}) {
  const switchSession = useSessions((state) => state.switchSession);
  const renameSession = useSessions((state) => state.renameSession);
  const deleteSession = useSessions((state) => state.deleteSession);
  const [hover, setHover] = useState(false);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const t = useT();

  const name = session.name || t("session.untitled");
  const commitRename = () => {
    setEditing(false);
    const next = draft.trim();
    if (next && next !== session.name) renameSession(session.id, next);
  };

  return (
    <div
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
      style={{ position: "relative" }}
    >
      <button
        className="pi-row"
        onClick={() => !editing && switchSession(session.id)}
        onDoubleClick={() => {
          setDraft(session.name);
          setEditing(true);
        }}
        title={session.preview || name}
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
            onBlur={commitRename}
            onKeyDown={(event) => {
              if (event.key === "Enter") commitRename();
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
              paddingRight: hover ? 18 : 0,
            }}
          >
            {name}
          </span>
        )}
      </button>
      {hover && !editing && (
        <motion.button
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          title={t("session.delete")}
          aria-label={t("session.delete")}
          onClick={(event) => {
            event.stopPropagation();
            deleteSession(session.id);
          }}
          style={{
            position: "absolute",
            right: 14,
            top: "50%",
            transform: "translateY(-50%)",
            display: "grid",
            placeItems: "center",
            width: 20,
            height: 20,
            border: "none",
            borderRadius: 5,
            background: "transparent",
            color: "var(--text-tertiary)",
            cursor: "pointer",
          }}
        >
          <Trash2 size={12} />
        </motion.button>
      )}
    </div>
  );
}, (prev, next) => prev.session === next.session && prev.active === next.active);
