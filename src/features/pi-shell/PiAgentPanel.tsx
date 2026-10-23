"use client";

import { useState } from "react";
import { AnimatePresence, motion } from "motion/react";
import { ScrollArea } from "@appica/ui-react/scroll-area";
import { FilePenLine, X } from "lucide-react";

import { AgentPanel } from "@/components/AgentPanel";
import { IconButton } from "@/components/primitives";
import { TranscriptTimeline } from "@/features/transcript/TranscriptTimeline";
import { usePiNativeStore } from "@/features/engines/pi-native-store";
import { useChat } from "@/lib/pi/chat";
import { useUI } from "@/lib/store";
import { useWorkspace } from "@/lib/workspace";

export function PiAgentPanel() {
  const workspace = useWorkspace((state) => state.root) ?? "";
  const detail = usePiNativeStore((state) => state.detail);
  const mutating = usePiNativeStore((state) => state.mutating);
  const streaming = useChat((state) => state.streaming);
  const [showEdit, setShowEdit] = useState(false);
  const [allowAssistantEditing, setAllowAssistantEditing] = useState(false);

  return (
    <div style={{ display: "flex", flex: 1, minWidth: 0, minHeight: 0, position: "relative" }}>
      <AgentPanel
        onShowHistory={() => {
          if (!useUI.getState().sidebarOpen) useUI.getState().toggleSidebar();
        }}
        onNewSession={() => void usePiNativeStore.getState().newSession(workspace)}
        extraHeaderActions={
          detail ? (
            <IconButton
              label="编辑 Pi 原生会话"
              onClick={() => setShowEdit((value) => !value)}
              active={showEdit}
            >
              <FilePenLine size={14} />
            </IconButton>
          ) : null
        }
      />

      <AnimatePresence initial={false}>
        {showEdit && detail && (
          <motion.aside
            initial={{ x: 40, opacity: 0 }}
            animate={{ x: 0, opacity: 1 }}
            exit={{ x: 40, opacity: 0 }}
            transition={{ type: "spring", stiffness: 340, damping: 32 }}
            className="material"
            style={{
              position: "absolute",
              top: 0,
              right: 0,
              bottom: 0,
              width: "min(520px, 54%)",
              minWidth: 360,
              zIndex: 12,
              display: "flex",
              flexDirection: "column",
              borderLeft: "1px solid var(--separator)",
              boxShadow: "var(--shadow-lg)",
              background: "var(--bg-base)",
            }}
          >
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 10,
                padding: "10px 12px",
                borderBottom: "1px solid var(--separator)",
              }}
            >
              <div style={{ minWidth: 0, flex: 1 }}>
                <div style={{ fontSize: 12, fontWeight: 650 }}>编辑 Pi 原生会话</div>
                <div
                  title={detail.summary.sourcePath}
                  style={{
                    marginTop: 2,
                    fontSize: 10.5,
                    color: "var(--text-tertiary)",
                    overflow: "hidden",
                    textOverflow: "ellipsis",
                    whiteSpace: "nowrap",
                  }}
                >
                  {detail.summary.sourcePath}
                </div>
              </div>
              <label
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                  fontSize: 11,
                  color: "var(--text-secondary)",
                  whiteSpace: "nowrap",
                }}
              >
                <input
                  type="checkbox"
                  checked={allowAssistantEditing}
                  onChange={(event) => setAllowAssistantEditing(event.target.checked)}
                />
                编辑助手消息
              </label>
              <IconButton label="关闭编辑器" onClick={() => setShowEdit(false)}>
                <X size={14} />
              </IconButton>
            </div>

            <ScrollArea
              orientation="vertical"
              scrollShadow
              style={{ flex: 1, minHeight: 0 }}
              viewportProps={{
                style: {
                  padding: "14px 14px 28px",
                  overscrollBehaviorY: "contain",
                },
              }}
            >
              <TranscriptTimeline
                detail={{ records: detail.records }}
                assistantLabel="Pi"
                sourceLabel="Pi"
                allowAssistantEditing={allowAssistantEditing}
                editingDisabled={streaming || mutating}
                mutating={mutating}
                onEdit={(target, text) => usePiNativeStore.getState().edit(target, text)}
              />
            </ScrollArea>
          </motion.aside>
        )}
      </AnimatePresence>
    </div>
  );
}
