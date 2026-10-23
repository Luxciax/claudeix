"use client";

import { create } from "zustand";
import { api } from "@/lib/tauri-api";
import { modelRef } from "@/lib/pi/model-scope";

export const CLAUDE_API_TYPES = [
  "anthropic-messages",
  "anthropic-auth-token",
  "bedrock",
  "vertex",
  "foundry",
] as const;

export interface ClaudeCustomModelDef {
  id: string;
  name?: string;
  reasoning?: boolean;
  input?: ("text" | "image")[];
  contextWindow?: number;
  maxTokens?: number;
  [key: string]: unknown;
}

export interface ClaudeCustomProvider {
  baseUrl: string;
  api: string;
  apiKey?: string;
  models: ClaudeCustomModelDef[];
  [key: string]: unknown;
}

export interface ClaudeModelsCatalog {
  version: number;
  enabledModels: string[];
  providers: Record<string, ClaudeCustomProvider>;
}

interface ProviderConfig {
  baseUrl: string;
  api: string;
  apiKey?: string;
}

interface ClaudeModelsStore {
  loaded: boolean;
  status: "unloaded" | "loading" | "ready" | "loadError";
  busy: boolean;
  revision: string | null;
  path: string;
  data: ClaudeModelsCatalog;
  parseError: string | null;
  lastError: string | null;
  load: () => Promise<void>;
  setEnabledModels: (enabledModels: string[]) => Promise<void>;
  addModel: (providerId: string, cfg: ProviderConfig, model: ClaudeCustomModelDef) => Promise<void>;
  addModels: (providerId: string, cfg: ProviderConfig, models: ClaudeCustomModelDef[]) => Promise<void>;
  updateProvider: (providerId: string, cfg: ProviderConfig) => Promise<void>;
  removeProvider: (providerId: string) => Promise<void>;
  removeModel: (providerId: string, modelId: string) => Promise<void>;
  updateModel: (
    oldProviderId: string,
    oldModelId: string,
    newProviderId: string,
    model: ClaudeCustomModelDef,
  ) => Promise<void>;
  fetchModels: (baseUrl: string, api: string, apiKey?: string) => Promise<string[]>;
}

const EMPTY: ClaudeModelsCatalog = {
  version: 1,
  enabledModels: [],
  providers: {},
};

function normalize(value: unknown): ClaudeModelsCatalog {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("模型目录必须为对象");
  }
  const raw = value as Partial<ClaudeModelsCatalog>;
  if (raw.version !== 1) throw new Error("不支持的模型目录版本");
  return {
    ...raw,
    version: raw.version!,
    enabledModels: Array.isArray(raw.enabledModels)
      ? raw.enabledModels.filter((item): item is string => typeof item === "string")
      : [],
    providers:
      raw.providers && typeof raw.providers === "object" && !Array.isArray(raw.providers)
        ? (raw.providers as Record<string, ClaudeCustomProvider>)
        : {},
  };
}

function providerWithConfig(
  existing: ClaudeCustomProvider | undefined,
  cfg: ProviderConfig,
): ClaudeCustomProvider {
  return {
    ...(existing ?? {}),
    baseUrl: cfg.baseUrl,
    api: cfg.api || CLAUDE_API_TYPES[0],
    ...(cfg.apiKey ? { apiKey: cfg.apiKey } : existing?.apiKey ? { apiKey: existing.apiKey } : {}),
    models: [...(existing?.models ?? [])],
  };
}

export const useClaudeModels = create<ClaudeModelsStore>((set, get) => ({
  loaded: false,
  status: "unloaded",
  busy: false,
  revision: null,
  path: "",
  data: structuredClone(EMPTY),
  parseError: null,
  lastError: null,

  load: async () => {
    if (get().busy) return;
    const generation = ++loadGeneration;
    set({ status: "loading" });
    try {
      const file = await api.readClaudeModels();
      if (generation !== loadGeneration) return;
      try {
        set({
          loaded: true,
          status: "ready",
          revision: file.revision,
          path: file.path,
          data: normalize(JSON.parse(file.content)),
          parseError: null,
          lastError: null,
        });
      } catch (error) {
        set({
          loaded: true,
          path: file.path,
          status: "loadError",
          parseError: error instanceof Error ? error.message : String(error),
        });
      }
    } catch (error) {
      if (generation !== loadGeneration) return;
      set({ loaded: true, status: "loadError", lastError: error instanceof Error ? error.message : String(error) });
    }
  },

  setEnabledModels: async (enabledModels) => {
    requireReady(get());
    const next = structuredClone(get().data);
    next.enabledModels = [...new Set(enabledModels)];
    await save(next, set, get);
  },

  addModel: async (providerId, cfg, model) => {
    if (!providerId.trim() || !model.id.trim()) throw new Error("Provider 和模型 ID 不能为空");
    requireReady(get());
    const next = structuredClone(get().data);
    const provider = providerWithConfig(next.providers[providerId], cfg);
    const index = provider.models.findIndex((item) => item.id === model.id);
    if (index >= 0) throw new Error("模型已存在");
    provider.models.push(model);
    next.providers[providerId] = provider;
    await save(next, set, get);
  },

  addModels: async (providerId, cfg, models) => {
    if (!providerId.trim()) throw new Error("Provider ID 不能为空");
    const clean = models.filter((model) => model.id.trim());
    if (clean.length === 0) return;
    requireReady(get());
    const next = structuredClone(get().data);
    const provider = providerWithConfig(next.providers[providerId], cfg);
    const seen = new Set(provider.models.map((item) => item.id));
    for (const model of clean) {
      if (seen.has(model.id)) continue;
      seen.add(model.id);
      provider.models.push(model);
    }
    next.providers[providerId] = provider;
    await save(next, set, get);
  },

  updateProvider: async (providerId, cfg) => {
    if (!providerId.trim()) throw new Error("Provider ID 不能为空");
    requireReady(get());
    const next = structuredClone(get().data);
    if (!next.providers[providerId] && providerId.includes("/")) throw new Error("Provider ID 不能包含 /，模型 ID 不受此限制");
    next.providers[providerId] = providerWithConfig(next.providers[providerId], cfg);
    await save(next, set, get);
  },

  removeProvider: async (providerId) => {
    requireReady(get());
    const next = structuredClone(get().data);
    delete next.providers[providerId];
    const removedRefs = new Set(get().data.providers[providerId]?.models.map((m) => modelRef(providerId, m.id)));
    next.enabledModels = next.enabledModels.filter((entry) => !removedRefs.has(entry));
    await save(next, set, get);
  },

  removeModel: async (providerId, modelId) => {
    requireReady(get());
    const next = structuredClone(get().data);
    const provider = next.providers[providerId];
    if (!provider) return;
    provider.models = provider.models.filter((model) => model.id !== modelId);
    next.enabledModels = next.enabledModels.filter(
      (entry) => entry !== modelRef(providerId, modelId),
    );
    await save(next, set, get);
  },

  updateModel: async (oldProviderId, oldModelId, newProviderId, model) => {
    if (!model.id.trim()) throw new Error("模型 ID 不能为空");
    requireReady(get());
    const next = structuredClone(get().data);
    const oldProvider = next.providers[oldProviderId];
    const provider = next.providers[newProviderId];
    if (!oldProvider || !provider) throw new Error("来源和目标 Provider 必须存在");
    if (!oldProvider.models.some((item) => item.id === oldModelId)) throw new Error("来源模型不存在");
    if (provider.models.some((item) => item.id === model.id && !(oldProviderId === newProviderId && item.id === oldModelId))) {
      throw new Error("目标 Provider 已有同名模型");
    }
    oldProvider.models = oldProvider.models.filter((item) => item.id !== oldModelId);
    provider.models.push(model);

    const oldRef = modelRef(oldProviderId, oldModelId);
    const newRef = modelRef(newProviderId, model.id);
    next.enabledModels = next.enabledModels.map((entry) => (entry === oldRef ? newRef : entry));
    await save(next, set, get);
  },

  fetchModels: async (baseUrl, scheme, apiKey) => {
    if (scheme !== "anthropic-messages" && scheme !== "anthropic-auth-token") {
      throw new Error("该 Provider 类型没有通用 /v1/models 接口，请手动添加模型。");
    }
    return api.fetchClaudeModels(baseUrl, scheme, apiKey);
  },
}));

async function save(
  next: ClaudeModelsCatalog,
  set: (partial: Partial<ClaudeModelsStore>) => void,
  get: () => ClaudeModelsStore,
) {
  requireReady(get());
  ++loadGeneration;
  set({ busy: true, lastError: null });
  try {
    const file = await api.writeClaudeModels(JSON.stringify(next, null, 2) + "\n", get().revision!);
    set({ path: file.path, revision: file.revision, data: normalize(JSON.parse(file.content)), lastError: null });
  } catch (error) {
    set({ lastError: error instanceof Error ? error.message : String(error) });
    throw error;
  } finally {
    set({ busy: false });
  }
}

let loadGeneration = 0;
function requireReady(state: ClaudeModelsStore) {
  if (state.status !== "ready" || state.busy || !state.revision) {
    throw new Error("请先成功加载模型目录，并等待当前保存结束");
  }
}
