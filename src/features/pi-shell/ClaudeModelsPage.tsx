"use client";

import { useEffect, useMemo, useState } from "react";
import { Bot, Check, KeyRound, RefreshCw, Server } from "lucide-react";

import { GroupRow, InsetGroup, Segmented, SettingsPage, TextRow } from "@/components/settings-ui";
import { api, type ClaudeSettingsFile } from "@/lib/tauri-api";
import { useWorkspace } from "@/lib/workspace";

type Scope = "global" | "project";
type JsonObject = Record<string, unknown>;

const MODEL_PRESETS = ["sonnet", "opus", "haiku"] as const;

function parse(file: ClaudeSettingsFile | null): JsonObject {
  if (!file) return {};
  try {
    const value = JSON.parse(file.content) as unknown;
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as JsonObject)
      : {};
  } catch {
    return {};
  }
}

function stringValue(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

export default function ClaudeModelsPage() {
  const workspace = useWorkspace((state) => state.root);
  const [scope, setScope] = useState<Scope>("global");
  const [file, setFile] = useState<ClaudeSettingsFile | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [secretDraft, setSecretDraft] = useState("");

  const settings = useMemo(() => parse(file), [file]);
  const env = useMemo(() => {
    const value = settings.env;
    return value && typeof value === "object" && !Array.isArray(value)
      ? (value as JsonObject)
      : {};
  }, [settings]);
  const currentModel = stringValue(settings.model);
  const baseUrl = stringValue(env.ANTHROPIC_BASE_URL);
  const hasApiKey =
    typeof env.ANTHROPIC_API_KEY === "string" && Boolean(env.ANTHROPIC_API_KEY) ||
    typeof env.ANTHROPIC_AUTH_TOKEN === "string" && Boolean(env.ANTHROPIC_AUTH_TOKEN);

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      setFile(await api.readClaudeSettings(scope, workspace));
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scope, workspace]);

  const write = async (next: JsonObject) => {
    try {
      const result = await api.writeClaudeSettings(
        scope,
        `${JSON.stringify(next, null, 2)}\n`,
        workspace,
      );
      setFile(result);
      setError(null);
    } catch (cause) {
      setError(String(cause));
    }
  };

  const setKey = (key: string, value: unknown) => {
    const next = { ...settings };
    if (value === undefined) delete next[key];
    else next[key] = value;
    void write(next);
  };

  const setEnv = (key: string, value: string | undefined) => {
    const nextEnv = { ...env };
    if (value === undefined) delete nextEnv[key];
    else nextEnv[key] = value;
    const next = { ...settings };
    if (Object.keys(nextEnv).length === 0) delete next.env;
    else next.env = nextEnv;
    void write(next);
  };

  const saveSecret = () => {
    const value = secretDraft.trim();
    if (!value) return;
    setSecretDraft("");
    setEnv("ANTHROPIC_API_KEY", value);
  };

  return (
    <SettingsPage
      title="模型"
      subtitle="保留 Pi Desktop 的模型/Provider 管理入口，配置对象改为 Claude Code。"
    >
      <InsetGroup header="配置范围" footer={file?.path ?? "Claude settings.json"}>
        <div style={{ padding: "12px 14px" }}>
          <Segmented
            options={["global", "project"] as const}
            value={scope}
            onChange={setScope}
            labelOf={(value) => (value === "global" ? "全局" : "当前项目")}
          />
        </div>
        <GroupRow
          icon={<RefreshCw size={15} />}
          title={loading ? "正在读取…" : "刷新配置"}
          onClick={() => void load()}
        />
      </InsetGroup>

      <InsetGroup header="Claude 模型" footer="点击预设会写入 settings.json → model；也可以填完整模型名。">
        {MODEL_PRESETS.map((model, index) => (
          <GroupRow
            key={model}
            first={index === 0}
            icon={<Bot size={15} />}
            title={model}
            detail={currentModel === model ? "当前默认模型" : "Claude Code 模型别名"}
            trailing={currentModel === model ? <Check size={16} color="var(--success)" /> : undefined}
            onClick={() => setKey("model", model)}
          />
        ))}
        <TextRow
          label="自定义模型名"
          value={currentModel}
          placeholder="例如 claude-sonnet-... 或网关提供的模型名"
          onCommit={(value) => setKey("model", value)}
        />
      </InsetGroup>

      <InsetGroup
        header="Provider / 兼容渠道"
        footer="Claude Code 通过环境变量使用兼容网关；原 Pi Provider 页的 Base URL 能力映射到这里。"
      >
        <TextRow
          first
          label="ANTHROPIC_BASE_URL"
          detail="Anthropic / Claude 兼容 API 地址"
          value={baseUrl}
          placeholder="https://api.anthropic.com"
          onCommit={(value) => setEnv("ANTHROPIC_BASE_URL", value)}
        />
        <GroupRow
          icon={<Server size={15} />}
          title="当前渠道"
          detail={baseUrl || "Anthropic 默认端点"}
        />
        <GroupRow
          icon={<KeyRound size={15} />}
          title="API Key"
          detail={hasApiKey ? "已配置；Claudeix 不回显现有密钥" : "尚未在 Claude settings.json env 中配置"}
          trailing={
            <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
              <input
                type="password"
                value={secretDraft}
                placeholder={hasApiKey ? "输入新值覆盖" : "输入 API Key"}
                onChange={(event) => setSecretDraft(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") saveSecret();
                }}
                style={{
                  width: 180,
                  padding: "6px 8px",
                  border: "1px solid var(--separator)",
                  borderRadius: 8,
                  background: "var(--bg-sunken)",
                  color: "var(--text-primary)",
                  fontSize: 11,
                }}
              />
              <button
                onClick={saveSecret}
                disabled={!secretDraft.trim()}
                style={{
                  border: "none",
                  borderRadius: 8,
                  padding: "6px 9px",
                  background: "var(--accent)",
                  color: "white",
                  opacity: secretDraft.trim() ? 1 : 0.45,
                }}
              >
                保存
              </button>
            </div>
          }
        />
      </InsetGroup>

      {error && <p style={{ marginTop: 14, color: "var(--danger)", fontSize: 12 }}>{error}</p>}
    </SettingsPage>
  );
}
