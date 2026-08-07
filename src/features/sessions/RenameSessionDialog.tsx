"use client";

import { useEffect, useState } from "react";
import { Pencil, X } from "lucide-react";

export function RenameSessionDialog({
  currentTitle,
  mutating,
  onCancel,
  onConfirm,
}: {
  currentTitle: string;
  mutating: boolean;
  onCancel: () => void;
  onConfirm: (title: string) => void;
}) {
  const [title, setTitle] = useState(currentTitle);

  useEffect(() => setTitle(currentTitle), [currentTitle]);

  const normalized = title.trim();
  const unchanged = normalized === currentTitle.trim();

  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 90,
        display: "grid",
        placeItems: "center",
        padding: 24,
        background: "rgba(0,0,0,0.38)",
      }}
    >
      <section
        role="dialog"
        aria-modal="true"
        aria-label="重命名 Claude 会话"
        style={{
          width: "min(520px, 100%)",
          border: "1px solid var(--separator)",
          borderRadius: 16,
          background: "var(--bg-base)",
          boxShadow: "0 22px 70px rgba(0,0,0,0.28)",
          overflow: "hidden",
        }}
      >
        <header
          style={{
            height: 54,
            display: "flex",
            alignItems: "center",
            gap: 9,
            padding: "0 14px 0 16px",
            borderBottom: "1px solid var(--separator)",
          }}
        >
          <Pencil size={15} />
          <strong style={{ fontSize: 13 }}>重命名会话</strong>
          <button
            onClick={onCancel}
            disabled={mutating}
            aria-label="关闭"
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

        <div style={{ padding: 16 }}>
          <div
            style={{
              padding: 11,
              borderRadius: 10,
              background: "var(--bg-elevated)",
              color: "var(--text-secondary)",
              fontSize: 11,
              lineHeight: 1.55,
            }}
          >
            Claudeix 会按 Claude Code 原生 <code>/rename</code> 格式，把新的
            <code> custom-title </code>记录追加到原始 JSONL。写入前会自动保存完整快照。
          </div>
          <input
            autoFocus
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && normalized && !unchanged && !mutating) {
                event.preventDefault();
                onConfirm(normalized);
              }
              if (event.key === "Escape" && !mutating) onCancel();
            }}
            placeholder="会话名称"
            style={{
              marginTop: 14,
              width: "100%",
              boxSizing: "border-box",
              border: "1px solid var(--separator)",
              borderRadius: 10,
              background: "var(--bg-elevated)",
              color: "var(--text-primary)",
              padding: "10px 12px",
              outline: "none",
              font: "inherit",
              fontSize: 13,
            }}
          />
        </div>

        <footer
          style={{
            display: "flex",
            justifyContent: "flex-end",
            gap: 8,
            padding: "12px 16px 16px",
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
            onClick={() => onConfirm(normalized)}
            disabled={!normalized || unchanged || mutating}
            style={{
              padding: "8px 14px",
              border: "none",
              borderRadius: 9,
              background: "var(--accent)",
              color: "white",
              opacity: !normalized || unchanged || mutating ? 0.45 : 1,
            }}
          >
            {mutating ? "写入中…" : "保存名称"}
          </button>
        </footer>
      </section>
    </div>
  );
}
