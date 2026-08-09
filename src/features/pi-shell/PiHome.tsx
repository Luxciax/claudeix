"use client";

import { useEffect, useState } from "react";
import dynamic from "next/dynamic";
import { AnimatePresence, motion } from "motion/react";
import { Command, Sparkles } from "lucide-react";

import { TopBar } from "@/components/TopBar";
import { CommandPalette } from "@/components/CommandPalette";
import { Kbd } from "@/components/primitives";
import { ClaudeAgentPanel } from "./ClaudeAgentPanel";
import { ClaudeSessionSidebar } from "./ClaudeSessionSidebar";
import { useRuntimeStore } from "@/features/runtime/runtime-store";
import { useSessionStore } from "@/features/sessions/session-store";
import { useUI } from "@/lib/store";
import { useWorkspace } from "@/lib/workspace";

const TerminalDrawer = dynamic(
  () => import("@/components/TerminalDrawer").then((module) => module.TerminalDrawer),
  { ssr: false },
);

const springPanel = { type: "spring" as const, stiffness: 300, damping: 30 };

export default function PiHome() {
  const { sidebarOpen, zenMode, setCommandPalette, toggleZen } = useUI();
  const workspace = useWorkspace((state) => state.root);
  const activeKey = useSessionStore((state) => state.activeKey);
  const runtime = useRuntimeStore((state) => state.snapshot);
  const startRuntime = useRuntimeStore((state) => state.start);
  const [zenDraft, setZenDraft] = useState("");

  const running =
    runtime?.status === "starting" ||
    runtime?.status === "running" ||
    runtime?.status === "waiting_permission" ||
    runtime?.status === "stopping";

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const mod = event.metaKey || event.ctrlKey;
      if (mod && event.key.toLowerCase() === "k") {
        event.preventDefault();
        setCommandPalette(!useUI.getState().commandPaletteOpen);
      }
      if (mod && event.key === ".") {
        event.preventDefault();
        toggleZen();
      }
      if (mod && event.key.toLowerCase() === "j") {
        event.preventDefault();
        useUI.getState().toggleTerminal();
      }
      if (event.key === "Escape") setCommandPalette(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setCommandPalette, toggleZen]);

  const submitZen = async () => {
    const prompt = zenDraft.trim();
    if (!prompt || !workspace || running) return;
    setZenDraft("");
    await startRuntime({
      workspaceRoot: workspace,
      sessionKey: activeKey,
      prompt,
      permissionMode: "manual",
    });
  };

  const showSidebar = sidebarOpen && !zenMode;

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        height: "100%",
        background: zenMode ? "var(--bg-sunken)" : "var(--bg-base)",
        transition: "background var(--duration-base) var(--spring-smooth)",
      }}
    >
      <TopBar />

      <div
        style={{
          display: "flex",
          flex: 1,
          minHeight: 0,
          overflow: "hidden",
          position: "relative",
        }}
      >
        <AnimatePresence initial={false}>
          {showSidebar && (
            <motion.div
              key="sidebar"
              initial={{ width: 0, opacity: 0 }}
              animate={{ width: 248, opacity: 1 }}
              exit={{ width: 0, opacity: 0 }}
              transition={springPanel}
              style={{ overflow: "hidden", flexShrink: 0 }}
            >
              <ClaudeSessionSidebar />
            </motion.div>
          )}
        </AnimatePresence>

        {!zenMode && (
          <motion.div
            key="agent-main"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.18 }}
            style={{
              display: "flex",
              flex: 1,
              minWidth: 0,
              minHeight: 0,
              overflow: "hidden",
            }}
          >
            <ClaudeAgentPanel />
          </motion.div>
        )}
      </div>

      <TerminalDrawer />

      <AnimatePresence>
        {zenMode && (
          <>
            <div className={`zen-title${running ? " is-working" : ""}`}>Claudeix</div>
            <motion.div
              initial={{ opacity: 0, y: 40 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 40 }}
              transition={{ type: "spring", stiffness: 300, damping: 28 }}
              className="material"
              style={{
                position: "fixed",
                bottom: 28,
                left: "50%",
                x: "-50%",
                display: "flex",
                alignItems: "center",
                gap: 10,
                width: 520,
                maxWidth: "80vw",
                padding: "12px 18px",
                borderRadius: 99,
                border: "1px solid var(--separator)",
                boxShadow: "var(--shadow-lg)",
                zIndex: 40,
              }}
            >
              <Sparkles size={16} style={{ color: "var(--accent)" }} />
              <input
                value={zenDraft}
                onChange={(event) => setZenDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") void submitZen();
                }}
                placeholder={running ? "Claude Code 正在运行…" : "向 Claude Code 描述任务…"}
                style={{
                  flex: 1,
                  border: "none",
                  outline: "none",
                  background: "transparent",
                  fontSize: 14,
                  color: "var(--text-primary)",
                  fontFamily: "var(--font-ui)",
                }}
              />
              <span style={{ fontSize: 11, color: "var(--text-tertiary)" }}>Claude Code</span>
              <Kbd style={{ fontSize: 11, background: "transparent", border: "none" }}>
                <Command size={10} />.
              </Kbd>
            </motion.div>
          </>
        )}
      </AnimatePresence>

      <CommandPalette />
    </div>
  );
}
