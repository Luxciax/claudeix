"use client";

import { useEffect, useMemo, useState } from "react";
import { Download, Package, Power, RefreshCw, Trash2, Upload } from "lucide-react";

import { GroupRow, InsetGroup, IOSSwitch, SettingsPage } from "@/components/settings-ui";
import { api } from "@/lib/tauri-api";

type PluginItem = {
  id: string;
  name: string;
  description: string;
  version: string;
  enabled: boolean;
  installed: boolean;
};

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function stringField(record: Record<string, unknown>, ...keys: string[]): string | undefined {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === "string" && value.trim()) return value;
  }
  return undefined;
}

function normalizePlugin(value: unknown, installed: boolean): PluginItem | null {
  const record = asRecord(value);
  if (!record) return null;
  const id = stringField(record, "id", "name", "plugin", "key", "identifier");
  if (!id) return null;
  const status = stringField(record, "status", "state")?.toLowerCase();
  const enabledValue = record.enabled;
  return {
    id,
    name: stringField(record, "displayName", "title", "name") ?? id,
    description: stringField(record, "description", "summary") ?? "",
    version: stringField(record, "version") ?? "",
    enabled:
      typeof enabledValue === "boolean"
        ? enabledValue
        : status
          ? status !== "disabled"
          : installed,
    installed,
  };
}

function normalizeResponse(value: unknown): { installed: PluginItem[]; available: PluginItem[] } {
  if (Array.isArray(value)) {
    return {
      installed: value.flatMap((item) => {
        const plugin = normalizePlugin(item, true);
        return plugin ? [plugin] : [];
      }),
      available: [],
    };
  }
  const record = asRecord(value);
  if (!record) return { installed: [], available: [] };
  const installed = Array.isArray(record.installed) ? record.installed : [];
  const available = Array.isArray(record.available) ? record.available : [];
  return {
    installed: installed.flatMap((item) => {
      const plugin = normalizePlugin(item, true);
      return plugin ? [plugin] : [];
    }),
    available: available.flatMap((item) => {
      const plugin = normalizePlugin(item, false);
      return plugin ? [plugin] : [];
    }),
  };
}

export default function ClaudePluginsPage() {
  const [installed, setInstalled] = useState<PluginItem[]>([]);
  const [available, setAvailable] = useState<PluginItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const value = await api.listClaudePlugins(true);
      const normalized = normalizeResponse(value);
      setInstalled(normalized.installed);
      setAvailable(normalized.available);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const installedIds = useMemo(() => new Set(installed.map((plugin) => plugin.id)), [installed]);
  const installable = available.filter((plugin) => !installedIds.has(plugin.id));

  const action = async (
    actionName: "enable" | "disable" | "install" | "uninstall" | "update",
    plugin: PluginItem,
  ) => {
    if (actionName === "uninstall") {
      const confirmed = window.confirm(`确认卸载 Claude Code 插件：${plugin.name}？`);
      if (!confirmed) return;
    }
    setBusyId(plugin.id);
    setError(null);
    try {
      await api.runClaudePluginAction(actionName, plugin.id);
      await load();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <SettingsPage
      title="插件"
      subtitle="沿用 Pi Desktop 插件管理页面结构，底层使用 Claude Code 原生 plugin CLI。"
    >
      <InsetGroup header="Claude Code Plugins" footer="数据来自 claude plugin list --available --json。">
        <GroupRow
          first
          icon={<RefreshCw size={15} />}
          title={loading ? "正在刷新插件…" : "刷新插件列表"}
          detail="已安装插件与 Marketplace 可用插件"
          onClick={() => void load()}
        />
      </InsetGroup>

      <InsetGroup header={`已安装 · ${installed.length}`}>
        {installed.length === 0 ? (
          <GroupRow first icon={<Package size={15} />} title="暂无已安装插件" />
        ) : (
          installed.map((plugin, index) => (
            <GroupRow
              key={plugin.id}
              first={index === 0}
              icon={<Package size={15} />}
              title={plugin.name}
              detail={[plugin.version, plugin.description].filter(Boolean).join(" · ") || plugin.id}
              trailing={
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                  <button
                    title="更新插件"
                    onClick={() => void action("update", plugin)}
                    disabled={Boolean(busyId)}
                    style={iconButtonStyle}
                  >
                    <Upload size={13} />
                  </button>
                  <button
                    title="卸载插件"
                    onClick={() => void action("uninstall", plugin)}
                    disabled={Boolean(busyId)}
                    style={{ ...iconButtonStyle, color: "var(--danger)" }}
                  >
                    <Trash2 size={13} />
                  </button>
                  <IOSSwitch
                    checked={plugin.enabled}
                    disabled={Boolean(busyId)}
                    onChange={(checked) => void action(checked ? "enable" : "disable", plugin)}
                  />
                </div>
              }
            />
          ))
        )}
      </InsetGroup>

      <InsetGroup header={`Marketplace · ${installable.length}`} footer="安装动作调用 Claude Code 原生 plugin install。">
        {installable.length === 0 ? (
          <GroupRow first icon={<Download size={15} />} title="当前 Marketplace 没有可安装插件" />
        ) : (
          installable.map((plugin, index) => (
            <GroupRow
              key={plugin.id}
              first={index === 0}
              icon={<Download size={15} />}
              title={plugin.name}
              detail={plugin.description || plugin.id}
              trailing={
                <button
                  onClick={() => void action("install", plugin)}
                  disabled={Boolean(busyId)}
                  style={installButtonStyle}
                >
                  <Power size={13} /> 安装
                </button>
              }
            />
          ))
        )}
      </InsetGroup>

      {error && <p style={{ marginTop: 14, color: "var(--danger)", fontSize: 12 }}>{error}</p>}
    </SettingsPage>
  );
}

const iconButtonStyle: React.CSSProperties = {
  width: 28,
  height: 28,
  display: "grid",
  placeItems: "center",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-base)",
  color: "var(--text-secondary)",
  cursor: "pointer",
};

const installButtonStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 6,
  border: "none",
  borderRadius: 8,
  padding: "6px 10px",
  background: "var(--accent)",
  color: "white",
  cursor: "pointer",
  fontSize: 11,
};
