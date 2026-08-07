"use client";

import { useEffect, useMemo, useState } from "react";
import { FolderOpen, History, Play, Plus, RefreshCw, Square, Terminal, Trash2 } from "lucide-react";

import { RuntimeTimeline } from "@/features/runtime/RuntimeTimeline";
import { useRuntimeStore } from "@/features/runtime/runtime-store";
import type { ExternalRuntimeInfo } from "@/features/runtime/types";
import { useSessionStore } from "@/features/sessions/session-store";
import type {
  BackupSnapshot,
  ClaudeStatus,
  EditRecoveryItem,
  EditTarget,
} from "@/features/sessions/types";
import { DeleteSessionDialog } from "@/features/trash/DeleteSessionDialog";
import { TrashPanel } from "@/features/trash/TrashPanel";
import type { TrashDeletePreview } from "@/features/trash/types";
import { RecoveryDialog } from "@/features/transcript/RecoveryDialog";
import { SnapshotPanel } from "@/features/transcript/SnapshotPanel";
import { TranscriptTimeline } from "@/features/transcript/TranscriptTimeline";
import { useTranscriptStore } from "@/features/transcript/transcript-store";
import { api, isTauriRuntime } from "@/lib/tauri-api";

export default function Home() {
  const [status, setStatus] = useState<ClaudeStatus>({
    installed: false,
    version: "检测中…",
    executablePath: null,
  });
  const [draft, setDraft] = useState("");
  const [permissionMode, setPermissionMode] = useState("manual");
  const [externalRuntime, setExternalRuntime] = useState<ExternalRuntimeInfo | null>(null);
  const [checkingExternal, setCheckingExternal] = useState(false);
  const [showSnapshots, setShowSnapshots] = useState(false);
  const [showTrash, setShowTrash] = useState(false);
  const [deletePreview, setDeletePreview] = useState<TrashDeletePreview | null>(null);
  const [trashMutating, setTrashMutating] = useState(false);
  const [advancedAssistantEditing, setAdvancedAssistantEditing] = useState(false);
  const [recoveryItems, setRecoveryItems] = useState<EditRecoveryItem[]>([]);
  const [recoveryResolvingId, setRecoveryResolvingId] = useState<string | null>(null);
  const [recoveryError, setRecoveryError] = useState<string | null>(null);

  const workspace = useSessionStore((state) => state.workspaceRoot);
  const sessions = useSessionStore((state) => state.sessions);
  const activeKey = useSessionStore((state) => state.activeKey);
  const sessionLoading = useSessionStore((state) => state.loading);
  const sessionError = useSessionStore((state) => state.error);
  const scanSessions = useSessionStore((state) => state.scan);
  const setActiveKey = useSessionStore((state) => state.setActiveKey);

  const detail = useTranscriptStore((state) => state.detail);
  const snapshots = useTranscriptStore((state) => state.snapshots);
  const auditEntries = useTranscriptStore((state) => state.auditEntries);
  const transcriptLoading = useTranscriptStore((state) => state.loading);
  const transcriptError = useTranscriptStore((state) => state.error);
  const transcriptMutating = useTranscriptStore((state) => state.mutating);
  const mutationError = useTranscriptStore((state) => state.mutationError);
  const loadTranscript = useTranscriptStore((state) => state.load);
  const loadSnapshots = useTranscriptStore((state) => state.loadSnapshots);
  const loadAudit = useTranscriptStore((state) => state.loadAudit);
  const editTranscript = useTranscriptStore((state) => state.edit);
  const restoreTranscript = useTranscriptStore((state) => state.restore);
  const clearTranscript = useTranscriptStore((state) => state.clear);

  const runtimeSnapshot = useRuntimeStore((state) => state.snapshot);
  const runtimeTimeline = useRuntimeStore((state) => state.timeline);
  const completionVersion = useRuntimeStore((state) => state.completionVersion);
  const bindRuntimeEvents = useRuntimeStore((state) => state.bindEvents);
  const startRuntime = useRuntimeStore((state) => state.start);
  const stopRuntime = useRuntimeStore((state) => state.stop);
  const decidePermission = useRuntimeStore((state) => state.decidePermission);
  const clearRuntimeTimeline = useRuntimeStore((state) => state.clearTimeline);
  const resetRuntime = useRuntimeStore((state) => state.reset);

  const running =
    runtimeSnapshot?.status === "starting" ||
    runtimeSnapshot?.status === "running" ||
    runtimeSnapshot?.status === "waiting_permission" ||
    runtimeSnapshot?.status === "stopping";
  const runtimeLabel = runtimeSnapshot
    ? runtimeSnapshot.status === "starting"
      ? "Claude Code 启动中…"
      : runtimeSnapshot.status === "running"
        ? `Claude Code 运行中 · PID ${runtimeSnapshot.pid}`
        : runtimeSnapshot.status === "waiting_permission"
          ? `Claude Code 等待权限确认 · PID ${runtimeSnapshot.pid}`
          : runtimeSnapshot.status === "stopping"
            ? `正在停止 Claude Code · PID ${runtimeSnapshot.pid}`
            : runtimeSnapshot.status === "failed"
              ? `Claude Code 运行失败${runtimeSnapshot.error ? ` · ${runtimeSnapshot.error}` : ""}`
              : "运行完成，正在与磁盘会话对账"
    : externalRuntime?.active
      ? `外部 Claude Code 正在使用此会话 · PID ${externalRuntime.pid ?? "未知"}`
      : null;

  const activeSummary = useMemo(
    () => sessions.find((session) => session.key === activeKey) ?? null,
    [sessions, activeKey],
  );
  const activeTitle = detail?.summary.title ?? activeSummary?.title ?? "新会话";

  const selectSession = async (sessionKey: string) => {
    if (running) return;
    setActiveKey(sessionKey);
    resetRuntime();
    await Promise.all([
      loadTranscript(sessionKey),
      loadSnapshots(sessionKey),
      loadAudit(sessionKey),
    ]);
  };

  const newSession = () => {
    if (running) return;
    setActiveKey(null);
    clearTranscript();
    resetRuntime();
    setShowSnapshots(false);
  };

  useEffect(() => {
    if (!isTauriRuntime()) {
      setStatus({ installed: false, version: "浏览器预览模式", executablePath: null });
      return;
    }

    void api.claudeStatus().then(setStatus);
    void api
      .listTranscriptRecovery()
      .then((items) => {
        setRecoveryItems(items);
        setRecoveryError(null);
      })
      .catch((error) => setRecoveryError(`无法检查未完成会话事务：${String(error)}`));
    void api.workspaceRoot().then((root) => {
      void useSessionStore.getState().scan(root);
    });
    let dispose: () => void = () => {};
    void bindRuntimeEvents().then((unbind) => {
      dispose = unbind;
    });
    return () => dispose();
  }, []);

  useEffect(() => {
    if (completionVersion === 0 || !workspace) return;
    let cancelled = false;
    const reconcile = async () => {
      const previousKey = useSessionStore.getState().activeKey;
      const sessionId = useRuntimeStore.getState().snapshot?.sessionId;
      const refreshed = await useSessionStore.getState().scan(workspace, true);
      if (cancelled) return;
      if (previousKey && refreshed.some((session) => session.key === previousKey)) {
        useSessionStore.getState().setActiveKey(previousKey);
        await useTranscriptStore.getState().refresh(previousKey);
        clearRuntimeTimeline();
        return;
      }
      const created = refreshed.find((session) => session.sessionId === sessionId);
      if (created) {
        useSessionStore.getState().setActiveKey(created.key);
        await useTranscriptStore.getState().load(created.key);
        clearRuntimeTimeline();
      }
    };
    void reconcile();
    return () => {
      cancelled = true;
    };
  }, [completionVersion, workspace, clearRuntimeTimeline]);

  useEffect(() => {
    let cancelled = false;
    if (!activeKey || running || !isTauriRuntime()) {
      setExternalRuntime(null);
      return;
    }
    void api
      .probeExternalRuntime(activeKey)
      .then((info) => {
        if (!cancelled) setExternalRuntime(info);
      })
      .catch(() => {
        if (!cancelled) setExternalRuntime(null);
      });
    return () => {
      cancelled = true;
    };
  }, [activeKey, running, completionVersion]);

  const pickWorkspace = async () => {
    if (running) return;
    const picked = await api.pickWorkspace();
    if (!picked) return;
    const root = await api.openWorkspace(picked);
    newSession();
    await scanSessions(root, true);
  };

  const refreshExternalRuntime = async () => {
    if (!activeKey || !isTauriRuntime()) {
      setExternalRuntime(null);
      return null;
    }
    setCheckingExternal(true);
    try {
      const info = await api.probeExternalRuntime(activeKey);
      setExternalRuntime(info);
      return info;
    } catch {
      setExternalRuntime(null);
      return null;
    } finally {
      setCheckingExternal(false);
    }
  };

  const send = async () => {
    const prompt = draft.trim();
    if (!prompt || running || !workspace) return;
    if (activeKey) {
      const external = await refreshExternalRuntime();
      if (external?.active) return;
    }
    setDraft("");
    await startRuntime({
      workspaceRoot: workspace,
      sessionKey: activeKey,
      prompt,
      permissionMode,
    });
  };

  const saveTranscriptEdit = async (target: EditTarget, newText: string) => {
    if (!activeKey || !detail || running || externalRuntime?.active) return false;
    const confirmed = window.confirm(
      "这会直接覆盖 Claude Code 原始 JSONL。\n\n写入前 Claudeix 会先保存当前完整会话快照，之后可在“修改记录与恢复”中还原。\n\n确认继续？",
    );
    if (!confirmed) return false;
    const result = await editTranscript(
      activeKey,
      target,
      detail.fileFingerprint,
      newText,
      advancedAssistantEditing,
    );
    if (!result) return false;
    await loadAudit(activeKey);
    if (workspace) await scanSessions(workspace, true);
    return true;
  };

  const restoreSnapshot = async (snapshot: BackupSnapshot) => {
    if (!activeKey || !detail || running || externalRuntime?.active) return;
    const confirmed = window.confirm(
      `确认恢复到 ${new Date(snapshot.createdAt).toLocaleString()} 的完整会话快照？\n\n这会覆盖当前 Claude 原始 JSONL；覆盖前当前版本也会再次自动备份，因此恢复操作本身仍可撤销。`,
    );
    if (!confirmed) return;
    const result = await restoreTranscript(
      activeKey,
      snapshot.id,
      detail.fileFingerprint,
    );
    if (!result) return;
    await Promise.all([loadSnapshots(activeKey), loadAudit(activeKey)]);
    if (workspace) await scanSessions(workspace, true);
  };

  const resolveStartupRecovery = async (
    item: EditRecoveryItem,
    action: "keep_current" | "restore_backup",
  ) => {
    const description =
      action === "keep_current"
        ? "保留当前 JSONL，并将这次未完成事务正式提交到审计记录。"
        : "用操作前的自动备份覆盖当前原始 JSONL。覆盖前如果当前文件存在，Claudeix 会先再保存一份恢复前快照。";
    if (
      !window.confirm(
        `${description}\n\n会话：${item.sessionId}\n路径：${item.sourcePath}\n\n确认继续？`,
      )
    ) {
      return;
    }
    setRecoveryResolvingId(item.transactionId);
    setRecoveryError(null);
    try {
      await api.resolveTranscriptRecovery({ transactionId: item.transactionId, action });
      const remaining = await api.listTranscriptRecovery();
      setRecoveryItems(remaining);
      if (workspace) await scanSessions(workspace, true);
      if (activeKey) {
        await Promise.all([
          useTranscriptStore.getState().refresh(activeKey),
          useTranscriptStore.getState().loadSnapshots(activeKey),
          useTranscriptStore.getState().loadAudit(activeKey),
        ]);
      }
    } catch (error) {
      setRecoveryError(`事务恢复失败：${String(error)}`);
    } finally {
      setRecoveryResolvingId(null);
    }
  };

  const previewSessionDelete = async () => {
    if (!activeKey || running || transcriptMutating) return;
    try {
      const preview = await api.previewTrashDelete(activeKey);
      setDeletePreview(preview);
    } catch (error) {
      window.alert(`无法生成删除预览：${String(error)}`);
    }
  };

  const confirmSessionDelete = async () => {
    if (!deletePreview) return;
    setTrashMutating(true);
    try {
      await api.moveSessionToTrash({
        sessionKey: deletePreview.sessionKey,
        expectedPreviewFingerprint: deletePreview.previewFingerprint,
      });
      setDeletePreview(null);
      setActiveKey(null);
      clearTranscript();
      resetRuntime();
      setShowSnapshots(false);
      if (workspace) await scanSessions(workspace, true);
      setShowTrash(true);
    } catch (error) {
      window.alert(`移入回收站失败：${String(error)}`);
    } finally {
      setTrashMutating(false);
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

        <button
          onClick={() => {
            setShowSnapshots(false);
            setShowTrash(true);
          }}
          disabled={running || transcriptMutating}
          style={{
            marginTop: 2,
            width: "100%",
            display: "flex",
            gap: 8,
            alignItems: "center",
            padding: "9px 12px",
            borderRadius: 10,
            border: "none",
            background: showTrash ? "var(--bg-selected)" : "transparent",
            color: "var(--text-primary)",
            opacity: running || transcriptMutating ? 0.55 : 1,
          }}
        >
          <Trash2 size={15} /> 回收站
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

      <section
        style={{
          minWidth: 0,
          position: "relative",
          display: "grid",
          gridTemplateRows: "56px 1fr auto",
        }}
      >
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
                color:
                  runtimeSnapshot?.status === "failed"
                    ? "var(--danger)"
                    : running || externalRuntime?.active
                      ? "var(--warning)"
                      : status.installed
                        ? "var(--success)"
                        : "var(--warning)",
              }}
            >
              {runtimeLabel ?? (transcriptLoading
                ? "正在读取原生会话…"
                : status.installed
                  ? detail?.hasBranches
                    ? "Claude Code 已就绪 · 此会话包含分支"
                    : "Claude Code 已就绪"
                  : "未检测到 Claude Code")}
            </div>
          </div>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            {activeKey && (
              <label
                title="允许修改 Claude 可见文本；thinking、签名与工具记录仍不可编辑"
                style={{
                  height: 32,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "0 8px",
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: advancedAssistantEditing
                    ? "var(--warning-soft)"
                    : "var(--bg-elevated)",
                  color: "var(--text-primary)",
                  fontSize: 10,
                }}
              >
                <input
                  type="checkbox"
                  checked={advancedAssistantEditing}
                  onChange={(event) => setAdvancedAssistantEditing(event.target.checked)}
                  disabled={running || transcriptMutating}
                />
                高级编辑 Claude 回复
              </label>
            )}
            {activeKey && (
              <button
                onClick={() => {
                  setShowTrash(false);
                  setShowSnapshots((current) => !current);
                  void Promise.all([loadSnapshots(activeKey), loadAudit(activeKey)]);
                }}
                disabled={running}
                title="查看修改记录与完整快照"
                style={{
                  height: 32,
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  padding: "0 9px",
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: showSnapshots ? "var(--bg-selected)" : "var(--bg-elevated)",
                  color: "var(--text-primary)",
                  opacity: running ? 0.55 : 1,
                  fontSize: 11,
                }}
              >
                <History size={14} /> 恢复
              </button>
            )}
            {activeKey && (
              <button
                onClick={() => void previewSessionDelete()}
                disabled={running || transcriptMutating}
                title="将此会话和明确关联的子代理文件移入 Claudeix 回收站"
                style={{
                  width: 32,
                  height: 32,
                  display: "grid",
                  placeItems: "center",
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: "var(--bg-elevated)",
                  color: "var(--danger)",
                  opacity: running || transcriptMutating ? 0.5 : 1,
                }}
              >
                <Trash2 size={14} />
              </button>
            )}
            {externalRuntime?.active && (
              <button
                onClick={() => void refreshExternalRuntime()}
                disabled={checkingExternal}
                title="重新检测外部 Claude Code 进程"
                style={{
                  width: 32,
                  height: 32,
                  display: "grid",
                  placeItems: "center",
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: "var(--bg-elevated)",
                  color: "var(--text-primary)",
                }}
              >
                <RefreshCw size={14} className={checkingExternal ? "animate-spin" : undefined} />
              </button>
            )}
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
              <option value="manual">询问权限</option>
              <option value="acceptEdits">接受编辑</option>
              <option value="plan">计划模式</option>
            </select>
          </div>
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
          {mutationError && (
            <div
              style={{
                marginTop: transcriptError ? 10 : 0,
                marginBottom: 12,
                padding: 12,
                borderRadius: 10,
                background: "var(--warning-soft)",
                color: "var(--danger)",
                fontSize: 12,
              }}
            >
              会话写入/恢复失败：{mutationError}
            </div>
          )}
          {!detail && runtimeTimeline.length === 0 && !transcriptLoading ? (
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
              {detail && (
                <TranscriptTimeline
                  detail={detail}
                  allowAssistantEditing={advancedAssistantEditing}
                  editingDisabled={running || Boolean(externalRuntime?.active)}
                  mutating={transcriptMutating}
                  onEdit={saveTranscriptEdit}
                />
              )}
              <RuntimeTimeline
                items={runtimeTimeline}
                onPermissionDecision={(requestId, decision, message) =>
                  void decidePermission(requestId, decision, message)
                }
              />
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
              placeholder={
                externalRuntime?.active
                  ? "外部 Claude Code 正在使用此会话"
                  : workspace
                    ? "向 Claude Code 描述任务…"
                    : "先选择工作区"
              }
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
                onClick={() => void stopRuntime()}
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
                disabled={!draft.trim() || !workspace || !status.installed || checkingExternal}
                style={{
                  width: 38,
                  height: 38,
                  borderRadius: 10,
                  border: "none",
                  background: "var(--accent)",
                  color: "white",
                  opacity: !draft.trim() || checkingExternal ? 0.45 : 1,
                }}
              >
                <Play size={16} />
              </button>
            )}
          </div>
        </div>

        {showSnapshots && activeKey && (
          <SnapshotPanel
            snapshots={snapshots}
            auditEntries={auditEntries}
            mutating={transcriptMutating || running || Boolean(externalRuntime?.active)}
            onClose={() => setShowSnapshots(false)}
            onRestore={(snapshot) => void restoreSnapshot(snapshot)}
          />
        )}
        {showTrash && (
          <TrashPanel
            onClose={() => setShowTrash(false)}
            onRestored={() => {
              if (workspace) void scanSessions(workspace, true);
            }}
          />
        )}
      </section>
      {deletePreview && (
        <DeleteSessionDialog
          preview={deletePreview}
          mutating={trashMutating}
          onCancel={() => setDeletePreview(null)}
          onConfirm={() => void confirmSessionDelete()}
        />
      )}
      {(recoveryItems.length > 0 || recoveryError) && (
        <RecoveryDialog
          items={recoveryItems}
          resolvingId={recoveryResolvingId}
          error={recoveryError}
          onResolve={(item, action) => void resolveStartupRecovery(item, action)}
        />
      )}
    </main>
  );
}
