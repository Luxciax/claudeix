"use client";

import { Bot, Code2 } from "lucide-react";

import { InsetGroup } from "@/components/settings-ui";
import { useEngineSelection, type ActiveEngine } from "./engine-selection";

export function EngineScopeSelector() {
  const activeEngine = useEngineSelection((state) => state.activeEngine);
  const setActiveEngine = useEngineSelection((state) => state.setActiveEngine);

  return (
    <InsetGroup header="当前 Agent" footer="插件、商店等管理页面会跟随这里选择的 Agent。">
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 6, padding: 8 }}>
        <EngineScopeButton
          active={activeEngine === "claude"}
          label="Claude Code"
          icon={<Bot size={14} />}
          onClick={() => setActiveEngine("claude")}
        />
        <EngineScopeButton
          active={activeEngine === "pi"}
          label="Pi"
          icon={<Code2 size={14} />}
          onClick={() => setActiveEngine("pi")}
        />
      </div>
    </InsetGroup>
  );
}

function EngineScopeButton({
  active,
  label,
  icon,
  onClick,
}: {
  active: boolean;
  label: string;
  icon: React.ReactNode;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      style={{
        height: 36,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        gap: 7,
        border: `1px solid ${active ? "var(--accent)" : "var(--separator)"}`,
        borderRadius: 9,
        background: active ? "var(--accent-muted)" : "var(--bg-elevated)",
        color: active ? "var(--accent)" : "var(--text-secondary)",
        fontSize: 12,
        fontWeight: 650,
        cursor: "pointer",
      }}
    >
      {icon}
      {label}
    </button>
  );
}

