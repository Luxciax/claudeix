import { AlertTriangle, Brain, FileImage, TerminalSquare, Wrench } from "lucide-react";

import type {
  ContentBlock,
  SessionDetail,
  TranscriptRecord,
} from "@/features/sessions/types";

function pretty(value: unknown): string {
  if (typeof value === "string") return value;
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

function BlockView({ block }: { block: ContentBlock }) {
  switch (block.type) {
    case "text":
      return <div style={{ whiteSpace: "pre-wrap" }}>{block.text}</div>;
    case "thinking":
      return (
        <details
          style={{
            border: "1px solid var(--separator)",
            borderRadius: 10,
            padding: "8px 10px",
            color: "var(--text-secondary)",
          }}
        >
          <summary style={{ cursor: "pointer", display: "flex", gap: 7, alignItems: "center" }}>
            <Brain size={14} />
            {block.redacted ? "已隐藏的思考" : "思考过程"}
          </summary>
          {!block.redacted && (
            <div style={{ marginTop: 8, whiteSpace: "pre-wrap", fontSize: 12 }}>
              {block.text}
            </div>
          )}
        </details>
      );
    case "tool_use":
      return (
        <div
          style={{
            border: "1px solid var(--separator)",
            borderRadius: 11,
            padding: 11,
            background: "var(--bg-sunken)",
          }}
        >
          <div style={{ display: "flex", gap: 7, alignItems: "center", fontWeight: 600, fontSize: 12 }}>
            <Wrench size={14} /> {block.name}
          </div>
          <pre style={{ margin: "8px 0 0", whiteSpace: "pre-wrap", fontSize: 11, color: "var(--text-secondary)" }}>
            {pretty(block.input)}
          </pre>
        </div>
      );
    case "tool_result":
      return (
        <div
          style={{
            border: `1px solid ${block.is_error ? "var(--danger)" : "var(--separator)"}`,
            borderRadius: 11,
            padding: 11,
            background: "var(--bg-sunken)",
          }}
        >
          <div style={{ display: "flex", gap: 7, alignItems: "center", fontWeight: 600, fontSize: 12 }}>
            <TerminalSquare size={14} /> 工具结果
          </div>
          <pre style={{ margin: "8px 0 0", whiteSpace: "pre-wrap", fontSize: 11, color: "var(--text-secondary)" }}>
            {pretty(block.output)}
          </pre>
        </div>
      );
    case "image":
      return (
        <div style={{ display: "flex", gap: 7, alignItems: "center", color: "var(--text-secondary)" }}>
          <FileImage size={14} /> 图片记录{block.media_type ? ` · ${block.media_type}` : ""}
        </div>
      );
    case "system_event":
      return (
        <div style={{ fontSize: 12, color: "var(--text-secondary)", whiteSpace: "pre-wrap" }}>
          <strong>{block.name}</strong>
          {block.text ? `：${block.text}` : ""}
        </div>
      );
    case "unknown":
      return (
        <details style={{ fontSize: 11, color: "var(--text-tertiary)" }}>
          <summary>未识别记录 · {block.kind}</summary>
          <pre style={{ whiteSpace: "pre-wrap" }}>{pretty(block.raw)}</pre>
        </details>
      );
  }
}

function RecordView({ record }: { record: TranscriptRecord }) {
  if (record.blocks.length === 0) return null;
  const role = record.role ?? "system";
  const isUser = role === "user";
  const isAssistant = role === "assistant";
  return (
    <article
      style={{
        marginBottom: 18,
        marginLeft: isUser ? "18%" : 0,
        marginRight: !isUser && !isAssistant ? "12%" : 0,
        padding: "13px 15px",
        borderRadius: 14,
        background: isUser
          ? "var(--accent-soft)"
          : isAssistant
            ? "var(--bg-elevated)"
            : "var(--warning-soft)",
        lineHeight: 1.6,
        fontSize: 14,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          gap: 12,
          marginBottom: 8,
          fontSize: 10,
          color: "var(--text-tertiary)",
        }}
      >
        <span>{isUser ? "你" : isAssistant ? "Claude" : record.rawType}</span>
        <span>JSONL 第 {record.lineIndex + 1} 行</span>
      </div>
      <div style={{ display: "grid", gap: 10 }}>
        {record.blocks.map((block, index) => (
          <BlockView key={`${record.rawHash}-${index}`} block={block} />
        ))}
      </div>
    </article>
  );
}

export function TranscriptTimeline({ detail }: { detail: SessionDetail }) {
  const visibleRecords = detail.records.filter((record) => record.blocks.length > 0);
  return (
    <>
      {detail.diagnostics.length > 0 && (
        <div
          style={{
            display: "flex",
            gap: 8,
            alignItems: "flex-start",
            marginBottom: 16,
            padding: "10px 12px",
            borderRadius: 10,
            background: "var(--warning-soft)",
            color: "var(--text-secondary)",
            fontSize: 12,
          }}
        >
          <AlertTriangle size={15} style={{ marginTop: 1, flexShrink: 0 }} />
          <div>
            此会话有 {detail.diagnostics.length} 行无法解析，其他记录仍已加载。
            <div style={{ marginTop: 3 }}>{detail.diagnostics[0]?.message}</div>
          </div>
        </div>
      )}
      {visibleRecords.map((record) => (
        <RecordView key={`${record.lineIndex}-${record.rawHash}`} record={record} />
      ))}
    </>
  );
}
