"use client";

import Link from "next/link";
import { AnimatePresence, motion } from "motion/react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@appica/ui-react/dropdown-menu";
import { Bot, Check, ChevronDown, SlidersHorizontal } from "lucide-react";

import { useClaudeModel } from "@/features/runtime/model-store";

const PRESETS = [
  { id: "sonnet", label: "Sonnet" },
  { id: "opus", label: "Opus" },
  { id: "haiku", label: "Haiku" },
] as const;

export function ClaudeModelPicker({ compact = false }: { compact?: boolean }) {
  const model = useClaudeModel((state) => state.model);
  const setModel = useClaudeModel((state) => state.setModel);
  const label = PRESETS.find((item) => item.id === model)?.label ?? model ?? "默认模型";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <motion.button
            whileTap={{ scale: 0.95 }}
            transition={{ type: "spring", stiffness: 500, damping: 24 }}
            aria-label="选择 Claude 模型"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              maxWidth: compact ? 150 : 210,
              padding: compact ? "2px 6px" : "3px 9px",
              fontSize: 11.5,
              fontWeight: 500,
              color: "var(--text-secondary)",
              background: compact ? "transparent" : "var(--bg-base)",
              border: compact ? "none" : "1px solid var(--separator)",
              borderRadius: 99,
              cursor: "pointer",
              whiteSpace: "nowrap",
              fontFamily: "var(--font-ui)",
            }}
          />
        }
      >
        <Bot size={12} style={{ flexShrink: 0, color: "var(--accent)" }} />
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={model ?? "default"}
            initial={{ y: 6, opacity: 0, filter: "blur(2px)" }}
            animate={{ y: 0, opacity: 1, filter: "blur(0px)" }}
            exit={{ y: -6, opacity: 0, filter: "blur(2px)" }}
            transition={{ type: "spring", stiffness: 500, damping: 30 }}
            style={{ overflow: "hidden", textOverflow: "ellipsis" }}
          >
            {label}
          </motion.span>
        </AnimatePresence>
        <ChevronDown size={11} style={{ flexShrink: 0, opacity: 0.6 }} />
      </DropdownMenuTrigger>

      <DropdownMenuContent
        side="top"
        align="start"
        sideOffset={8}
        className="material-thick"
        style={{
          minWidth: 220,
          padding: 6,
          border: "1px solid var(--separator)",
          borderRadius: "var(--radius-md)",
          boxShadow: "var(--shadow-lg)",
          zIndex: 60,
        }}
      >
        <DropdownMenuItem onClick={() => setModel(null)} style={itemStyle}>
          <Bot size={14} />
          <span style={{ flex: 1 }}>使用 settings.json 默认模型</span>
          {model === null && <Check size={14} color="var(--accent)" />}
        </DropdownMenuItem>
        {PRESETS.map((item) => (
          <DropdownMenuItem key={item.id} onClick={() => setModel(item.id)} style={itemStyle}>
            <Bot size={14} />
            <span style={{ flex: 1 }}>{item.label}</span>
            {model === item.id && <Check size={14} color="var(--accent)" />}
          </DropdownMenuItem>
        ))}
        {model && !PRESETS.some((item) => item.id === model) && (
          <DropdownMenuItem onClick={() => setModel(model)} style={itemStyle}>
            <Bot size={14} />
            <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis" }}>{model}</span>
            <Check size={14} color="var(--accent)" />
          </DropdownMenuItem>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem
          render={
            <Link href="/models/" style={{ ...itemStyle, textDecoration: "none" }}>
              <SlidersHorizontal size={14} />
              <span>管理模型与 Provider</span>
            </Link>
          }
        />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

const itemStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 9,
  padding: "7px 10px",
  borderRadius: 8,
  fontSize: 12.5,
  color: "var(--text-primary)",
  cursor: "pointer",
};
