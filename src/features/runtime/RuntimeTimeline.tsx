import { AlertTriangle, CheckCircle2, LoaderCircle, Wrench } from "lucide-react";

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

export function RuntimeTimeline({ items }: { items: RuntimeTimelineItem[] }) {
  return items.map((item) => {
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
