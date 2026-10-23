import { beforeEach, expect, it, vi } from "vitest";
const bridge = vi.hoisted(() => ({ readClaudeModels: vi.fn(), writeClaudeModels: vi.fn() }));
vi.mock("@/lib/tauri-api", () => ({ api: bridge }));
import { useClaudeModels } from "@/lib/claude/models";
const catalog = () => ({ version: 1, enabledModels: ["a/x", "b/x"], providers: {
  a: { api: "anthropic-messages", baseUrl: "https://a.invalid", apiKey: "test-a", models: [{ id: "x" }] },
  b: { api: "anthropic-messages", baseUrl: "https://b.invalid", models: [] },
} });
beforeEach(async () => {
  vi.resetAllMocks();
  useClaudeModels.setState({ busy: false, status: "unloaded", revision: null, parseError: null });
  bridge.readClaudeModels.mockResolvedValue({ path: "test", content: JSON.stringify(catalog()), revision: "r1" });
  bridge.writeClaudeModels.mockImplementation(async (content) => ({ path: "test", content, revision: "r2" }));
  await useClaudeModels.getState().load();
});
it("moves only the model and preserves both provider identities", async () => {
  await useClaudeModels.getState().updateModel("a", "x", "b", { id: "x" });
  const data = useClaudeModels.getState().data;
  expect(data.providers.a.models).toEqual([]);
  expect(data.providers.a.apiKey).toBe("test-a");
  expect(data.providers.b.apiKey).toBeUndefined();
  expect(data.providers.b.baseUrl).toBe("https://b.invalid");
});
it("rejects a target collision without writing", async () => {
  const data = catalog(); data.providers.b.models.push({ id: "x" } as never);
  useClaudeModels.setState({ data });
  await expect(useClaudeModels.getState().updateModel("a", "x", "b", { id: "x" })).rejects.toThrow("同名");
  expect(bridge.writeClaudeModels).not.toHaveBeenCalled();
});
it("blocks edits after a backend parse/read failure", async () => {
  bridge.readClaudeModels.mockRejectedValue(new Error("invalid catalog"));
  await useClaudeModels.getState().load();
  await expect(useClaudeModels.getState().setEnabledModels([])).rejects.toThrow("加载");
  expect(bridge.writeClaudeModels).not.toHaveBeenCalled();
});
it("does not roll back committed state or accept a second mutation during a save", async () => {
  let finish!: (value: unknown) => void;
  bridge.writeClaudeModels.mockImplementation((content) => new Promise(resolve => {
    finish = () => resolve({ path: "test", content, revision: "r2" });
  }));
  const first = useClaudeModels.getState().setEnabledModels(["a/x"]);
  await expect(useClaudeModels.getState().removeProvider("b")).rejects.toThrow("等待");
  finish(null); await first;
  expect(useClaudeModels.getState().data.enabledModels).toEqual(["a/x"]);
  expect(useClaudeModels.getState().revision).toBe("r2");
});
it("discards an older catalog load", async () => {
  let finish!: (value: unknown) => void;
  bridge.readClaudeModels.mockReturnValueOnce(new Promise(resolve => { finish = resolve; }));
  const stale = useClaudeModels.getState().load();
  bridge.readClaudeModels.mockResolvedValue({ path: "new", content: JSON.stringify(catalog()), revision: "new" });
  await useClaudeModels.getState().load();
  finish({ path: "old", content: "{}", revision: "old" }); await stale;
  expect(useClaudeModels.getState().revision).toBe("new");
});

it("keeps the last authoritative state when persistence fails", async () => {
  bridge.writeClaudeModels.mockRejectedValue(new Error("Conflict"));
  await expect(useClaudeModels.getState().removeProvider("a")).rejects.toThrow("Conflict");
  expect(useClaudeModels.getState().data.providers.a).toBeDefined();
  expect(useClaudeModels.getState().busy).toBe(false);
});

it("preserves unknown catalog fields through a normal edit", async () => {
  useClaudeModels.setState({ data: { ...catalog(), extension: { keep: true } } as ReturnType<typeof catalog> });
  await useClaudeModels.getState().setEnabledModels([]);
  expect(JSON.parse(bridge.writeClaudeModels.mock.calls[0][0]).extension).toEqual({ keep: true });
});
