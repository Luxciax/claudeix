"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import { motion } from "motion/react";
import {
  Sparkles,
  Boxes,
  Puzzle,
  Wand2,
  Download,
  Settings,
} from "lucide-react";
import { useUI } from "@/lib/store";
import { useT, type MsgKey } from "@/lib/i18n";
import { useRuntimeStore } from "@/features/runtime/runtime-store";
import { api } from "@/lib/tauri-api";

const ITEMS: { href: string; icon: React.ReactNode; labelKey: MsgKey }[] = [
  { href: "/", icon: <Sparkles size={17} />, labelKey: "nav.workspace" },
  { href: "/models/", icon: <Boxes size={17} />, labelKey: "nav.models" },
  { href: "/plugins/", icon: <Puzzle size={17} />, labelKey: "nav.plugins" },
  { href: "/skills/", icon: <Wand2 size={17} />, labelKey: "nav.skills" },
  { href: "/store/", icon: <Download size={17} />, labelKey: "nav.store" },
  { href: "/settings/", icon: <Settings size={17} />, labelKey: "nav.settings" },
];

const STATUS_COLOR: Record<string, string> = {
  ready: "var(--success)",
  running: "var(--agent-thinking)",
  connecting: "var(--warning)",
  disconnected: "var(--text-tertiary)",
};

/** Left-most icon rail — iPadOS tab-bar feel. Hidden in zen mode. */
export function NavRail() {
  const pathname = usePathname();
  const zenMode = useUI((s) => s.zenMode);
  const runtime = useRuntimeStore((state) => state.snapshot);
  const [installed, setInstalled] = useState(false);
  const t = useT();

  useEffect(() => {
    void api.claudeStatus().then((status) => setInstalled(status.installed)).catch(() => setInstalled(false));
  }, []);

  if (zenMode) return null;

  const isActive = (href: string) =>
    href === "/" ? pathname === "/" : pathname.startsWith(href.replace(/\/$/, ""));

  return (
    <nav
      className="material-thin"
      style={{
        width: 56,
        height: "100%",
        display: "flex",
        flexDirection: "column",
        alignItems: "center",
        gap: 6,
        paddingTop: 52, // below traffic lights
        paddingBottom: 14,
        borderRight: "1px solid var(--separator)",
        flexShrink: 0,
        zIndex: 15,
      }}
      data-tauri-drag-region
    >
      <div
        title="Claudeix"
        style={{
          width: 30,
          height: 30,
          display: "grid",
          placeItems: "center",
          marginBottom: 8,
          borderRadius: 9,
          background: "var(--accent)",
          color: "var(--text-on-accent)",
          fontSize: 15,
          fontWeight: 750,
        }}
      >
        C
      </div>
      {ITEMS.map((item) => {
        const active = isActive(item.href);
        return (
          <Link key={item.href} href={item.href} title={t(item.labelKey)}>
            <motion.span
              whileTap={{ scale: 0.86 }}
              transition={{ type: "spring", stiffness: 500, damping: 24 }}
              style={{
                position: "relative",
                display: "grid",
                placeItems: "center",
                width: 38,
                height: 38,
                fontSize: 16,
                borderRadius: 11, // squircle-ish
                color: active ? "var(--accent)" : "var(--text-tertiary)",
                transition:
                  "color var(--duration-fast) var(--spring-smooth)",
                cursor: "pointer",
              }}
            >
              {/* active pill slides between tabs — iPadOS tab-bar feel */}
              {active && (
                <motion.span
                  layoutId="rail-active"
                  transition={{ type: "spring", stiffness: 500, damping: 34 }}
                  style={{
                    position: "absolute",
                    inset: 0,
                    borderRadius: 11,
                    background: "var(--accent-muted)",
                  }}
                />
              )}
              <span
                style={{
                  position: "relative",
                  zIndex: 1,
                  display: "grid",
                  placeItems: "center",
                }}
              >
                {item.icon}
              </span>
            </motion.span>
          </Link>
        );
      })}

      {/* connection dot pinned to the bottom */}
      <span
        title={
          runtime?.status === "running" || runtime?.status === "waiting_permission"
            ? "Claude Code 正在运行"
            : installed
              ? "Claude Code 已就绪"
              : "未检测到 Claude Code"
        }
        style={{
          marginTop: "auto",
          width: 8,
          height: 8,
          borderRadius: 99,
          background:
            runtime?.status === "running" || runtime?.status === "waiting_permission"
              ? STATUS_COLOR.running
              : installed
                ? STATUS_COLOR.ready
                : STATUS_COLOR.disconnected,
          boxShadow:
            runtime?.status === "running" || runtime?.status === "waiting_permission"
              ? "0 0 0 4px var(--accent-muted)"
              : "none",
        }}
      />
    </nav>
  );
}
