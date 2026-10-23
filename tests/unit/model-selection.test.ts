import { expect, it, vi } from "vitest";
const bridge = vi.hoisted(() => ({ readClaudeSettings: vi.fn() }));
vi.mock("@/lib/tauri-api", () => ({ api: bridge }));
import { useClaudeModel } from "@/features/runtime/model-store";
it("does not erase an explicit provider selection when old settings load finishes", async () => {
  let resolve!: (value: unknown) => void;
  bridge.readClaudeSettings.mockReturnValue(new Promise(done => { resolve = done; }));
  const loading = useClaudeModel.getState().load();
  useClaudeModel.getState().setModel("x", "b");
  resolve({ content: '{"model":"old"}' });
  await loading;
  expect(useClaudeModel.getState().providerId).toBe("b");
  expect(useClaudeModel.getState().model).toBe("x");
});
