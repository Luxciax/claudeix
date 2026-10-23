"use client";

import { useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { Virtuoso, type Components, type VirtuosoHandle } from "react-virtuoso";
import { FilePenLine, History, SquarePen, X } from "lucide-react";

import { MessageBubble } from "@/components/MessageBubble";
import { ComposerInput } from "@/components/ComposerInput";
import { IconButton, SectionLabel } from "@/components/primitives";
import { PiSpark, ShimmerText } from "@/components/ActivityLine";
import { SnapshotPanel } from "@/features/transcript/SnapshotPanel";
import { TranscriptTimeline } from "@/features/transcript/TranscriptTimeline";
import { useTranscriptStore } from "@/features/transcript/transcript-store";
import { useSessionStore } from "@/features/sessions/session-store";
import type { ContentBlock, SessionDetail, TranscriptRecord } from "@/features/sessions/types";
import { useRuntimeStore } from "@/features/runtime/runtime-store";
import { useClaudeModel } from "@/features/runtime/model-store";
import { ClaudeModelPicker } from "./ClaudeModelPicker";
import type { RuntimeTimelineItem } from "@/features/runtime/types";
import { api } from "@/lib/tauri-api";
import { useUI } from "@/lib/store";
import { useWorkspace } from "@/lib/workspace";
import type { ChatMessage, ChatToolCall, DeliveryMode, QueueEntry } from "@/lib/pi/chat";

type UiMessage = {
  key: string;
  message: ChatMessage;
};

function textFromBlocks(blocks: ContentBlock[]): string {
  return blocks
    .filter((block): block is Extract<ContentBlock, { type: "text" }> => block.type === "text")
    .map((block) => block.text)
    .filter(Boolean)
    .join("\n\n");
}

function thinkingFromBlocks(blocks: ContentBlock[]): string {
  return blocks
    .filter(
      (block): block is Extract<ContentBlock, { type: "thinking" }> =>
        block.type === "thinking" && !block.redacted,
    )
    .map((block) => block.text)
    .filter(Boolean)
    .join("\n\n");
}

function historicalMessages(detail: SessionDetail | null): UiMessage[] {
  if (!detail) return [];
  const results = new Map<string, { output: unknown; isError: boolean }>();
  for (const record of detail.records) {
    for (const block of record.blocks) {
      if (block.type === "tool_result") {
        results.set(block.tool_use_id, { output: block.output, isError: block.is_error });
      }
    }
  }

  return detail.records.flatMap((record): UiMessage[] => {
    if (record.role !== "user" && record.role !== "assistant") return [];
    const text = textFromBlocks(record.blocks);
    const thinking = thinkingFromBlocks(record.blocks);
    const tools: ChatToolCall[] = record.blocks
      .filter(
        (block): block is Extract<ContentBlock, { type: "tool_use" }> => block.type === "tool_use",
      )
      .map((block) => ({
        id: block.id,
        name: block.name,
        args: block.input,
        status: results.get(block.id)?.isError ? "error" : "done",
      }));
    if (!text && !thinking && tools.length === 0) return [];
    return [
      {
        key: record.uuid ?? `line-${record.lineIndex}`,
        message: {
          id: record.uuid ?? `msg-line-${record.lineIndex}`,
          role: record.role,
          text,
          thinking,
          tools,
          streaming: false,
        },
      },
    ];
  });
}

function liveMessages(items: RuntimeTimelineItem[], busy: boolean): UiMessage[] {
  const messages: UiMessage[] = [];
  const ensureAssistant = () => {
    const last = messages[messages.length - 1];
    if (last?.message.role === "assistant") return last;
    const next: UiMessage = {
      key: `live-assistant-${messages.length}`,
      message: {
        id: `live-assistant-${messages.length}`,
        role: "assistant",
        text: "",
        thinking: "",
        tools: [],
        streaming: busy,
      },
    };
    messages.push(next);
    return next;
  };

  for (const item of items) {
    if (item.type === "user") {
      messages.push({
        key: item.id,
        message: {
          id: item.id,
          role: "user",
          text: item.text,
          thinking: "",
          tools: [],
          streaming: false,
        },
      });
    } else if (item.type === "assistant") {
      messages.push({
        key: item.id,
        message: {
          id: item.id,
          role: "assistant",
          text: item.text,
          thinking: "",
          tools: [],
          streaming: busy,
        },
      });
    } else if (item.type === "tool") {
      const assistant = ensureAssistant();
      assistant.message.tools.push({
        id: item.toolUseId,
        name: item.name,
        args: item.input,
        status:
          item.status === "running" ? "running" : item.status === "failed" ? "error" : "done",
      });
    } else if (item.type === "system" && item.tone === "error") {
      messages.push({
        key: item.id,
        message: {
          id: item.id,
          role: "assistant",
          text: "",
          thinking: "",
          tools: [],
          streaming: false,
          isError: true,
          errorText: item.text,
        },
      });
    }
  }
  return messages;
}

export function ClaudeAgentPanel() {
  const workspace = useWorkspace((state) => state.root) ?? "";
  const activeKey = useSessionStore((state) => state.activeKey);
  const setActiveKey = useSessionStore((state) => state.setActiveKey);
  const scanSessions = useSessionStore((state) => state.scan);
  const detail = useTranscriptStore((state) => state.detail);
  const transcriptLoading = useTranscriptStore((state) => state.loading);
  const transcriptMutating = useTranscriptStore((state) => state.mutating);
  const snapshots = useTranscriptStore((state) => state.snapshots);
  const auditEntries = useTranscriptStore((state) => state.auditEntries);
  const runtime = useRuntimeStore((state) => state.snapshot);
  const timeline = useRuntimeStore((state) => state.timeline);
  const completionVersion = useRuntimeStore((state) => state.completionVersion);
  const startRuntime = useRuntimeStore((state) => state.start);
  const stopRuntime = useRuntimeStore((state) => state.stop);
  const decidePermission = useRuntimeStore((state) => state.decidePermission);
  const [draft, setDraft] = useState("");
  const [attachments, setAttachments] = useState<string[]>([]);
  const [permissionMode, setPermissionMode] = useState("manual");
  const selectedProviderId = useClaudeModel((state) => state.providerId);
  const selectedModel = useClaudeModel((state) => state.model);
  const [showEdit, setShowEdit] = useState(false);
  const [showSnapshots, setShowSnapshots] = useState(false);
  const [advancedAssistantEditing, setAdvancedAssistantEditing] = useState(false);
  const [isAtTop, setIsAtTop] = useState(true);
  const [isAtBottom, setIsAtBottom] = useState(true);
  const virtuosoRef = useRef<VirtuosoHandle>(null);
  const userScrollPauseUntilRef = useRef(0);

  const busy =
    runtime?.status === "starting" ||
    runtime?.status === "running" ||
    runtime?.status === "waiting_permission" ||
    runtime?.status === "stopping";
  const persisted = useMemo(() => historicalMessages(detail), [detail]);
  const live = useMemo(() => liveMessages(timeline, busy), [timeline, busy]);
  const messages = timeline.length > 0 ? [...persisted, ...live] : persisted;
  const permissionItems = timeline.filter(
    (item): item is Extract<RuntimeTimelineItem, { type: "permission" }> => item.type === "permission",
  );
  const waitingForFirstToken =
    busy && !timeline.some((item) => item.type === "assistant" || item.type === "tool");
  const canScroll = !(isAtTop && isAtBottom);

  useEffect(() => {
    if (!activeKey) {
      useTranscriptStore.getState().clear();
      return;
    }
    void Promise.all([
      useTranscriptStore.getState().load(activeKey),
      useTranscriptStore.getState().loadSnapshots(activeKey),
      useTranscriptStore.getState().loadAudit(activeKey),
    ]);
  }, [activeKey]);

  useEffect(() => {
    if (completionVersion === 0 || !workspace) return;
    void (async () => {
      const sessionId = useRuntimeStore.getState().snapshot?.sessionId;
      const currentKey = useSessionStore.getState().activeKey;
      const refreshed = await scanSessions(workspace, true);
      if (currentKey && refreshed.some((session) => session.key === currentKey)) {
        await useTranscriptStore.getState().refresh(currentKey);
      } else if (sessionId) {
        const created = refreshed.find((session) => session.sessionId === sessionId);
        if (created) {
          setActiveKey(created.key);
          await useTranscriptStore.getState().load(created.key);
        }
      }
      useRuntimeStore.getState().clearTimeline();
    })();
  }, [completionVersion, workspace, scanSessions, setActiveKey]);

  const submit = async () => {
    const prompt = draft.trim();
    if (!prompt || !workspace || busy) return;
    setDraft("");
    setAttachments([]);
    await startRuntime({
      workspaceRoot: workspace,
      sessionKey: activeKey,
      prompt,
      permissionMode,
      providerId: selectedProviderId,
      model: selectedModel,
    });
  };

  const newSession = () => {
    if (busy) return;
    setActiveKey(null);
    useTranscriptStore.getState().clear();
    useRuntimeStore.getState().reset();
  };

  const saveEdit = async (target: import("@/features/sessions/types").EditTarget, text: string) => {
    if (!activeKey || !detail) return false;
    const result = await useTranscriptStore.getState().edit(
      activeKey,
      target,
      detail.fileFingerprint,
      text,
      advancedAssistantEditing,
    );
    if (result) {
      await useTranscriptStore.getState().loadAudit(activeKey);
      return true;
    }
    return false;
  };

  const markUserScrollIntent = () => {
    userScrollPauseUntilRef.current = performance.now() + 900;
  };

  return (
    <aside
      className="material"
      style={{
        width: "100%",
        minWidth: 0,
        flex: 1,
        minHeight: 0,
        borderLeft: "1px solid var(--separator)",
        display: "flex",
        flexDirection: "column",
        overflow: "hidden",
        position: "relative",
      }}
    >
      <div style={{ display: "flex", alignItems: "center", paddingRight: 8 }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <SectionLabel>
            {busy
              ? runtime?.status === "waiting_permission"
                ? "Claude Code · 等待权限确认"
                : "Claude Code · 正在工作"
              : transcriptLoading
                ? "正在读取 Claude 原生会话…"
                : "Claude Code · 已就绪"}
          </SectionLabel>
        </div>
        <div style={{ display: "flex", alignItems: "center", paddingTop: 8 }}>
          {activeKey && (
            <IconButton label="编辑会话" onClick={() => setShowEdit((value) => !value)} active={showEdit}>
              <FilePenLine size={14} />
            </IconButton>
          )}
          {activeKey && (
            <IconButton
              label="修改记录与恢复"
              onClick={() => {
                setShowSnapshots((value) => !value);
                void Promise.all([
                  useTranscriptStore.getState().loadSnapshots(activeKey),
                  useTranscriptStore.getState().loadAudit(activeKey),
                ]);
              }}
              active={showSnapshots}
            >
              <History size={14} />
            </IconButton>
          )}
          <IconButton
            label="会话历史"
            onClick={() => {
              if (!useUI.getState().sidebarOpen) useUI.getState().toggleSidebar();
            }}
          >
            <History size={14} />
          </IconButton>
          <IconButton label="新建会话" onClick={newSession}>
            <SquarePen size={14} />
          </IconButton>
        </div>
      </div>

      <div
        onWheelCapture={markUserScrollIntent}
        onPointerDownCapture={markUserScrollIntent}
        style={{ position: "relative", flex: 1, minHeight: 0, overscrollBehaviorY: "contain" }}
      >
        <Virtuoso<UiMessage>
          ref={virtuosoRef}
          data={messages}
          atTopStateChange={setIsAtTop}
          atBottomStateChange={setIsAtBottom}
          followOutput={(atBottom) =>
            busy && atBottom && performance.now() >= userScrollPauseUntilRef.current ? "auto" : false
          }
          increaseViewportBy={{ top: 600, bottom: 600 }}
          computeItemKey={(_index, item) => item.key}
          className="material"
          style={{ height: "100%", overscrollBehaviorY: "contain", overflowAnchor: "none" }}
          components={
            {
              Header: () =>
                messages.length === 0 && !busy ? (
                  <div
                    style={{
                      fontSize: 12.5,
                      color: "var(--text-tertiary)",
                      padding: "16px 18px",
                      lineHeight: 1.6,
                    }}
                  >
                    选择左侧历史会话即可直接继续，或者在下方开始新的 Claude Code 任务。
                  </div>
                ) : null,
              Footer: () => (
                <div style={{ padding: "0 12px 6px" }}>
                  <AnimatePresence>
                    {waitingForFirstToken && (
                      <motion.div
                        initial={{ opacity: 0, y: -3 }}
                        animate={{ opacity: 1, y: 0 }}
                        exit={{ opacity: 0 }}
                        style={{ display: "flex", alignItems: "center", gap: 7, padding: "4px 2px" }}
                      >
                        <PiSpark size={13} />
                        <ShimmerText style={{ fontSize: 12, lineHeight: 1.45 }}>
                          Claude Code 正在思考…
                        </ShimmerText>
                      </motion.div>
                    )}
                  </AnimatePresence>
                </div>
              ),
            } satisfies Components<UiMessage>
          }
          itemContent={(index, item) => (
            <div style={{ padding: "0 12px" }}>
              <MessageBubble
                m={item.message}
                animateIn={index === messages.length - 1 && item.key.startsWith("live-")}
              />
            </div>
          )}
        />

        {messages.length > 0 && canScroll && (
          <button
            onClick={() =>
              virtuosoRef.current?.scrollTo({
                top: isAtBottom ? 0 : Number.MAX_SAFE_INTEGER,
                behavior: "smooth",
              })
            }
            style={{
              position: "absolute",
              right: 14,
              bottom: 12,
              zIndex: 2,
              border: "1px solid var(--separator)",
              borderRadius: 8,
              background: "var(--material-thick)",
              color: "var(--text-secondary)",
              padding: "5px 8px",
              fontSize: 10,
              cursor: "pointer",
            }}
          >
            {isAtBottom ? "顶部" : "底部"}
          </button>
        )}
      </div>

      {permissionItems.length > 0 && (
        <div style={{ padding: "0 12px 8px", display: "grid", gap: 7 }}>
          {permissionItems.map((item) => (
            <div
              key={item.requestId}
              className="material"
              style={{
                padding: "10px 11px",
                border: "1px solid var(--separator)",
                borderRadius: 10,
                background: "var(--bg-elevated)",
              }}
            >
              <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                <strong style={{ fontSize: 12 }}>{item.title || item.tool}</strong>
                <span style={{ fontSize: 10, color: "var(--text-tertiary)" }}>
                  {item.status === "pending" ? "请求权限" : item.status}
                </span>
                {item.status === "pending" && (
                  <div style={{ marginLeft: "auto", display: "flex", gap: 6 }}>
                    <button
                      onClick={() => void decidePermission(item.requestId, "deny", "用户在 Claudeix 中拒绝")}
                      style={permissionButtonStyle}
                    >
                      拒绝
                    </button>
                    <button
                      onClick={() => void decidePermission(item.requestId, "allow")}
                      style={{ ...permissionButtonStyle, background: "var(--accent)", color: "white" }}
                    >
                      允许
                    </button>
                  </div>
                )}
              </div>
              {item.description && (
                <div style={{ marginTop: 5, fontSize: 11, color: "var(--text-secondary)" }}>
                  {item.description}
                </div>
              )}
            </div>
          ))}
        </div>
      )}

      <ComposerInput
        draft={draft}
        setDraft={setDraft}
        attachments={attachments}
        setAttachments={setAttachments}
        streaming={busy}
        busy={busy || transcriptMutating}
        onSubmit={() => void submit()}
        onAbort={() => void stopRuntime()}
        onKeyDown={() => undefined}
        onPaste={() => undefined}
        queue={[] as QueueEntry[]}
        delivery={"followUp" as DeliveryMode}
        onDeliveryChange={() => undefined}
        allowMidTurnDelivery={false}
        modelControl={
          <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <ClaudeModelPicker compact />
            <select
              value={permissionMode}
              onChange={(event) => setPermissionMode(event.target.value)}
              disabled={busy}
              title="Claude Code 权限模式"
              style={{
                border: "none",
                outline: "none",
                background: "transparent",
                color: "var(--text-tertiary)",
                fontSize: 10.5,
                cursor: busy ? "default" : "pointer",
              }}
            >
              <option value="manual">询问</option>
              <option value="acceptEdits">接受编辑</option>
              <option value="plan">计划</option>
            </select>
          </div>
        }
      />

      {showEdit && activeKey && detail && (
        <div
          className="material-thick"
          style={{
            position: "absolute",
            inset: "36px 12px 12px auto",
            width: "min(560px, 78%)",
            zIndex: 30,
            border: "1px solid var(--separator)",
            borderRadius: 14,
            boxShadow: "var(--shadow-lg)",
            background: "var(--material-thick)",
            display: "grid",
            gridTemplateRows: "auto 1fr",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "10px 12px",
              borderBottom: "1px solid var(--separator)",
            }}
          >
            <strong style={{ fontSize: 12 }}>会话内容编辑</strong>
            <label style={{ marginLeft: "auto", fontSize: 10, color: "var(--text-secondary)" }}>
              <input
                type="checkbox"
                checked={advancedAssistantEditing}
                onChange={(event) => setAdvancedAssistantEditing(event.target.checked)}
              />{" "}
              高级编辑 Claude 回复
            </label>
            <IconButton label="关闭" onClick={() => setShowEdit(false)}>
              <X size={13} />
            </IconButton>
          </div>
          <div style={{ overflow: "auto" }}>
            <TranscriptTimeline
              detail={detail}
              allowAssistantEditing={advancedAssistantEditing}
              editingDisabled={busy}
              mutating={transcriptMutating}
              onEdit={saveEdit}
            />
          </div>
        </div>
      )}

      {showSnapshots && activeKey && (
        <SnapshotPanel
          snapshots={snapshots}
          auditEntries={auditEntries}
          mutating={transcriptMutating || busy}
          onClose={() => setShowSnapshots(false)}
          onRestore={(snapshot) => {
            if (!detail) return;
            void useTranscriptStore
              .getState()
              .restore(activeKey, snapshot.id, detail.fileFingerprint)
              .then(() => useTranscriptStore.getState().loadAudit(activeKey));
          }}
        />
      )}
    </aside>
  );
}

const permissionButtonStyle: React.CSSProperties = {
  border: "1px solid var(--separator)",
  borderRadius: 7,
  padding: "4px 9px",
  background: "var(--bg-base)",
  color: "var(--text-secondary)",
  fontSize: 10.5,
  cursor: "pointer",
};
