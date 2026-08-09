"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { Download, Package, RefreshCw, Store } from "lucide-react";

import { GroupRow, InsetGroup, SettingsPage } from "@/components/settings-ui";
import { api } from "@/lib/tauri-api";

type MarketplacePlugin = {
  id: string;
  name: string;
  description: string;
  installed: boolean;
};

function normalize(value: unknown): MarketplacePlugin[] {
  const record = value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
  if (!record) return [];
  const installedRaw = Array.isArray(record.installed) ? record.installed : [];
  const availableRaw = Array.isArray(record.available) ? record.available : [];
  const ids = new Set(
    installedRaw.flatMap((item) => {
      if (!item || typeof item !== "object" || Array.isArray(item)) return [];
      const row = item as Record<string, unknown>;
      const id = [row.id, row.name, row.plugin].find((field) => typeof field === "string") as
        | string
        | undefined;
      return id ? [id] : [];
    }),
  );
  return availableRaw.flatMap((item) => {
    if (!item || typeof item !== "object" || Array.isArray(item)) return [];
    const row = item as Record<string, unknown>;
    const id = [row.id, row.name, row.plugin, row.identifier].find(
      (field) => typeof field === "string" && field,
    ) as string | undefined;
    if (!id) return [];
    const name = [row.displayName, row.title, row.name].find(
      (field) => typeof field === "string" && field,
    ) as string | undefined;
    const description = [row.description, row.summary].find(
      (field) => typeof field === "string",
    ) as string | undefined;
    return [
      {
        id,
        name: name ?? id,
        description: description ?? "",
        installed: ids.has(id),
      },
    ];
  });
}

export default function ClaudeStorePage() {
  const [plugins, setPlugins] = useState<MarketplacePlugin[]>([]);
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setPlugins(normalize(await api.listClaudePlugins(true)));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const install = async (plugin: MarketplacePlugin) => {
    setBusyId(plugin.id);
    setError(null);
    try {
      await api.runClaudePluginAction("install", plugin.id);
      await load();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusyId(null);
    }
  };

  return (
    <SettingsPage
      title="商店"
      subtitle="Pi Desktop Store 模块保留，数据源替换为 Claude Code Plugin Marketplace。"
    >
      <InsetGroup header="Marketplace">
        <GroupRow
          first
          icon={<RefreshCw size={15} />}
          title={loading ? "正在刷新 Marketplace…" : "刷新 Marketplace"}
          onClick={() => void load()}
        />
        <GroupRow
          icon={<Package size={15} />}
          title="管理已安装插件"
          detail="启用、禁用、更新和卸载"
          trailing={
            <Link href="/plugins/" style={{ color: "var(--accent)", fontSize: 12 }}>
              打开插件页
            </Link>
          }
        />
      </InsetGroup>

      <InsetGroup header={`可用插件 · ${plugins.length}`}>
        {plugins.length === 0 ? (
          <GroupRow
            first
            icon={<Store size={15} />}
            title="当前 Marketplace 没有返回可安装插件"
            detail="可先通过 Claude Code 添加 Marketplace，然后在这里刷新。"
          />
        ) : (
          plugins.map((plugin, index) => (
            <GroupRow
              key={plugin.id}
              first={index === 0}
              icon={<Store size={15} />}
              title={plugin.name}
              detail={plugin.description || plugin.id}
              trailing={
                plugin.installed ? (
                  <span style={{ color: "var(--success)", fontSize: 11 }}>已安装</span>
                ) : (
                  <button
                    onClick={() => void install(plugin)}
                    disabled={Boolean(busyId)}
                    style={{
                      display: "inline-flex",
                      alignItems: "center",
                      gap: 6,
                      border: "none",
                      borderRadius: 8,
                      padding: "6px 10px",
                      background: "var(--accent)",
                      color: "white",
                      cursor: busyId ? "default" : "pointer",
                      opacity: busyId ? 0.6 : 1,
                    }}
                  >
                    <Download size={13} /> 安装
                  </button>
                )
              }
            />
          ))
        )}
      </InsetGroup>

      {error && <p style={{ marginTop: 14, color: "var(--danger)", fontSize: 12 }}>{error}</p>}
    </SettingsPage>
  );
}
