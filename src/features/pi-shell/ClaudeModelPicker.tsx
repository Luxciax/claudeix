"use client";

import Link from "next/link";
import { useEffect, useMemo } from "react";
import { AnimatePresence, motion } from "motion/react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@appica/ui-react/dropdown-menu";
import { Bot, Check, ChevronDown, SlidersHorizontal } from "lucide-react";

import { ProviderMeta } from "@/components/provider-meta";
import { useClaudeModel } from "@/features/runtime/model-store";
import { useClaudeModels } from "@/lib/claude/models";
import { isModelEnabled } from "@/lib/pi/model-scope";

const PRESETS = [
  { id: "sonnet", label: "Sonnet" },
  { id: "opus", label: "Opus" },
  { id: "haiku", label: "Haiku" },
] as const;

export function ClaudeModelPicker({ compact = false }: { compact?: boolean }) {
  const providerId = useClaudeModel((state) => state.providerId);
  const model = useClaudeModel((state) => state.model);
  const setModel = useClaudeModel((state) => state.setModel);

  const catalog = useClaudeModels((state) => state.data);
  const loaded = useClaudeModels((state) => state.loaded);

  useEffect(() => {
    if (!loaded) void useClaudeModels.getState().load();
  }, [loaded]);

  const customModels = useMemo(
    () =>
      Object.entries(catalog.providers)
        .flatMap(([currentProviderId, provider]) =>
          (provider.models ?? [])
            .filter((entry) =>
              isModelEnabled(catalog.enabledModels, currentProviderId, entry.id)
            )
            .map((entry) => ({
              providerId: currentProviderId,
              id: entry.id,
              label: entry.name || entry.id,
            }))
        )
        .sort((a, b) =>
          a.providerId === b.providerId
            ? a.label.localeCompare(b.label)
            : a.providerId.localeCompare(b.providerId)
        ),
    [catalog.enabledModels, catalog.providers]
  );

  const selectedCustom = customModels.find(
    (entry) => entry.providerId === providerId && entry.id === model
  );
  const label = providerId && !selectedCustom ? "模型已失效，请重新选择" :
    selectedCustom?.label ??
    PRESETS.find((item) => !providerId && item.id === model)?.label ??
    model ??
    "默认模型";

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <motion.button
            whileTap={{ scale: 0.95 }}
            transition={{ type: "spring", stiffness: 500, damping: 24 }}
            aria-label="选择 Claude Code 模型"
            style={{
              display: "flex",
              alignItems: "center",
              gap: 5,
              maxWidth: compact ? 170 : 240,
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
        {providerId ? (
          <ProviderMeta provider={providerId} size={10} />
        ) : (
          <Bot size={12} style={{ flexShrink: 0, color: "var(--accent)" }} />
        )}
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={providerId ? `${providerId}/${model ?? ""}` : model ?? "default"}
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
          minWidth: 240,
          maxHeight: 360,
          overflowY: "auto",
          padding: 6,
          border: "1px solid var(--separator)",
          borderRadius: "var(--radius-md)",
          boxShadow: "var(--shadow-lg)",
          zIndex: 60,
        }}
      >
        <DropdownMenuItem onClick={() => setModel(null, null)} style={itemStyle}>
          <Bot size={14} />
          <span style={{ flex: 1 }}>使用 settings.json 默认模型</span>
          {model === null && providerId === null && <Check size={14} color="var(--accent)" />}
        </DropdownMenuItem>

        {PRESETS.map((item) => (
          <DropdownMenuItem
            key={item.id}
            onClick={() => setModel(item.id, null)}
            style={itemStyle}
          >
            <Bot size={14} />
            <span style={{ flex: 1 }}>{item.label}</span>
            {providerId === null && model === item.id && (
              <Check size={14} color="var(--accent)" />
            )}
          </DropdownMenuItem>
        ))}

        {customModels.length > 0 && <DropdownMenuSeparator />}

        {customModels.map((entry) => (
          <DropdownMenuItem
            key={`${entry.providerId}/${entry.id}`}
            onClick={() => setModel(entry.id, entry.providerId)}
            style={itemStyle}
          >
            <ProviderMeta provider={entry.providerId} size={10} />
            <span style={{ flex: 1, minWidth: 0 }}>
              <span
                style={{
                  display: "block",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {entry.label}
              </span>
              <span
                style={{
                  display: "block",
                  marginTop: 1,
                  fontSize: 10,
                  color: "var(--text-tertiary)",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                  whiteSpace: "nowrap",
                }}
              >
                {entry.providerId} · {entry.id}
              </span>
            </span>
            {providerId === entry.providerId && model === entry.id && (
              <Check size={14} color="var(--accent)" />
            )}
          </DropdownMenuItem>
        ))}

        {model && providerId === null && !PRESETS.some((item) => item.id === model) && (
          <DropdownMenuItem onClick={() => setModel(model, null)} style={itemStyle}>
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
