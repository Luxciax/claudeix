"use client";

import { ArchiveRestore, RefreshCw, ShieldAlert, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { createPortal } from "react-dom";

import type {
  EngineTrashItemSummary,
  EngineTrashPurgePreview,
  EngineTrashRestorePreview,
  EngineTrashRestoreResult,
} from "@/features/engines/types";
import { api } from "@/lib/tauri-api";

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(2)} MB`;
}

function stateLabel(state: string): string {
  if (state === "free") return "原路径空闲";
  if (state === "same") return "原路径已有相同内容";
  if (state === "different") return "原路径存在不同内容";
  if (state === "corrupt_payload") return "回收站文件哈希异常";
  if (state === "missing_payload") return "回收站文件缺失";
  return state;
}

export function EngineTrashPanel({
  engineId,
  engineLabel,
  onClose,
  onRestored,
}: {
  engineId: string;
  engineLabel: string;
  onClose: () => void;
  onRestored: (result: EngineTrashRestoreResult) => void;
}) {
  const [items, setItems] = useState<EngineTrashItemSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [restorePreview, setRestorePreview] = useState<EngineTrashRestorePreview | null>(null);
  const [purgePreview, setPurgePreview] = useState<EngineTrashPurgePreview | null>(null);
  const [purgeConfirmation, setPurgeConfirmation] = useState("");
  const [loading, setLoading] = useState(false);
  const [mutating, setMutating] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await api.listEngineTrash(engineId);
      setItems(next);
      if (selectedId && !next.some((item) => item.trashId === selectedId)) {
        setSelectedId(null);
        setRestorePreview(null);
        setPurgePreview(null);
      }
    } catch (reason) {
      setError(String(reason));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [engineId]);

  const selected = useMemo(
    () => items.find((item) => item.trashId === selectedId) ?? null,
    [items, selectedId],
  );

  const choose = async (item: EngineTrashItemSummary) => {
    setSelectedId(item.trashId);
    setPurgePreview(null);
    setPurgeConfirmation("");
    setError(null);
    try {
      setRestorePreview(await api.previewEngineTrashRestore(item.trashId));
    } catch (reason) {
      setError(String(reason));
    }
  };

  const restore = async (strategy: "safe" | "exchange") => {
    if (!selected || !restorePreview) return;
    setMutating(true);
    setError(null);
    try {
      const result = await api.restoreEngineTrash({
        trashId: selected.trashId,
        strategy,
        expectedPreviewFingerprint: restorePreview.previewFingerprint,
      });
      await load();
      setRestorePreview(await api.previewEngineTrashRestore(selected.trashId).catch(() => null));
      onRestored(result);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setMutating(false);
    }
  };

  const openPurge = async () => {
    if (!selected) return;
    setPurgeConfirmation("");
    setError(null);
    try {
      setPurgePreview(await api.previewEngineTrashPurge(selected.trashId));
    } catch (reason) {
      setError(String(reason));
    }
  };

  const purge = async () => {
    if (!selected || !purgePreview) return;
    setMutating(true);
    setError(null);
    try {
      await api.purgeEngineTrash({
        trashId: selected.trashId,
        confirmationText: purgeConfirmation,
        expectedPreviewFingerprint: purgePreview.previewFingerprint,
      });
      setSelectedId(null);
      setRestorePreview(null);
      setPurgePreview(null);
      setPurgeConfirmation("");
      await load();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setMutating(false);
    }
  };

  if (typeof document === "undefined") return null;

  return createPortal(
    <aside
      style={{
        position: "fixed",
        top: 0,
        right: 0,
        bottom: 0,
        width: "min(720px, 68vw)",
        zIndex: 30,
        display: "grid",
        gridTemplateRows: "56px 1fr",
        borderLeft: "1px solid var(--separator)",
        background: "var(--bg-base)",
        boxShadow: "-18px 0 40px rgba(0,0,0,0.14)",
      }}
    >
      <header
        style={{
          display: "flex",
          alignItems: "center",
          gap: 9,
          padding: "0 14px 0 16px",
          borderBottom: "1px solid var(--separator)",
        }}
      >
        <Trash2 size={16} />
        <div>
          <div style={{ fontSize: 13, fontWeight: 700 }}>{engineLabel} 回收站</div>
          <div style={{ fontSize: 10, color: "var(--text-secondary)" }}>
            源会话已真正移出 Agent 原始目录；默认永久保留
          </div>
        </div>
        <button
          onClick={() => void load()}
          disabled={loading || mutating}
          title="刷新回收站"
          style={iconButtonStyle}
        >
          <RefreshCw size={14} className={loading ? "animate-spin" : undefined} />
        </button>
        <button onClick={onClose} aria-label="关闭回收站" style={iconButtonStyle}>
          <X size={14} />
        </button>
      </header>

      <div style={{ minHeight: 0, display: "grid", gridTemplateColumns: "260px 1fr" }}>
        <div style={{ overflow: "auto", borderRight: "1px solid var(--separator)", padding: 10 }}>
          {items.length === 0 ? (
            <div style={{ padding: 12, fontSize: 12, color: "var(--text-secondary)" }}>
              {engineLabel} 回收站为空。
            </div>
          ) : (
            <div style={{ display: "grid", gap: 5 }}>
              {items.map((item) => (
                <button
                  key={item.trashId}
                  onClick={() => void choose(item)}
                  style={{
                    width: "100%",
                    textAlign: "left",
                    padding: "10px 11px",
                    border: "none",
                    borderRadius: 10,
                    background: selectedId === item.trashId ? "var(--bg-selected)" : "transparent",
                    color: "var(--text-primary)",
                  }}
                >
                  <div style={{ fontSize: 12, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                    {item.title}
                  </div>
                  <div style={{ marginTop: 3, fontSize: 10, color: "var(--text-secondary)" }}>
                    {item.status} · {item.totalFiles} 文件 · {formatBytes(item.totalSize)}
                  </div>
                  <div style={{ marginTop: 2, fontSize: 10, color: "var(--text-tertiary)" }}>
                    {new Date(item.deletedAt).toLocaleString()}
                  </div>
                </button>
              ))}
            </div>
          )}
        </div>

        <div style={{ overflow: "auto", padding: 14 }}>
          {error && (
            <div style={{ marginBottom: 12, padding: 10, borderRadius: 9, background: "var(--warning-soft)", color: "var(--danger)", fontSize: 11 }}>
              {error}
            </div>
          )}
          {!selected ? (
            <div style={{ color: "var(--text-secondary)", fontSize: 12 }}>
              选择一个回收站会话查看恢复状态。
            </div>
          ) : (
            <>
              <div style={{ fontSize: 15, fontWeight: 700 }}>{selected.title}</div>
              <div style={{ marginTop: 4, fontSize: 10, color: "var(--text-secondary)" }}>
                {selected.engineId} · sessionId：{selected.nativeSessionId}
              </div>
              <div style={{ marginTop: 2, fontSize: 10, color: "var(--text-secondary)", wordBreak: "break-all" }}>
                {selected.workspaceRoot}
              </div>

              {restorePreview && (
                <>
                  <div style={{ marginTop: 14, display: "grid", gap: 7 }}>
                    {restorePreview.files.map((file) => (
                      <article
                        key={file.originalPath}
                        style={{
                          padding: 10,
                          border: `1px solid ${file.state === "different" ? "var(--warning)" : file.state.includes("corrupt") || file.state.includes("missing") ? "var(--danger)" : "var(--separator)"}`,
                          borderRadius: 10,
                          background: "var(--bg-elevated)",
                        }}
                      >
                        <div style={{ display: "flex", gap: 8, fontSize: 10 }}>
                          <strong>{stateLabel(file.state)}</strong>
                          <span style={{ marginLeft: "auto", color: "var(--text-secondary)" }}>
                            {formatBytes(file.size)}
                          </span>
                        </div>
                        <div style={{ marginTop: 5, fontSize: 10, color: "var(--text-secondary)", wordBreak: "break-all" }}>
                          {file.originalPath}
                        </div>
                      </article>
                    ))}
                  </div>

                  <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
                    {restorePreview.canRestoreSafely && selected.status !== "restored" && (
                      <button onClick={() => void restore("safe")} disabled={mutating} style={primaryButtonStyle}>
                        <ArchiveRestore size={14} /> 恢复到原路径
                      </button>
                    )}
                    {restorePreview.hasConflicts && !restorePreview.corrupt && selected.status !== "restored" && (
                      <button onClick={() => void restore("exchange")} disabled={mutating} style={warningButtonStyle}>
                        <ShieldAlert size={14} /> 交换恢复并保留当前版本
                      </button>
                    )}
                  </div>
                </>
              )}

              <div style={{ marginTop: 18, paddingTop: 14, borderTop: "1px solid var(--separator)" }}>
                {!purgePreview ? (
                  <button onClick={() => void openPurge()} disabled={mutating} style={purgeButtonStyle}>
                    永久删除此回收站条目…
                  </button>
                ) : (
                  <div style={{ padding: 11, border: "1px solid var(--danger)", borderRadius: 10, background: "var(--warning-soft)" }}>
                    <div style={{ fontSize: 12, fontWeight: 700, color: "var(--danger)" }}>
                      永久删除后无法恢复
                    </div>
                    <div style={{ marginTop: 5, fontSize: 10, color: "var(--text-secondary)" }}>
                      将永久删除 {purgePreview.totalFiles} 个回收站文件，共 {formatBytes(purgePreview.totalSize)}。
                    </div>
                    <div style={{ marginTop: 8, maxHeight: 150, overflow: "auto", display: "grid", gap: 4 }}>
                      {purgePreview.files.map((file) => (
                        <div key={file.path} style={{ fontSize: 9, color: "var(--text-secondary)", wordBreak: "break-all" }}>
                          {file.path} · {formatBytes(file.size)}
                        </div>
                      ))}
                    </div>
                    <div style={{ marginTop: 9, fontSize: 10, color: "var(--text-secondary)" }}>
                      输入：<strong>{purgePreview.confirmationText}</strong>
                    </div>
                    <input
                      value={purgeConfirmation}
                      onChange={(event) => setPurgeConfirmation(event.target.value)}
                      disabled={mutating}
                      style={{ width: "100%", marginTop: 7, padding: "8px 9px", border: "1px solid var(--separator)", borderRadius: 8, background: "var(--bg-base)", color: "var(--text-primary)" }}
                    />
                    <div style={{ marginTop: 8, display: "flex", gap: 7 }}>
                      <button
                        onClick={() => {
                          setPurgePreview(null);
                          setPurgeConfirmation("");
                        }}
                        disabled={mutating}
                        style={secondaryButtonStyle}
                      >
                        取消
                      </button>
                      <button
                        onClick={() => void purge()}
                        disabled={mutating || purgeConfirmation !== purgePreview.confirmationText}
                        style={{
                          ...dangerButtonStyle,
                          opacity: mutating || purgeConfirmation !== purgePreview.confirmationText ? 0.5 : 1,
                        }}
                      >
                        确认永久删除
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </aside>,
    document.body,
  );
}

const iconButtonStyle: React.CSSProperties = {
  marginLeft: "auto",
  width: 30,
  height: 30,
  display: "grid",
  placeItems: "center",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-elevated)",
  color: "var(--text-primary)",
};

const primaryButtonStyle: React.CSSProperties = {
  flex: 1,
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  gap: 7,
  padding: "9px 10px",
  border: "none",
  borderRadius: 9,
  background: "var(--accent)",
  color: "white",
};

const warningButtonStyle: React.CSSProperties = {
  ...primaryButtonStyle,
  border: "1px solid var(--warning)",
  background: "var(--warning-soft)",
  color: "var(--text-primary)",
};

const purgeButtonStyle: React.CSSProperties = {
  width: "100%",
  padding: "8px 10px",
  border: "1px solid var(--danger)",
  borderRadius: 9,
  background: "transparent",
  color: "var(--danger)",
};

const secondaryButtonStyle: React.CSSProperties = {
  flex: 1,
  padding: "8px 9px",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-elevated)",
  color: "var(--text-primary)",
};

const dangerButtonStyle: React.CSSProperties = {
  flex: 1,
  padding: "8px 9px",
  border: "none",
  borderRadius: 8,
  background: "var(--danger)",
  color: "white",
};
