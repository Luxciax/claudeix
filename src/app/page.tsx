"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { FolderOpen, Play, Plus, Square, Terminal } from "lucide-react";

import { useSessionStore } from "@/features/sessions/session-store";
import type { ClaudeStatus } from "@/features/sessions/types";
import { TranscriptTimeline } from "@/features/transcript/TranscriptTimeline";
import { useTranscriptStore } from "@/features/transcript/transcript-store";
import { api, isTauriRuntime } from "@/lib/tauri-api";

type StreamMessage = {
  id: string;
  role: "user" | "assistant" | "system";
  text: string;
};

function parseClaudeLine(raw: string): { text?: string; sessionId?: string } {
  try {
    const event = JSON.parse(raw) as Record<string, unknown>;
    const sessionId =
      typeof event.session_id === "string"
        ? event.session_id
        : typeof event.sessionId === "string"
          ? event.sessionId
          : undefined;
    const nested = event.event as Record<string, unknown> | undefined;
    const delta = nested?.delta as Record<string, unknown> | undefined;
    if (typeof delta?.text === "string") return { text: delta.text, sessionId };

    const message = event.message as Record<string, unknown> | undefined;
    if (Array.isArray(message?.content)) {
      const text = message.content
        .map((item) =>
          item &&
          typeof item === "object" &&
          typeof (item as { text?: unknown }).text === "string"
            ? (item as { text: string }).text
            : "",
        )
        .join("");
      if (text) return { text, sessionId };
    }
    if (typeof event.result === "string") return { text: event.result, sessionId };
    return { sessionId };
  } catch {
    return {};
  }
}

export default function Home() {
  const [status, setStatus] = useState<ClaudeStatus>({
    installed: false,
    version: "检测中…",
  });
  const [streamMessages, setStreamMessages] = useState<StreamMessage[]>([]);
  const [draft, setDraft] = useState("");
  const [running, setRunning] = useState(false);
  const [permissionMode, setPermissionMode] = useState("default");
  const [identifiedSessionId, setIdentifiedSessionId] = useState<string | null>(null);
  const identifiedSessionIdRef = useRef<string | null>(null);

  const workspace = useSessionStore((state) => state.workspaceRoot);
  const sessions = useSessionStore((state) => state.sessions);
  const activeKey = useSessionStore((state) => state.activeKey);
  const sessionLoading = useSessionStore((state) => state.loading);
  const sessionError = useSessionStore((state) => state.error);
  const scanSessions = useSessionStore((state) => state.scan);
  const setActiveKey = useSessionStore((state) => state.setActiveKey);

  const detail = useTranscriptStore((state) => state.detail);
  const transcriptLoading = useTranscriptStore((state) => state.loading);
  const transcriptError = useTranscriptStore((state) => state.error);
  const loadTranscript = useTranscriptStore((state) => state.load);
  const clearTranscript = useTranscriptStore((state) => state.clear);

  const activeSummary = useMemo(
    () => sessions.find((session) => session.key === activeKey) ?? null,
    [sessions, activeKey],
  );
  const activeTitle = detail?.summary.title ?? activeSummary?.title ?? "新会话";
  const activeSessionId = detail?.summary.sessionId ?? identifiedSessionId;

  const updateIdentifiedSession = (sessionId: string | null) => {
    identifiedSessionIdRef.current = sessionId;
    setIdentifiedSessionId(sessionId);
  };

  const selectSession = async (sessionKey: string) => {
    if (running) return;
    setActiveKey(sessionKey);
    setStreamMessages([]);
    updateIdentifiedSession(null);
    await loadTranscript(sessionKey);
  };

  const newSession = () => {
    if (running) return;
    setActiveKey(null);
    clearTranscript();
    setStreamMessages([]);
    updateIdentifiedSession(null);
  };

  useEffect(() => {
    if (!isTauriRuntime()) {
      setStatus({ installed: false, version: "浏览器预览模式" });
      return;
    }

    void api.claudeStatus().then(setStatus);
    void api.workspaceRoot().then((root) => {
      void useSessionStore.getState().scan(root);
    });

    const unlisten: Array<() => void> = [];
    void listen<string>("claude://line", (event) => {
      const parsed = parseClaudeLine(event.payload);
      if (parsed.sessionId) {
        identifiedSessionIdRef.current = parsed.sessionId;
        setIdentifiedSessionId(parsed.sessionId);
      }
      const text = parsed.text;
      if (!text) return;
      setStreamMessages((current) => {
        const last = current[current.length - 1];
        if (last?.role === "assistant") {
          return [...current.slice(0, -1), { ...last, text: last.text + text }];
        }
        return [...current, { id: crypto.randomUUID(), role: "assistant", text }];
      });
    }).then((fn) => unlisten.push(fn));

    void listen<string>("claude://stderr", (event) => {
      setStreamMessages((current) => [
        ...current,
        { id: crypto.randomUUID(), role: "system", text: event.payload },
      ]);
    }).then((fn) => unlisten.push(fn));

    void listen<number | null>("claude://exit", async () => {
      setRunning(false);
      const sessionState = useSessionStore.getState();
      const transcriptState = useTranscriptStore.getState();
      if (!sessionState.workspaceRoot) return;

      const activeBeforeRefresh = sessionState.activeKey;
      const refreshed = await sessionState.scan(sessionState.workspaceRoot, true);
      if (
        activeBeforeRefresh &&
        refreshed.some((session) => session.key === activeBeforeRefresh)
      ) {
        useSessionStore.getState().setActiveKey(activeBeforeRefresh);
        await transcriptState.refresh(activeBeforeRefresh);
        setStreamMessages([]);
        return;
      }

      const created = refreshed.find(
        (session) => session.sessionId === identifiedSessionIdRef.current,
      );
      if (created) {
        useSessionStore.getState().setActiveKey(created.key);
        await transcriptState.load(created.key);
        setStreamMessages([]);
      }
    }).then((fn) => unlisten.push(fn));

    return () => unlisten.forEach((fn) => fn());
  }, []);

  const pickWorkspace = async () => {
    if (running) return;
    const picked = await api.pickWorkspace();
    if (!picked) return;
    const root = await api.openWorkspace(picked);
    newSession();
    await scanSessions(root, true);
  };

  const send = async () => {
    const prompt = draft.trim();
    if (!prompt || running || !workspace) return;
    setDraft("");
    setRunning(true);
    setStreamMessages((current) => [
      ...current,
      { id: crypto.randomUUID(), role: "user", text: prompt },
    ]);
    try {
      await api.runClaude({
        cwd: workspace,
        prompt,
        sessionId: activeSessionId,
        permissionMode,
      });
    } catch (error) {
      setRunning(false);
      setStreamMessages((current) => [
        ...current,
        { id: crypto.randomUUID(), role: "system", text: String(error) },
      ]);
    }
  };

  return (
    <main
      style={{
        height: "100vh",
        display: "grid",
        gridTemplateColumns: "280px 1fr",
        background: "var(--bg-base)",
      }}
    >
      <aside
        style={{
          borderRight: "1px solid var(--separator)",
          padding: 16,
          overflow: "auto",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 20 }}>
          <div
            style={{
              width: 34,
              height: 34,
              borderRadius: 10,
              display: "grid",
              placeItems: "center",
              background: "var(--accent)",
              color: "white",
              fontWeight: 700,
            }}
          >
            C
          </div>
          <div>
            <div style={{ fontWeight: 700 }}>Claudeix</div>
            <div style={{ fontSize: 11, color: "var(--text-secondary)" }}>
              {status.version}
            </div>
          </div>
        </div>

        <button
          onClick={pickWorkspace}
          disabled={running}
          style={{
            width: "100%",
            display: "flex",
            gap: 8,
            alignItems: "center",
            padding: "10px 12px",
            borderRadius: 10,
            border: "1px solid var(--separator)",
            background: "var(--bg-elevated)",
            color: "var(--text-primary)",
            opacity: running ? 0.55 : 1,
          }}
        >
          <FolderOpen size={15} />
          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
            {workspace || "选择工作区"}
          </span>
        </button>

        <button
          onClick={newSession}
          disabled={running}
          style={{
            marginTop: 12,
            width: "100%",
            display: "flex",
            gap: 8,
            alignItems: "center",
            padding: "9px 12px",
            borderRadius: 10,
            border: "none",
            background: "transparent",
            color: "var(--text-primary)",
            opacity: running ? 0.55 : 1,
          }}
        >
          <Plus size={15} /> 新建会话
        </button>

        <div
          style={{
            marginTop: 16,
            fontSize: 11,
            color: "var(--text-tertiary)",
            padding: "0 8px",
          }}
        >
          历史会话 {sessionLoading ? "· 扫描中" : `· ${sessions.length}`}
        </div>
        {sessionError && (
          <div style={{ margin: "8px", fontSize: 11, color: "var(--danger)" }}>
            {sessionError}
          </div>
        )}
        <div style={{ marginTop: 6, display: "grid", gap: 4 }}>
          {sessions.map((session) => (
            <button
              key={session.key}
              onClick={() => void selectSession(session.key)}
              disabled={running}
              style={{
                textAlign: "left",
                border: "none",
                borderRadius: 10,
                padding: "10px 11px",
                background:
                  activeKey === session.key ? "var(--bg-selected)" : "transparent",
                color: "var(--text-primary)",
                opacity: running && activeKey !== session.key ? 0.55 : 1,
              }}
            >
              <div
                style={{
                  fontSize: 13,
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {session.title}
              </div>
              <div
                style={{
                  marginTop: 3,
                  fontSize: 11,
                  color: "var(--text-secondary)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {session.preview || `${session.messageCount} 条记录`}
              </div>
            </button>
          ))}
        </div>
      </aside>

      <section style={{ minWidth: 0, display: "grid", gridTemplateRows: "56px 1fr auto" }}>
        <header
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
            padding: "0 20px",
            borderBottom: "1px solid var(--separator)",
          }}
        >
          <div>
            <div style={{ fontSize: 14, fontWeight: 600 }}>{activeTitle}</div>
            <div
              style={{
                fontSize: 11,
                color: status.installed ? "var(--success)" : "var(--warning)",
              }}
            >
              {transcriptLoading
                ? "正在读取原生会话…"
                : status.installed
                  ? detail?.hasBranches
                    ? "Claude Code 已就绪 · 此会话包含分支"
                    : "Claude Code 已就绪"
                  : "未检测到 Claude Code"}
            </div>
          </div>
          <select
            value={permissionMode}
            onChange={(event) => setPermissionMode(event.target.value)}
            disabled={running}
            style={{
              border: "1px solid var(--separator)",
              borderRadius: 8,
              background: "var(--bg-elevated)",
              color: "var(--text-primary)",
              padding: "7px 10px",
            }}
          >
            <option value="default">询问权限</option>
            <option value="acceptEdits">接受编辑</option>
            <option value="plan">计划模式</option>
          </select>
        </header>

        <div style={{ overflow: "auto", padding: "24px max(24px, 10vw)" }}>
          {transcriptError && (
            <div
              style={{
                padding: 12,
                borderRadius: 10,
                background: "var(--warning-soft)",
                color: "var(--danger)",
              }}
            >
              {transcriptError}
            </div>
          )}
          {!detail && streamMessages.length === 0 && !transcriptLoading ? (
            <div
              style={{
                height: "100%",
                display: "grid",
                placeItems: "center",
                color: "var(--text-secondary)",
                textAlign: "center",
              }}
            >
              <div>
                <Terminal size={34} style={{ marginBottom: 14 }} />
                <div style={{ fontSize: 18, color: "var(--text-primary)" }}>
                  选择历史会话即可直接继续
                </div>
                <div style={{ marginTop: 7, fontSize: 13 }}>
                  不需要先添加到工作台。新任务也可以直接在下方发送。
                </div>
              </div>
            </div>
          ) : (
            <>
              {detail && <TranscriptTimeline detail={detail} />}
              {streamMessages.map((message) => (
                <article
                  key={message.id}
                  style={{
                    marginBottom: 18,
                    marginLeft: message.role === "user" ? "18%" : 0,
                    padding: "13px 15px",
                    borderRadius: 14,
                    background:
                      message.role === "user"
                        ? "var(--accent-soft)"
                        : message.role === "system"
                          ? "var(--warning-soft)"
                          : "var(--bg-elevated)",
                    whiteSpace: "pre-wrap",
                    lineHeight: 1.6,
                    fontSize: 14,
                  }}
                >
                  {message.text}
                </article>
              ))}
            </>
          )}
        </div>

        <div style={{ padding: "14px max(24px, 10vw) 20px" }}>
          <div
            style={{
              display: "flex",
              alignItems: "flex-end",
              gap: 10,
              border: "1px solid var(--separator)",
              borderRadius: 16,
              background: "var(--bg-elevated)",
              padding: 10,
            }}
          >
            <textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter" && !event.shiftKey) {
                  event.preventDefault();
                  void send();
                }
              }}
              placeholder={workspace ? "向 Claude Code 描述任务…" : "先选择工作区"}
              disabled={!workspace || !status.installed}
              rows={3}
              style={{
                flex: 1,
                resize: "none",
                border: "none",
                outline: "none",
                background: "transparent",
                color: "var(--text-primary)",
                font: "inherit",
                padding: 6,
              }}
            />
            {running ? (
              <button
                onClick={() => void api.stopClaude()}
                style={{
                  width: 38,
                  height: 38,
                  borderRadius: 10,
                  border: "none",
                  background: "var(--danger)",
                  color: "white",
                }}
              >
                <Square size={16} />
              </button>
            ) : (
              <button
                onClick={() => void send()}
                disabled={!draft.trim() || !workspace || !status.installed}
                style={{
                  width: 38,
                  height: 38,
                  borderRadius: 10,
                  border: "none",
                  background: "var(--accent)",
                  color: "white",
                  opacity: !draft.trim() ? 0.45 : 1,
                }}
              >
                <Play size={16} />
              </button>
            )}
          </div>
        </div>
      </section>
    </main>
  );
}
