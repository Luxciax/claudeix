"use client";

import { useState } from "react";
import {
  AlertTriangle,
  Brain,
  FileImage,
  Pencil,
  Save,
  TerminalSquare,
  Wrench,
  X,
} from "lucide-react";

import type {
  ContentBlock,
  EditTarget,
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

function BlockView({
  block,
  target,
  sourceLabel,
  editingDisabled,
  mutating,
  onEdit,
}: {
  block: ContentBlock;
  target: EditTarget | null;
  sourceLabel: string;
  editingDisabled: boolean;
  mutating: boolean;
  onEdit: (target: EditTarget, newText: string) => Promise<boolean>;
}) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");

  switch (block.type) {
    case "text":
      if (editing && target) {
        return (
          <div
            style={{
              display: "grid",
              gap: 9,
              padding: 11,
              border: "1px solid var(--warning)",
              borderRadius: 10,
              background: "var(--warning-soft)",
            }}
          >
            <textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              disabled={mutating}
              rows={Math.min(14, Math.max(4, draft.split("\n").length + 2))}
              style={{
                width: "100%",
                resize: "vertical",
                border: "1px solid var(--separator)",
                borderRadius: 8,
                background: "var(--bg-elevated)",
                color: "var(--text-primary)",
                padding: 10,
                font: "inherit",
                lineHeight: 1.55,
              }}
            />
            <div style={{ fontSize: 11, color: "var(--text-secondary)" }}>
              保存后会直接覆盖 {sourceLabel} 原始会话；写入前会自动创建完整快照。
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end", gap: 8 }}>
              <button
                onClick={() => setEditing(false)}
                disabled={mutating}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: "var(--bg-elevated)",
                  color: "var(--text-primary)",
                  padding: "7px 10px",
                }}
              >
                <X size={14} /> 取消
              </button>
              <button
                onClick={async () => {
                  const saved = await onEdit(target, draft);
                  if (saved) setEditing(false);
                }}
                disabled={mutating || draft === block.text}
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  border: "none",
                  borderRadius: 8,
                  background: "var(--accent)",
                  color: "white",
                  padding: "7px 10px",
                  opacity: mutating || draft === block.text ? 0.5 : 1,
                }}
              >
                <Save size={14} /> {mutating ? "写入中…" : "保存并覆盖原会话"}
              </button>
            </div>
          </div>
        );
      }
      return (
        <div style={{ position: "relative", whiteSpace: "pre-wrap", paddingRight: target ? 34 : 0 }}>
          {block.text}
          {target && (
            <button
              onClick={() => {
                setDraft(block.text);
                setEditing(true);
              }}
              disabled={editingDisabled || mutating}
              title={editingDisabled ? `会话运行或被外部 ${sourceLabel} 占用时不能编辑` : "编辑这条消息"}
              style={{
                position: "absolute",
                top: -4,
                right: -4,
                width: 28,
                height: 28,
                display: "grid",
                placeItems: "center",
                border: "1px solid var(--separator)",
                borderRadius: 8,
                background: "var(--bg-elevated)",
                color: "var(--text-secondary)",
                opacity: editingDisabled || mutating ? 0.45 : 1,
              }}
            >
              <Pencil size={13} />
            </button>
          )}
        </div>
      );
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

function RecordView({
  record,
  assistantLabel,
  sourceLabel,
  allowAssistantEditing,
  editingDisabled,
  mutating,
  onEdit,
}: {
  record: TranscriptRecord;
  assistantLabel: string;
  sourceLabel: string;
  allowAssistantEditing: boolean;
  editingDisabled: boolean;
  mutating: boolean;
  onEdit: (target: EditTarget, newText: string) => Promise<boolean>;
}) {
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
        <span>{isUser ? "你" : isAssistant ? assistantLabel : record.rawType}</span>
        <span>JSONL 第 {record.lineIndex + 1} 行</span>
      </div>
      <div style={{ display: "grid", gap: 10 }}>
        {record.blocks.map((block, index) => {
          const editableRole =
            record.role === "user" ||
            (record.role === "assistant" && allowAssistantEditing);
          const target =
            editableRole && block.type === "text"
              ? record.editableTargets.find(
                  (candidate) =>
                    candidate.contentIndex === index ||
                    (candidate.contentIndex == null && index === 0),
                ) ?? null
              : null;
          return (
            <BlockView
              key={`${record.rawHash}-${index}`}
              block={block}
              target={target}
              sourceLabel={sourceLabel}
              editingDisabled={editingDisabled}
              mutating={mutating}
              onEdit={onEdit}
            />
          );
        })}
      </div>
    </article>
  );
}

export function TranscriptTimeline({
  detail,
  assistantLabel = "Claude",
  sourceLabel = "Claude Code",
  allowAssistantEditing,
  editingDisabled,
  mutating,
  onEdit,
}: {
  detail: Pick<SessionDetail, "records"> & { diagnostics?: SessionDetail["diagnostics"] };
  assistantLabel?: string;
  sourceLabel?: string;
  allowAssistantEditing: boolean;
  editingDisabled: boolean;
  mutating: boolean;
  onEdit: (target: EditTarget, newText: string) => Promise<boolean>;
}) {
  const visibleRecords = detail.records.filter((record) => record.blocks.length > 0);
  return (
    <>
      {(detail.diagnostics?.length ?? 0) > 0 && (
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
            此会话有 {detail.diagnostics?.length ?? 0} 行无法解析，其他记录仍已加载。
            <div style={{ marginTop: 3 }}>{detail.diagnostics?.[0]?.message}</div>
          </div>
        </div>
      )}
      {visibleRecords.map((record) => (
        <RecordView
          key={`${record.lineIndex}-${record.rawHash}`}
          record={record}
          assistantLabel={assistantLabel}
          sourceLabel={sourceLabel}
          allowAssistantEditing={allowAssistantEditing}
          editingDisabled={editingDisabled}
          mutating={mutating}
          onEdit={onEdit}
        />
      ))}
    </>
  );
}
