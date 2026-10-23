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
    cfg: ProviderConfig,
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
    return structuredClone(EMPTY);
  }
  const raw = value as Partial<ClaudeModelsCatalog>;
  return {
    version: 1,
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
  path: "",
  data: structuredClone(EMPTY),
  parseError: null,
  lastError: null,

  load: async () => {
    try {
      const file = await api.readClaudeModels();
      try {
        set({
          loaded: true,
          path: file.path,
          data: normalize(JSON.parse(file.content)),
          parseError: null,
          lastError: null,
        });
      } catch (error) {
        set({
          loaded: true,
          path: file.path,
          parseError: error instanceof Error ? error.message : String(error),
        });
      }
    } catch (error) {
      set({ loaded: true, lastError: error instanceof Error ? error.message : String(error) });
    }
  },

  setEnabledModels: async (enabledModels) => {
    const next = structuredClone(get().data);
    next.enabledModels = [...new Set(enabledModels)];
    await save(next, set, get);
  },

  addModel: async (providerId, cfg, model) => {
    if (!providerId.trim() || !model.id.trim() || get().parseError) return;
    const next = structuredClone(get().data);
    const provider = providerWithConfig(next.providers[providerId], cfg);
    const index = provider.models.findIndex((item) => item.id === model.id);
    if (index >= 0) provider.models[index] = model;
    else provider.models.push(model);
    next.providers[providerId] = provider;
    await save(next, set, get);
  },

  addModels: async (providerId, cfg, models) => {
    if (!providerId.trim() || get().parseError) return;
    const clean = models.filter((model) => model.id.trim());
    if (clean.length === 0) return;
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
    if (!providerId.trim() || get().parseError) return;
    const next = structuredClone(get().data);
    next.providers[providerId] = providerWithConfig(next.providers[providerId], cfg);
    await save(next, set, get);
  },

  removeProvider: async (providerId) => {
    if (get().parseError) return;
    const next = structuredClone(get().data);
    delete next.providers[providerId];
    const prefix = `${providerId}/`;
    next.enabledModels = next.enabledModels.filter((entry) => !entry.startsWith(prefix));
    await save(next, set, get);
  },

  removeModel: async (providerId, modelId) => {
    if (get().parseError) return;
    const next = structuredClone(get().data);
    const provider = next.providers[providerId];
    if (!provider) return;
    provider.models = provider.models.filter((model) => model.id !== modelId);
    next.enabledModels = next.enabledModels.filter(
      (entry) => entry !== modelRef(providerId, modelId),
    );
    await save(next, set, get);
  },

  updateModel: async (oldProviderId, oldModelId, newProviderId, cfg, model) => {
    if (!model.id.trim() || get().parseError) return;
    const next = structuredClone(get().data);
    const oldProvider = next.providers[oldProviderId];
    const oldSnapshot = oldProvider ? { ...oldProvider } : undefined;
    if (oldProvider) {
      oldProvider.models = oldProvider.models.filter((item) => item.id !== oldModelId);
      if (oldProvider.models.length === 0 && oldProviderId !== newProviderId) {
        delete next.providers[oldProviderId];
      }
    }

    const provider = providerWithConfig(
      next.providers[newProviderId],
      {
        baseUrl: cfg.baseUrl || oldSnapshot?.baseUrl || "",
        api: cfg.api || oldSnapshot?.api || CLAUDE_API_TYPES[0],
        apiKey: cfg.apiKey || oldSnapshot?.apiKey,
      },
    );
    const index = provider.models.findIndex((item) => item.id === model.id);
    if (index >= 0) provider.models[index] = model;
    else provider.models.push(model);
    next.providers[newProviderId] = provider;

    const oldRef = modelRef(oldProviderId, oldModelId);
    const newRef = modelRef(newProviderId, model.id);
    next.enabledModels = next.enabledModels.map((entry) => (entry === oldRef ? newRef : entry));
    await save(next, set, get);
  },

  fetchModels: async (baseUrl, api, apiKey) => {
    if (api !== "anthropic-messages" && api !== "anthropic-auth-token") {
      throw new Error("该 Provider 类型没有通用 /v1/models 接口，请手动添加模型。");
    }
    return apiFetch(baseUrl, apiKey);
  },
}));

async function apiFetch(baseUrl: string, apiKey?: string) {
  return api.fetchClaudeModels(baseUrl, apiKey);
}

async function save(
  next: ClaudeModelsCatalog,
  set: (partial: Partial<ClaudeModelsStore>) => void,
  get: () => ClaudeModelsStore,
) {
  const previous = get().data;
  set({ data: next, lastError: null });
  try {
    const file = await api.writeClaudeModels(JSON.stringify(next, null, 2) + "\n");
    set({ path: file.path, data: normalize(JSON.parse(file.content)), lastError: null });
  } catch (error) {
    set({
      data: previous,
      lastError: error instanceof Error ? error.message : String(error),
    });
    throw error;
  }
}
