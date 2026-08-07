"use client";

import { ArchiveX, X } from "lucide-react";

import type { TrashDeletePreview } from "./types";

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / 1024 / 1024).toFixed(2)} MB`;
}

function purposeLabel(value: string): string {
  if (value === "main_transcript") return "主会话 JSONL";
  if (value === "subagent_transcript") return "子代理会话";
  return "会话关联文件";
}

export function DeleteSessionDialog({
  preview,
  mutating,
  onCancel,
  onConfirm,
}: {
  preview: TrashDeletePreview;
  mutating: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const blocked = preview.runtimeActive || preview.externalRuntimeActive;
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 60,
        display: "grid",
        placeItems: "center",
        padding: 24,
        background: "rgba(0,0,0,0.38)",
      }}
    >
      <section
        style={{
          width: "min(760px, 92vw)",
          maxHeight: "82vh",
          display: "grid",
          gridTemplateRows: "auto 1fr auto",
          border: "1px solid var(--separator)",
          borderRadius: 16,
          background: "var(--bg-base)",
          boxShadow: "0 28px 80px rgba(0,0,0,0.24)",
          overflow: "hidden",
        }}
      >
        <header
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "15px 17px",
            borderBottom: "1px solid var(--separator)",
          }}
        >
          <ArchiveX size={18} />
          <div>
            <div style={{ fontSize: 14, fontWeight: 700 }}>将会话移入 Claudeix 回收站</div>
            <div style={{ marginTop: 2, fontSize: 11, color: "var(--text-secondary)" }}>
              不会永久删除。恢复时会校验 SHA-256 并回到原路径。
            </div>
          </div>
          <button
            onClick={onCancel}
            disabled={mutating}
            aria-label="取消删除"
            style={{
              marginLeft: "auto",
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

        <div style={{ overflow: "auto", padding: 17 }}>
          <div style={{ fontSize: 14, fontWeight: 650 }}>{preview.title}</div>
          <div style={{ marginTop: 4, fontSize: 11, color: "var(--text-secondary)" }}>
            sessionId：{preview.sessionId}
          </div>
          <div style={{ marginTop: 2, fontSize: 11, color: "var(--text-secondary)" }}>
            工作区：{preview.workspaceRoot}
          </div>
          <div
            style={{
              marginTop: 14,
              padding: 10,
              borderRadius: 10,
              background: blocked ? "var(--warning-soft)" : "var(--bg-elevated)",
              color: blocked ? "var(--warning)" : "var(--text-secondary)",
              fontSize: 12,
            }}
          >
            共 {preview.totalFiles} 个文件，{formatBytes(preview.totalSize)}。
            {preview.runtimeActive && " 当前 Claudeix 会话仍在运行，必须先停止。"}
            {preview.externalRuntimeActive &&
              ` 外部 Claude Code PID ${preview.externalRuntimePid ?? "未知"} 正在占用，暂时禁止删除。`}
          </div>

          <div style={{ marginTop: 14, display: "grid", gap: 8 }}>
            {preview.files.map((file) => (
              <article
                key={file.originalPath}
                style={{
                  padding: 10,
                  border: "1px solid var(--separator)",
                  borderRadius: 10,
                  background: "var(--bg-elevated)",
                }}
              >
                <div style={{ display: "flex", gap: 8, fontSize: 11 }}>
                  <strong>{purposeLabel(file.purpose)}</strong>
                  <span style={{ marginLeft: "auto", color: "var(--text-secondary)" }}>
                    {formatBytes(file.size)}
                  </span>
                </div>
                <div
                  style={{
                    marginTop: 6,
                    fontSize: 10,
                    color: "var(--text-secondary)",
                    wordBreak: "break-all",
                  }}
                >
                  {file.originalPath}
                </div>
              </article>
            ))}
          </div>
        </div>

        <footer
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 9,
            padding: "13px 17px",
            borderTop: "1px solid var(--separator)",
          }}
        >
          <button
            onClick={onCancel}
            disabled={mutating}
            style={{
              padding: "8px 13px",
              border: "1px solid var(--separator)",
              borderRadius: 9,
              background: "var(--bg-elevated)",
              color: "var(--text-primary)",
            }}
          >
            取消
          </button>
          <button
            onClick={onConfirm}
            disabled={blocked || mutating}
            style={{
              padding: "8px 13px",
              border: "none",
              borderRadius: 9,
              background: "var(--danger)",
              color: "white",
              opacity: blocked || mutating ? 0.5 : 1,
            }}
          >
            {mutating ? "正在移动…" : "确认移入回收站"}
          </button>
        </footer>
      </section>
    </div>
  );
}
