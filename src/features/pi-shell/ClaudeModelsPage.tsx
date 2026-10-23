"use client";

import { useEffect, useMemo, useState } from "react";
import {
  Bot,
  Check,
  Cloud,
  KeyRound,
  Plus,
  RefreshCw,
  Save,
  Server,
  Settings2,
  Trash2,
  X,
} from "lucide-react";

import {
  GroupRow,
  InsetGroup,
  Segmented,
  SettingsPage,
  TextRow,
} from "@/components/settings-ui";
import {
  api,
  type ClaudeProviderKind,
  type ClaudeProviderModel,
  type ClaudeProviderProfile,
  type ClaudeProviderStore,
  type ClaudeSettingsFile,
} from "@/lib/tauri-api";
import { useWorkspace } from "@/lib/workspace";

type Scope = "global" | "project";
type JsonObject = Record<string, unknown>;

const PROVIDER_KINDS: { id: ClaudeProviderKind; name: string; detail: string }[] = [
  { id: "anthropic", name: "Anthropic 官方", detail: "Claude API / Claude 订阅默认通道" },
  { id: "gateway", name: "Anthropic 兼容网关", detail: "反代、中转、LiteLLM、Kong 或自建兼容端点" },
  { id: "bedrock", name: "Amazon Bedrock", detail: "Claude Code 原生 Bedrock Provider" },
  { id: "vertex", name: "Google Vertex AI", detail: "Claude Code 原生 Vertex Provider" },
  { id: "foundry", name: "Microsoft Foundry", detail: "Claude Code 原生 Foundry Provider" },
];

const MANAGED_ENV_KEYS = new Set([
  "ANTHROPIC_BASE_URL",
  "ANTHROPIC_API_KEY",
  "ANTHROPIC_AUTH_TOKEN",
  "ANTHROPIC_CUSTOM_HEADERS",
  "ANTHROPIC_MODEL",
  "ANTHROPIC_DEFAULT_OPUS_MODEL",
  "ANTHROPIC_DEFAULT_SONNET_MODEL",
  "ANTHROPIC_DEFAULT_HAIKU_MODEL",
  "CLAUDE_CODE_SUBAGENT_MODEL",
  "CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY",
  "CLAUDE_CODE_USE_BEDROCK",
  "CLAUDE_CODE_USE_VERTEX",
  "CLAUDE_CODE_USE_FOUNDRY",
  "ANTHROPIC_BEDROCK_BASE_URL",
  "ANTHROPIC_VERTEX_BASE_URL",
  "ANTHROPIC_FOUNDRY_BASE_URL",
  "ANTHROPIC_FOUNDRY_API_KEY",
]);

function parseSettings(file: ClaudeSettingsFile | null): JsonObject {
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

function objectValue(value: unknown): JsonObject {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonObject)
    : {};
}

function emptyProfile(kind: ClaudeProviderKind = "gateway"): ClaudeProviderProfile {
  return {
    id: `provider-${Date.now()}`,
    name: PROVIDER_KINDS.find((item) => item.id === kind)?.name ?? "新渠道",
    kind,
    baseUrl: "",
    apiKey: "",
    authToken: "",
    model: "",
    opusModel: "",
    sonnetModel: "",
    haikuModel: "",
    subagentModel: "",
    enableModelDiscovery: kind === "gateway",
    modelPickerEnabled: false,
    replaceBuiltinModels: false,
    models: [],
    customHeaders: "",
    extraEnv: {},
  };
}

function kindLabel(kind: ClaudeProviderKind): string {
  return PROVIDER_KINDS.find((item) => item.id === kind)?.name ?? kind;
}

function inferProvider(settings: JsonObject): string {
  const env = objectValue(settings.env);
  if (env.CLAUDE_CODE_USE_BEDROCK === "1") return "Amazon Bedrock";
  if (env.CLAUDE_CODE_USE_VERTEX === "1") return "Google Vertex AI";
  if (env.CLAUDE_CODE_USE_FOUNDRY === "1") return "Microsoft Foundry";
  if (typeof env.ANTHROPIC_BASE_URL === "string" && env.ANTHROPIC_BASE_URL) {
    return "自定义 Anthropic 兼容网关";
  }
  return "Anthropic 默认通道";
}

function cleanManagedEnv(env: JsonObject): JsonObject {
  const next = { ...env };
  for (const key of MANAGED_ENV_KEYS) delete next[key];
  return next;
}

function setIf(env: JsonObject, key: string, value: string) {
  const trimmed = value.trim();
  if (trimmed) env[key] = trimmed;
}

function envForProfile(profile: ClaudeProviderProfile): JsonObject {
  const env: JsonObject = {};

  if (profile.kind === "gateway") {
    setIf(env, "ANTHROPIC_BASE_URL", profile.baseUrl);
    if (profile.enableModelDiscovery) env.CLAUDE_CODE_ENABLE_GATEWAY_MODEL_DISCOVERY = "1";
    setIf(env, "ANTHROPIC_API_KEY", profile.apiKey);
    setIf(env, "ANTHROPIC_AUTH_TOKEN", profile.authToken);
  } else if (profile.kind === "anthropic") {
    setIf(env, "ANTHROPIC_BASE_URL", profile.baseUrl);
    setIf(env, "ANTHROPIC_API_KEY", profile.apiKey);
    setIf(env, "ANTHROPIC_AUTH_TOKEN", profile.authToken);
  } else if (profile.kind === "bedrock") {
    env.CLAUDE_CODE_USE_BEDROCK = "1";
    setIf(env, "ANTHROPIC_BEDROCK_BASE_URL", profile.baseUrl);
  } else if (profile.kind === "vertex") {
    env.CLAUDE_CODE_USE_VERTEX = "1";
    setIf(env, "ANTHROPIC_VERTEX_BASE_URL", profile.baseUrl);
  } else if (profile.kind === "foundry") {
    env.CLAUDE_CODE_USE_FOUNDRY = "1";
    setIf(env, "ANTHROPIC_FOUNDRY_BASE_URL", profile.baseUrl);
    setIf(env, "ANTHROPIC_FOUNDRY_API_KEY", profile.apiKey);
  }

  setIf(env, "ANTHROPIC_DEFAULT_OPUS_MODEL", profile.opusModel);
  setIf(env, "ANTHROPIC_DEFAULT_SONNET_MODEL", profile.sonnetModel);
  setIf(env, "ANTHROPIC_DEFAULT_HAIKU_MODEL", profile.haikuModel);
  setIf(env, "CLAUDE_CODE_SUBAGENT_MODEL", profile.subagentModel);
  setIf(env, "ANTHROPIC_CUSTOM_HEADERS", profile.customHeaders);

  for (const [key, value] of Object.entries(profile.extraEnv)) {
    if (key.trim() && value.trim()) env[key.trim()] = value.trim();
  }
  return env;
}

function modelPickerForProfile(profile: ClaudeProviderProfile): JsonObject {
  return {
    replaceBuiltInOptions: profile.replaceBuiltinModels,
    options: profile.models
      .filter((model) => model.id.trim())
      .map((model) => ({
        model: model.id.trim(),
        ...(model.label.trim() ? { label: model.label.trim() } : {}),
        ...(model.description.trim() ? { description: model.description.trim() } : {}),
        ...(model.behavesAs.trim() ? { behavesAs: model.behavesAs.trim() } : {}),
      })),
  };
}

function parseExtraEnv(value: string): Record<string, string> {
  const result: Record<string, string> = {};
  for (const raw of value.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    const split = line.indexOf("=");
    if (split <= 0) continue;
    const key = line.slice(0, split).trim();
    const val = line.slice(split + 1).trim();
    if (key && val) result[key] = val;
  }
  return result;
}

function extraEnvText(env: Record<string, string>): string {
  return Object.entries(env)
    .map(([key, value]) => `${key}=${value}`)
    .join("\n");
}

export default function ClaudeModelsPage() {
  const workspace = useWorkspace((state) => state.root);
  const [scope, setScope] = useState<Scope>("global");
  const [file, setFile] = useState<ClaudeSettingsFile | null>(null);
  const [store, setStore] = useState<ClaudeProviderStore>({
    version: 1,
    activeProfileId: null,
    profiles: [],
  });
  const [editing, setEditing] = useState<ClaudeProviderProfile | null>(null);
  const [extraEnvDraft, setExtraEnvDraft] = useState("");
  const [loading, setLoading] = useState(false);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const settings = useMemo(() => parseSettings(file), [file]);
  const currentProvider = inferProvider(settings);
  const currentModel =
    typeof settings.model === "string"
      ? settings.model
      : typeof objectValue(settings.env).ANTHROPIC_MODEL === "string"
        ? String(objectValue(settings.env).ANTHROPIC_MODEL)
        : "跟随 Claude Code 默认";

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const [nextFile, nextStore] = await Promise.all([
        api.readClaudeSettings(scope, workspace),
        api.readClaudeProviderProfiles(),
      ]);
      setFile(nextFile);
      setStore(nextStore);
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

  const persistStore = async (next: ClaudeProviderStore) => {
    const saved = await api.writeClaudeProviderProfiles({
      ...next,
      version: 1,
      path: undefined,
    });
    setStore(saved);
    return saved;
  };

  const saveProfile = async (profile: ClaudeProviderProfile) => {
    setBusyId(profile.id);
    setError(null);
    try {
      const exists = store.profiles.some((item) => item.id === profile.id);
      const profiles = exists
        ? store.profiles.map((item) => (item.id === profile.id ? profile : item))
        : [...store.profiles, profile];
      await persistStore({ ...store, profiles });
      setEditing(null);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusyId(null);
    }
  };

  const applyProfile = async (profile: ClaudeProviderProfile) => {
    setBusyId(profile.id);
    setError(null);
    try {
      const latest = await api.readClaudeSettings(scope, workspace);
      const next = parseSettings(latest);
      const env = cleanManagedEnv(objectValue(next.env));
      for (const savedProfile of store.profiles) {
        for (const key of Object.keys(savedProfile.extraEnv)) delete env[key];
      }
      Object.assign(env, envForProfile(profile));
      if (Object.keys(env).length > 0) next.env = env;
      else delete next.env;

      if (profile.model.trim()) next.model = profile.model.trim();
      else delete next.model;

      if (profile.modelPickerEnabled) {
        next.modelPicker = modelPickerForProfile(profile);
      } else if (store.activeProfileId) {
        delete next.modelPicker;
      }

      const savedFile = await api.writeClaudeSettings(
        scope,
        `${JSON.stringify(next, null, 2)}\n`,
        workspace,
      );
      setFile(savedFile);
      await persistStore({ ...store, activeProfileId: profile.id });
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusyId(null);
    }
  };

  const removeProfile = async (profile: ClaudeProviderProfile) => {
    if (!window.confirm(`删除渠道“${profile.name}”？不会自动改动当前 Claude Code settings.json。`)) {
      return;
    }
    setBusyId(profile.id);
    try {
      const next: ClaudeProviderStore = {
        ...store,
        activeProfileId: store.activeProfileId === profile.id ? null : store.activeProfileId,
        profiles: store.profiles.filter((item) => item.id !== profile.id),
      };
      await persistStore(next);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusyId(null);
    }
  };

  const beginEdit = (profile: ClaudeProviderProfile) => {
    setEditing(structuredClone(profile));
    setExtraEnvDraft(extraEnvText(profile.extraEnv));
  };

  const beginAdd = (kind: ClaudeProviderKind) => {
    const profile = emptyProfile(kind);
    setEditing(profile);
    setExtraEnvDraft("");
  };

  return (
    <SettingsPage
      title="模型渠道"
      subtitle="为 Claude Code 管理官方 API、兼容网关和云 Provider。渠道只是路由配置，运行核心仍然是 Claude Code。"
    >
      <InsetGroup header="生效范围" footer={file?.path ?? "Claude settings.json"}>
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
          title={loading ? "正在读取…" : "重新读取配置"}
          detail="不会启动 Claude Code"
          onClick={() => void load()}
        />
      </InsetGroup>

      <InsetGroup header="当前 Claude Code 路由">
        <GroupRow
          first
          icon={<Server size={15} />}
          title={currentProvider}
          detail={`默认模型：${currentModel}`}
          trailing={store.activeProfileId ? <Check size={16} color="var(--success)" /> : undefined}
        />
      </InsetGroup>

      <InsetGroup
        header="渠道配置"
        footer="应用渠道时只替换 Claudeix 管理的 Provider / 模型环境变量，其他 Claude Code 设置保持不变。"
      >
        {store.profiles.length === 0 ? (
          <div style={{ padding: 14, display: "grid", gap: 8 }}>
            <div style={{ fontSize: 12, color: "var(--text-secondary)", marginBottom: 2 }}>
              还没有保存渠道。选择一种模板开始：
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 8 }}>
              {PROVIDER_KINDS.map((kind) => (
                <button key={kind.id} onClick={() => beginAdd(kind.id)} style={templateButtonStyle}>
                  <ProviderIcon kind={kind.id} />
                  <span style={{ minWidth: 0, textAlign: "left" }}>
                    <strong style={{ display: "block", fontSize: 12 }}>{kind.name}</strong>
                    <span style={{ display: "block", marginTop: 2, fontSize: 10, color: "var(--text-secondary)" }}>
                      {kind.detail}
                    </span>
                  </span>
                </button>
              ))}
            </div>
          </div>
        ) : (
          store.profiles.map((profile, index) => {
            const active = store.activeProfileId === profile.id;
            return (
              <GroupRow
                key={profile.id}
                first={index === 0}
                icon={<ProviderIcon kind={profile.kind} />}
                title={profile.name}
                detail={[
                  kindLabel(profile.kind),
                  profile.baseUrl || null,
                  profile.model ? `模型：${profile.model}` : "模型跟随默认",
                  profile.modelPickerEnabled ? `${profile.models.length} 个可选模型` : null,
                ]
                  .filter(Boolean)
                  .join(" · ")}
                trailing={
                  <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
                    {active && <span style={activeBadgeStyle}>已应用</span>}
                    <button
                      style={smallButtonStyle}
                      disabled={busyId === profile.id}
                      onClick={() => void applyProfile(profile)}
                    >
                      <Check size={13} /> 应用
                    </button>
                    <button style={iconButtonStyle} onClick={() => beginEdit(profile)} title="编辑">
                      <Settings2 size={13} />
                    </button>
                    <button
                      style={{ ...iconButtonStyle, color: "var(--danger)" }}
                      onClick={() => void removeProfile(profile)}
                      title="删除"
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>
                }
              />
            );
          })
        )}
        <div style={{ padding: "10px 14px", borderTop: "1px solid var(--separator)" }}>
          <button onClick={() => beginAdd("gateway")} style={addButtonStyle}>
            <Plus size={14} /> 添加渠道
          </button>
        </div>
      </InsetGroup>

      <InsetGroup
        header="兼容性说明"
        footer="Claude Code 官方支持 Anthropic、Bedrock、Vertex、Foundry 和 Anthropic-format 网关。"
      >
        <GroupRow
          first
          icon={<Bot size={15} />}
          title="自定义模型 ID"
          detail="网关可以使用自己的模型名；Claudeix 不限制模型字符串。非 Claude 模型是否完整兼容工具调用、thinking、缓存等能力取决于网关和模型。"
        />
        <GroupRow
          icon={<Cloud size={15} />}
          title="模型自动发现"
          detail="兼容网关可让 Claude Code 从 /v1/models 填充模型选择器；默认关闭，保存渠道时可单独开启。"
        />
      </InsetGroup>

      {error && <p style={{ marginTop: 14, color: "var(--danger)", fontSize: 12 }}>{error}</p>}

      {editing && (
        <ProviderEditor
          profile={editing}
          extraEnvDraft={extraEnvDraft}
          setExtraEnvDraft={setExtraEnvDraft}
          onChange={setEditing}
          onClose={() => setEditing(null)}
          onSave={() =>
            void saveProfile({
              ...editing,
              extraEnv: parseExtraEnv(extraEnvDraft),
            })
          }
          busy={busyId === editing.id}
        />
      )}
    </SettingsPage>
  );
}

function ProviderEditor({
  profile,
  extraEnvDraft,
  setExtraEnvDraft,
  onChange,
  onClose,
  onSave,
  busy,
}: {
  profile: ClaudeProviderProfile;
  extraEnvDraft: string;
  setExtraEnvDraft: (value: string) => void;
  onChange: (profile: ClaudeProviderProfile) => void;
  onClose: () => void;
  onSave: () => void;
  busy: boolean;
}) {
  const patch = (value: Partial<ClaudeProviderProfile>) => onChange({ ...profile, ...value });
  const [modelDraft, setModelDraft] = useState<ClaudeProviderModel>({
    id: "",
    label: "",
    description: "",
    behavesAs: "",
  });

  const addModel = () => {
    const id = modelDraft.id.trim();
    if (!id || profile.models.some((model) => model.id === id)) return;
    patch({
      modelPickerEnabled: true,
      models: [
        ...profile.models,
        {
          id,
          label: modelDraft.label.trim(),
          description: modelDraft.description.trim(),
          behavesAs: modelDraft.behavesAs.trim(),
        },
      ],
    });
    setModelDraft({ id: "", label: "", description: "", behavesAs: "" });
  };

  const removeModel = (id: string) => {
    patch({ models: profile.models.filter((model) => model.id !== id) });
  };

  return (
    <div style={overlayStyle}>
      <div style={sheetStyle}>
        <div style={sheetHeaderStyle}>
          <div>
            <div style={{ fontSize: 14, fontWeight: 700 }}>渠道配置</div>
            <div style={{ marginTop: 2, fontSize: 10, color: "var(--text-secondary)" }}>
              保存后可一键切换 Claude Code 的 Provider 和模型映射
            </div>
          </div>
          <button style={iconButtonStyle} onClick={onClose} aria-label="关闭">
            <X size={14} />
          </button>
        </div>

        <div style={{ padding: 14, overflow: "auto", display: "grid", gap: 14 }}>
          <InsetGroup header="基础">
            <TextRow first label="名称" value={profile.name} onCommit={(name) => patch({ name: name ?? "" })} />
            <div style={fieldStyle}>
              <span style={fieldLabelStyle}>渠道类型</span>
              <select
                value={profile.kind}
                onChange={(event) =>
                  patch({
                    kind: event.target.value as ClaudeProviderKind,
                    enableModelDiscovery: event.target.value === "gateway" ? profile.enableModelDiscovery : false,
                  })
                }
                style={selectStyle}
              >
                {PROVIDER_KINDS.map((kind) => (
                  <option key={kind.id} value={kind.id}>{kind.name}</option>
                ))}
              </select>
            </div>
            <TextRow
              label={profile.kind === "gateway" ? "Base URL" : "Endpoint（可选）"}
              detail={endpointHint(profile.kind)}
              value={profile.baseUrl}
              placeholder={endpointPlaceholder(profile.kind)}
              onCommit={(baseUrl) => patch({ baseUrl: baseUrl ?? "" })}
            />
          </InsetGroup>

          {(profile.kind === "anthropic" || profile.kind === "gateway" || profile.kind === "foundry") && (
            <InsetGroup header="认证">
              <SecretRow
                first
                label={profile.kind === "foundry" ? "Foundry API Key" : "API Key"}
                value={profile.apiKey}
                onCommit={(apiKey) => patch({ apiKey })}
              />
              {profile.kind !== "foundry" && (
                <SecretRow
                  label="Auth Token"
                  value={profile.authToken}
                  onCommit={(authToken) => patch({ authToken })}
                />
              )}
              <div style={{ padding: "8px 14px", fontSize: 10, color: "var(--text-tertiary)" }}>
                凭据保存在本机 Claudeix 配置中；应用渠道后会按 Claude Code 官方变量写入当前 settings.json。
              </div>
            </InsetGroup>
          )}

          <InsetGroup header="模型映射" footer="留空就让 Claude Code / Provider 自己选择默认值。">
            <TextRow
              first
              label="默认模型"
              value={profile.model}
              placeholder="例如 sonnet / claude-sonnet-… / 网关模型 ID"
              onCommit={(model) => patch({ model: model ?? "" })}
            />
            <TextRow label="Opus 映射" value={profile.opusModel} onCommit={(opusModel) => patch({ opusModel: opusModel ?? "" })} />
            <TextRow label="Sonnet 映射" value={profile.sonnetModel} onCommit={(sonnetModel) => patch({ sonnetModel: sonnetModel ?? "" })} />
            <TextRow label="Haiku 映射" value={profile.haikuModel} onCommit={(haikuModel) => patch({ haikuModel: haikuModel ?? "" })} />
            <TextRow label="Subagent 模型" value={profile.subagentModel} onCommit={(subagentModel) => patch({ subagentModel: subagentModel ?? "" })} />
          </InsetGroup>

          <InsetGroup
            header="可选模型列表"
            footer="开启后生成 Claude Code 的 modelPicker。可加入 Provider 自己的模型 ID；behavesAs 用于告诉 Claude Code 按哪个已知 Claude 模型的客户端能力处理它。"
          >
            <label style={switchRowStyle}>
              <span>
                <strong style={{ display: "block", fontSize: 12 }}>管理 /model 模型列表</strong>
                <span style={{ fontSize: 10, color: "var(--text-secondary)" }}>
                  适合一个网关同时提供多个模型的情况
                </span>
              </span>
              <input
                type="checkbox"
                checked={profile.modelPickerEnabled}
                onChange={(event) => patch({ modelPickerEnabled: event.target.checked })}
              />
            </label>

            {profile.modelPickerEnabled && (
              <>
                <label style={{ ...switchRowStyle, borderTop: "1px solid var(--separator)" }}>
                  <span>
                    <strong style={{ display: "block", fontSize: 12 }}>替换内置模型列表</strong>
                    <span style={{ fontSize: 10, color: "var(--text-secondary)" }}>
                      关闭时保留 Claude Code 自带模型，并把下面模型追加进去
                    </span>
                  </span>
                  <input
                    type="checkbox"
                    checked={profile.replaceBuiltinModels}
                    onChange={(event) => patch({ replaceBuiltinModels: event.target.checked })}
                  />
                </label>

                {profile.models.length > 0 && (
                  <div style={{ display: "grid", gap: 6, padding: "8px 12px", borderTop: "1px solid var(--separator)" }}>
                    {profile.models.map((model) => (
                      <div key={model.id} style={modelRowStyle}>
                        <div style={{ minWidth: 0, flex: 1 }}>
                          <div style={{ fontSize: 11.5, fontWeight: 650, wordBreak: "break-all" }}>
                            {model.label || model.id}
                          </div>
                          <div style={{ marginTop: 2, fontSize: 9.5, color: "var(--text-secondary)", wordBreak: "break-all" }}>
                            {model.id}
                            {model.behavesAs ? ` · behavesAs: ${model.behavesAs}` : ""}
                          </div>
                          {model.description && (
                            <div style={{ marginTop: 2, fontSize: 9.5, color: "var(--text-tertiary)" }}>
                              {model.description}
                            </div>
                          )}
                        </div>
                        <button
                          style={{ ...iconButtonStyle, color: "var(--danger)" }}
                          onClick={() => removeModel(model.id)}
                          title="移除模型"
                        >
                          <Trash2 size={12} />
                        </button>
                      </div>
                    ))}
                  </div>
                )}

                <div style={modelAddGridStyle}>
                  <input
                    value={modelDraft.id}
                    onChange={(event) => setModelDraft({ ...modelDraft, id: event.target.value })}
                    placeholder="模型 ID，例如 provider/model-name"
                    style={compactInputStyle}
                  />
                  <input
                    value={modelDraft.label}
                    onChange={(event) => setModelDraft({ ...modelDraft, label: event.target.value })}
                    placeholder="显示名称（可选）"
                    style={compactInputStyle}
                  />
                  <input
                    value={modelDraft.behavesAs}
                    onChange={(event) => setModelDraft({ ...modelDraft, behavesAs: event.target.value })}
                    placeholder="behavesAs（可选，已知 Claude 模型 ID）"
                    style={compactInputStyle}
                  />
                  <input
                    value={modelDraft.description}
                    onChange={(event) => setModelDraft({ ...modelDraft, description: event.target.value })}
                    placeholder="说明（可选）"
                    style={compactInputStyle}
                  />
                  <button
                    style={{ ...addButtonStyle, justifyContent: "center" }}
                    onClick={addModel}
                    disabled={
                      !modelDraft.id.trim() ||
                      profile.models.some((model) => model.id === modelDraft.id.trim())
                    }
                  >
                    <Plus size={13} /> 添加模型
                  </button>
                </div>
              </>
            )}
          </InsetGroup>

          {profile.kind === "gateway" && (
            <InsetGroup header="网关能力">
              <label style={switchRowStyle}>
                <span>
                  <strong style={{ display: "block", fontSize: 12 }}>自动发现模型</strong>
                  <span style={{ fontSize: 10, color: "var(--text-secondary)" }}>
                    让 Claude Code 查询网关 /v1/models
                  </span>
                </span>
                <input
                  type="checkbox"
                  checked={profile.enableModelDiscovery}
                  onChange={(event) => patch({ enableModelDiscovery: event.target.checked })}
                />
              </label>
              <TextRow
                label="自定义请求头"
                detail="多行：Name: Value"
                value={profile.customHeaders}
                onCommit={(customHeaders) => patch({ customHeaders: customHeaders ?? "" })}
              />
            </InsetGroup>
          )}

          <InsetGroup
            header="高级环境变量"
            footer={advancedHint(profile.kind)}
          >
            <textarea
              value={extraEnvDraft}
              onChange={(event) => setExtraEnvDraft(event.target.value)}
              placeholder={"KEY=value\nKEY2=value2"}
              spellCheck={false}
              style={textareaStyle}
            />
          </InsetGroup>
        </div>

        <div style={sheetFooterStyle}>
          <button style={secondaryButtonStyle} onClick={onClose}>取消</button>
          <button
            style={primaryButtonStyle}
            onClick={onSave}
            disabled={busy || !profile.name.trim()}
          >
            <Save size={14} /> {busy ? "保存中…" : "保存渠道"}
          </button>
        </div>
      </div>
    </div>
  );
}

function SecretRow({
  label,
  value,
  onCommit,
  first = false,
}: {
  label: string;
  value: string;
  onCommit: (value: string) => void;
  first?: boolean;
}) {
  const [draft, setDraft] = useState(value);
  useEffect(() => setDraft(value), [value]);
  return (
    <div style={{ ...fieldStyle, borderTop: first ? "none" : "1px solid var(--separator)" }}>
      <span style={fieldLabelStyle}><KeyRound size={13} /> {label}</span>
      <input
        type="password"
        value={draft}
        placeholder={value ? "••••••••" : "留空"}
        onChange={(event) => setDraft(event.target.value)}
        onBlur={() => onCommit(draft)}
        style={inputStyle}
      />
    </div>
  );
}

function ProviderIcon({ kind }: { kind: ClaudeProviderKind }) {
  if (kind === "anthropic") return <Bot size={15} />;
  if (kind === "gateway") return <Server size={15} />;
  return <Cloud size={15} />;
}

function endpointHint(kind: ClaudeProviderKind): string {
  if (kind === "gateway") return "写入 ANTHROPIC_BASE_URL";
  if (kind === "bedrock") return "写入 ANTHROPIC_BEDROCK_BASE_URL";
  if (kind === "vertex") return "写入 ANTHROPIC_VERTEX_BASE_URL";
  if (kind === "foundry") return "写入 ANTHROPIC_FOUNDRY_BASE_URL";
  return "官方 Anthropic 一般留空";
}

function endpointPlaceholder(kind: ClaudeProviderKind): string {
  if (kind === "gateway") return "https://gateway.example.com";
  if (kind === "foundry") return "https://resource.services.ai.azure.com/anthropic";
  return "留空使用 Provider 默认端点";
}

function advancedHint(kind: ClaudeProviderKind): string {
  if (kind === "bedrock") return "例如 AWS_REGION、AWS_BEARER_TOKEN_BEDROCK。";
  if (kind === "vertex") return "例如 ANTHROPIC_VERTEX_PROJECT_ID、CLOUD_ML_REGION。";
  if (kind === "foundry") return "例如 ANTHROPIC_FOUNDRY_RESOURCE。";
  return "用于 Claude Code 支持但 Claudeix 尚未做成专门字段的变量；每行 KEY=value。";
}

const templateButtonStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 10,
  padding: "11px 12px",
  border: "1px solid var(--separator)",
  borderRadius: 10,
  background: "var(--bg-elevated)",
  color: "var(--text-primary)",
};

const addButtonStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 6,
  padding: "7px 10px",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-elevated)",
  color: "var(--text-primary)",
  fontSize: 11,
};

const smallButtonStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 4,
  padding: "5px 8px",
  border: "1px solid var(--separator)",
  borderRadius: 7,
  background: "var(--bg-elevated)",
  color: "var(--text-primary)",
  fontSize: 10,
};

const iconButtonStyle: React.CSSProperties = {
  width: 28,
  height: 28,
  display: "grid",
  placeItems: "center",
  border: "1px solid var(--separator)",
  borderRadius: 7,
  background: "var(--bg-elevated)",
  color: "var(--text-secondary)",
};

const activeBadgeStyle: React.CSSProperties = {
  padding: "3px 6px",
  borderRadius: 99,
  background: "var(--accent-muted)",
  color: "var(--accent)",
  fontSize: 9,
  fontWeight: 700,
};

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  zIndex: 80,
  display: "flex",
  justifyContent: "flex-end",
  background: "rgba(0,0,0,0.28)",
};

const sheetStyle: React.CSSProperties = {
  width: "min(590px, 76vw)",
  height: "100%",
  display: "grid",
  gridTemplateRows: "58px 1fr 62px",
  background: "var(--bg-base)",
  borderLeft: "1px solid var(--separator)",
  boxShadow: "-18px 0 42px rgba(0,0,0,0.16)",
};

const sheetHeaderStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  padding: "0 14px 0 16px",
  borderBottom: "1px solid var(--separator)",
};

const sheetFooterStyle: React.CSSProperties = {
  display: "flex",
  justifyContent: "flex-end",
  gap: 8,
  alignItems: "center",
  padding: "0 14px",
  borderTop: "1px solid var(--separator)",
};

const fieldStyle: React.CSSProperties = {
  minHeight: 48,
  padding: "8px 14px",
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 14,
  borderTop: "1px solid var(--separator)",
};

const fieldLabelStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 6,
  fontSize: 12,
  color: "var(--text-primary)",
};

const inputStyle: React.CSSProperties = {
  width: 250,
  padding: "7px 9px",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-sunken)",
  color: "var(--text-primary)",
  fontSize: 11,
};

const selectStyle: React.CSSProperties = {
  ...inputStyle,
  width: 250,
};

const switchRowStyle: React.CSSProperties = {
  minHeight: 52,
  padding: "8px 14px",
  display: "flex",
  alignItems: "center",
  justifyContent: "space-between",
  gap: 12,
};

const modelRowStyle: React.CSSProperties = {
  display: "flex",
  alignItems: "center",
  gap: 8,
  padding: "8px 9px",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-elevated)",
};

const modelAddGridStyle: React.CSSProperties = {
  display: "grid",
  gridTemplateColumns: "1fr 1fr",
  gap: 7,
  padding: 12,
  borderTop: "1px solid var(--separator)",
};

const compactInputStyle: React.CSSProperties = {
  minWidth: 0,
  padding: "7px 8px",
  border: "1px solid var(--separator)",
  borderRadius: 8,
  background: "var(--bg-sunken)",
  color: "var(--text-primary)",
  fontSize: 10.5,
};

const textareaStyle: React.CSSProperties = {
  width: "100%",
  minHeight: 120,
  padding: 10,
  border: "none",
  outline: "none",
  resize: "vertical",
  background: "var(--bg-sunken)",
  color: "var(--text-primary)",
  fontFamily: "var(--font-mono)",
  fontSize: 11,
};

const primaryButtonStyle: React.CSSProperties = {
  display: "inline-flex",
  alignItems: "center",
  gap: 6,
  padding: "8px 12px",
  border: "none",
  borderRadius: 8,
  background: "var(--accent)",
  color: "white",
  fontSize: 11,
};

const secondaryButtonStyle: React.CSSProperties = {
  ...primaryButtonStyle,
  border: "1px solid var(--separator)",
  background: "var(--bg-elevated)",
  color: "var(--text-primary)",
};
