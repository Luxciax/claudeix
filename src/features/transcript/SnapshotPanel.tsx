"use client";

import { RotateCcw, X } from "lucide-react";

import type { BackupSnapshot } from "@/features/sessions/types";

function formatTime(value: number): string {
  try {
    return new Date(value).toLocaleString();
  } catch {
    return String(value);
  }
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(2)} MB`;
}

export function SnapshotPanel({
  snapshots,
  mutating,
  onClose,
  onRestore,
}: {
  snapshots: BackupSnapshot[];
  mutating: boolean;
  onClose: () => void;
  onRestore: (snapshot: BackupSnapshot) => void;
}) {
  return (
    <aside
      style={{
        position: "absolute",
        top: 0,
        right: 0,
        bottom: 0,
        width: "min(420px, 46vw)",
        zIndex: 20,
        display: "grid",
        gridTemplateRows: "56px 1fr",
        borderLeft: "1px solid var(--separator)",
        background: "var(--bg-base)",
        boxShadow: "-18px 0 40px rgba(0,0,0,0.12)",
      }}
    >
      <header
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          padding: "0 14px 0 16px",
          borderBottom: "1px solid var(--separator)",
        }}
      >
        <div>
          <div style={{ fontSize: 13, fontWeight: 700 }}>修改记录与恢复</div>
          <div style={{ marginTop: 2, fontSize: 10, color: "var(--text-secondary)" }}>
            每次编辑和恢复前都会保留完整 JSONL 快照
          </div>
        </div>
        <button
          onClick={onClose}
          aria-label="关闭恢复面板"
          style={{
            width: 30,
            height: 30,
            display: "grid",
            placeItems: "center",
            border: "1px solid var(--separator)",
            borderRadius: 8,
            background: "var(--bg-elevated)",
            color: "var(--text-primary)",
          }}
        >
          <X size={14} />
        </button>
      </header>

      <div style={{ overflow: "auto", padding: 14 }}>
        {snapshots.length === 0 ? (
          <div
            style={{
              padding: 18,
              border: "1px dashed var(--separator)",
              borderRadius: 12,
              color: "var(--text-secondary)",
              fontSize: 12,
              lineHeight: 1.6,
            }}
          >
            这个会话还没有 Claudeix 快照。第一次编辑消息时会自动创建。
          </div>
        ) : (
          <div style={{ display: "grid", gap: 10 }}>
            {snapshots.map((snapshot) => (
              <article
                key={snapshot.id}
                style={{
                  padding: 12,
                  border: "1px solid var(--separator)",
                  borderRadius: 12,
                  background: "var(--bg-elevated)",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <strong style={{ fontSize: 12 }}>
                    {snapshot.reason === "before_restore" ? "恢复操作前" : "编辑操作前"}
                  </strong>
                  <span
                    style={{
                      marginLeft: "auto",
                      fontSize: 10,
                      color: "var(--text-secondary)",
                    }}
                  >
                    {formatBytes(snapshot.fileSize)}
                  </span>
                </div>
                <div style={{ marginTop: 5, fontSize: 11, color: "var(--text-secondary)" }}>
                  {formatTime(snapshot.createdAt)}
                </div>
                {snapshot.target && (
                  <div style={{ marginTop: 5, fontSize: 11, color: "var(--text-secondary)" }}>
                    JSONL 第 {snapshot.target.lineIndex + 1} 行
                  </div>
                )}
                <div
                  title={snapshot.backupPath}
                  style={{
                    marginTop: 6,
                    fontSize: 10,
                    color: "var(--text-tertiary)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {snapshot.backupPath}
                </div>
                <button
                  onClick={() => onRestore(snapshot)}
                  disabled={mutating}
                  style={{
                    marginTop: 10,
                    width: "100%",
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "center",
                    gap: 7,
                    padding: "8px 10px",
                    border: "1px solid var(--separator)",
                    borderRadius: 8,
                    background: "var(--bg-base)",
                    color: "var(--text-primary)",
                    opacity: mutating ? 0.5 : 1,
                  }}
                >
                  <RotateCcw size={14} /> 恢复到这个完整快照
                </button>
              </article>
            ))}
          </div>
        )}
      </div>
    </aside>
  );
}
