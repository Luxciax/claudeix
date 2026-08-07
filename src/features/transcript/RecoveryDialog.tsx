"use client";

import { RotateCcw, ShieldCheck, TriangleAlert } from "lucide-react";

import type { EditRecoveryItem } from "@/features/sessions/types";

function stateLabel(state: string): string {
  if (state === "source_after") return "原 JSONL 已是本次修改后的版本";
  if (state === "source_before") return "原 JSONL 已回到修改前版本";
  if (state === "source_missing") return "原 JSONL 当前缺失";
  if (state === "source_unknown") return "原 JSONL 已发生其他变化";
  return state;
}

function operationLabel(operation: string): string {
  if (operation === "snapshot_restore") return "完整快照恢复";
  return "消息编辑";
}

export function RecoveryDialog({
  items,
  resolvingId,
  error,
  onResolve,
}: {
  items: EditRecoveryItem[];
  resolvingId: string | null;
  error: string | null;
  onResolve: (
    item: EditRecoveryItem,
    action: "keep_current" | "restore_backup",
  ) => void;
}) {
  return (
    <div
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 100,
        display: "grid",
        placeItems: "center",
        padding: 24,
        background: "rgba(0,0,0,0.52)",
      }}
    >
      <section
        style={{
          width: "min(820px, 94vw)",
          maxHeight: "88vh",
          display: "grid",
          gridTemplateRows: "auto 1fr",
          border: "1px solid var(--separator)",
          borderRadius: 16,
          background: "var(--bg-base)",
          boxShadow: "0 30px 90px rgba(0,0,0,0.32)",
          overflow: "hidden",
        }}
      >
        <header
          style={{
            display: "flex",
            alignItems: "center",
            gap: 10,
            padding: "16px 18px",
            borderBottom: "1px solid var(--separator)",
          }}
        >
          <TriangleAlert size={19} />
          <div>
            <div style={{ fontSize: 14, fontWeight: 750 }}>检测到未完成的会话写入事务</div>
            <div style={{ marginTop: 3, fontSize: 11, color: "var(--text-secondary)" }}>
              Claudeix 已暂停正常写入。请先确认保留当前版本，或恢复到操作前的自动备份。
            </div>
          </div>
        </header>

        <div style={{ overflow: "auto", padding: 16 }}>
          {error && (
            <div
              style={{
                marginBottom: 12,
                padding: 10,
                borderRadius: 9,
                background: "var(--warning-soft)",
                color: "var(--danger)",
                fontSize: 11,
              }}
            >
              {error}
            </div>
          )}
          <div style={{ display: "grid", gap: 12 }}>
            {items.map((item) => {
              const busy = resolvingId === item.transactionId;
              return (
                <article
                  key={item.transactionId}
                  style={{
                    padding: 14,
                    border: "1px solid var(--warning)",
                    borderRadius: 12,
                    background: "var(--warning-soft)",
                  }}
                >
                  <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <strong style={{ fontSize: 12 }}>{operationLabel(item.operation)}</strong>
                    <span
                      style={{
                        marginLeft: "auto",
                        fontSize: 10,
                        color: "var(--text-secondary)",
                      }}
                    >
                      事务状态：{item.status}
                    </span>
                  </div>
                  <div style={{ marginTop: 7, fontSize: 11 }}>{stateLabel(item.detectedState)}</div>
                  <div
                    style={{
                      marginTop: 6,
                      fontSize: 10,
                      color: "var(--text-secondary)",
                      wordBreak: "break-all",
                    }}
                  >
                    {item.sourcePath}
                  </div>
                  <div style={{ marginTop: 6, fontSize: 10, color: "var(--text-secondary)" }}>
                    自动备份：{item.backupValid ? "校验正常" : "不可用或哈希异常"}
                  </div>

                  <div style={{ marginTop: 12, display: "flex", gap: 8 }}>
                    {item.canRestoreBackup && (
                      <button
                        onClick={() => onResolve(item, "restore_backup")}
                        disabled={Boolean(resolvingId)}
                        style={{
                          flex: 1,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          gap: 7,
                          padding: "9px 11px",
                          border: "1px solid var(--separator)",
                          borderRadius: 9,
                          background: "var(--bg-elevated)",
                          color: "var(--text-primary)",
                          opacity: resolvingId ? 0.55 : 1,
                        }}
                      >
                        <RotateCcw size={14} />
                        {busy ? "处理中…" : "恢复操作前备份"}
                      </button>
                    )}
                    {item.canKeepCurrent && (
                      <button
                        onClick={() => onResolve(item, "keep_current")}
                        disabled={Boolean(resolvingId)}
                        style={{
                          flex: 1,
                          display: "flex",
                          alignItems: "center",
                          justifyContent: "center",
                          gap: 7,
                          padding: "9px 11px",
                          border: "none",
                          borderRadius: 9,
                          background: "var(--accent)",
                          color: "white",
                          opacity: resolvingId ? 0.55 : 1,
                        }}
                      >
                        <ShieldCheck size={14} />
                        {busy ? "处理中…" : "保留当前修改并完成事务"}
                      </button>
                    )}
                  </div>

                  {!item.canKeepCurrent && !item.canRestoreBackup && (
                    <div
                      style={{
                        marginTop: 10,
                        padding: 9,
                        borderRadius: 8,
                        background: "var(--bg-elevated)",
                        color: "var(--danger)",
                        fontSize: 10,
                      }}
                    >
                      当前文件既不是预期修改结果，自动备份也不可用。Claudeix 不会猜测性覆盖，请先手工保留该路径内容再处理数据库事务。
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        </div>
      </section>
    </div>
  );
}
