"use client";

import { useState } from "react";
import {
  AlertTriangle,
  Check,
  CheckCircle2,
  LoaderCircle,
  ShieldAlert,
  Wrench,
  X,
} from "lucide-react";

import type { RuntimeTimelineItem } from "./types";

function display(value: unknown): string {
  if (typeof value === "string") return value;
  if (value == null) return "";
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export function RuntimeTimeline({
  items,
  onPermissionDecision,
}: {
  items: RuntimeTimelineItem[];
  onPermissionDecision: (
    requestId: string,
    decision: "allow" | "deny",
    message?: string,
  ) => void;
}) {
  const [denyMessages, setDenyMessages] = useState<Record<string, string>>({});

  return items.map((item) => {
    if (item.type === "permission") {
      const pending = item.status === "pending";
      return (
        <article
          key={item.id}
          style={{
            marginBottom: 18,
            padding: "14px 15px",
            borderRadius: 14,
            border: "1px solid var(--warning)",
            background: "var(--warning-soft)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <ShieldAlert size={17} />
            <strong>{item.title || `${item.tool} 请求权限`}</strong>
            <span style={{ marginLeft: "auto", fontSize: 12, color: "var(--text-secondary)" }}>
              {item.status === "pending"
                ? "等待确认"
                : item.status === "allowed"
                  ? "已允许"
                  : item.status === "denied"
                    ? "已拒绝"
                    : "响应失败"}
            </span>
          </div>
          {item.description && (
            <div style={{ marginTop: 8, fontSize: 13, lineHeight: 1.5 }}>
              {item.description}
            </div>
          )}
          {item.decisionReason && (
            <div style={{ marginTop: 6, fontSize: 12, color: "var(--text-secondary)" }}>
              {item.decisionReason}
            </div>
          )}
          <pre
            style={{
              marginTop: 10,
              maxHeight: 260,
              overflow: "auto",
              whiteSpace: "pre-wrap",
              fontSize: 12,
            }}
          >
            {display(item.input)}
          </pre>
          {pending && (
            <>
              <input
                value={denyMessages[item.requestId] ?? ""}
                onChange={(event) =>
                  setDenyMessages((current) => ({
                    ...current,
                    [item.requestId]: event.target.value,
                  }))
                }
                placeholder="拒绝原因（可选）"
                style={{
                  width: "100%",
                  marginTop: 10,
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: "var(--bg-elevated)",
                  color: "var(--text-primary)",
                  padding: "8px 10px",
                }}
              />
              <div style={{ display: "flex", justifyContent: "flex-end", gap: 8, marginTop: 10 }}>
                <button
                  onClick={() =>
                    onPermissionDecision(
                      item.requestId,
                      "deny",
                      denyMessages[item.requestId],
                    )
                  }
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    border: "1px solid var(--separator)",
                    borderRadius: 8,
                    background: "var(--bg-elevated)",
                    color: "var(--danger)",
                    padding: "7px 11px",
                  }}
                >
                  <X size={14} /> 拒绝
                </button>
                <button
                  onClick={() => onPermissionDecision(item.requestId, "allow")}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 6,
                    border: "none",
                    borderRadius: 8,
                    background: "var(--accent)",
                    color: "white",
                    padding: "7px 11px",
                  }}
                >
                  <Check size={14} /> 允许
                </button>
              </div>
            </>
          )}
          {item.error && (
            <div style={{ marginTop: 8, color: "var(--danger)", fontSize: 12 }}>
              {item.error}
            </div>
          )}
        </article>
      );
    }
    if (item.type === "tool") {
      return (
        <article
          key={item.id}
          style={{
            marginBottom: 18,
            padding: "13px 15px",
            borderRadius: 14,
            border: "1px solid var(--separator)",
            background: "var(--bg-elevated)",
          }}
        >
          <div style={{ display: "flex", alignItems: "center", gap: 8, fontSize: 13 }}>
            <Wrench size={15} />
            <strong>{item.name}</strong>
            <span style={{ marginLeft: "auto", color: "var(--text-secondary)" }}>
              {item.status === "running" ? (
                <LoaderCircle size={14} className="animate-spin" />
              ) : item.status === "failed" ? (
                <AlertTriangle size={14} />
              ) : (
                <CheckCircle2 size={14} />
              )}
            </span>
          </div>
          {item.input != null && (
            <pre style={{ marginTop: 10, whiteSpace: "pre-wrap", fontSize: 12 }}>
              {display(item.input)}
            </pre>
          )}
          {item.output != null && (
            <pre
              style={{
                marginTop: 10,
                paddingTop: 10,
                borderTop: "1px solid var(--separator)",
                whiteSpace: "pre-wrap",
                fontSize: 12,
              }}
            >
              {display(item.output)}
            </pre>
          )}
        </article>
      );
    }
    return (
      <article
        key={item.id}
        style={{
          marginBottom: 18,
          marginLeft: item.type === "user" ? "18%" : 0,
          padding: "13px 15px",
          borderRadius: 14,
          background:
            item.type === "user"
              ? "var(--accent-soft)"
              : item.type === "system"
                ? "var(--warning-soft)"
                : "var(--bg-elevated)",
          color: item.type === "system" && item.tone === "error" ? "var(--danger)" : undefined,
          whiteSpace: "pre-wrap",
          lineHeight: 1.6,
          fontSize: 14,
        }}
      >
        {item.text}
      </article>
    );
  });
}
