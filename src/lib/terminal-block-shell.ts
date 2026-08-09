"use client";

import { useTerminalBlocks } from "@/lib/terminal-blocks";
import { termBus } from "@/lib/terminal-bus";
import { useWorkspace } from "./workspace";

/**
 * Block-mode shell handler — each command becomes a card in the blocks view.
 * Replaces the classic line-discipline when viewMode="blocks".
 */

let buffer = "";

export function handleBlockInput(data: string) {
  const blocks = useTerminalBlocks.getState();
  const runningBlock = blocks.blocks.find((b) => b.status === "running");

  // Ctrl-C: cancel the running block
  if (data === "\x03" && runningBlock) {
    return;
  }

  // backspace
  if (data === "\x7f") {
    if (buffer.length > 0) {
      buffer = buffer.slice(0, -1);
      termBus.write("\b \b");
    }
    return;
  }

  // Enter: run the command as a block
  if (data === "\r") {
    termBus.write("\r\n");
    const cmd = buffer.trim();
    buffer = "";
    if (!cmd) return;
    runBlockCommand(cmd);
    return;
  }

  // printable chars
  if (data >= " " || data === "\t") {
    buffer += data;
    termBus.write(data);
  }
}

export function blockPromptLine() {
  termBus.write("\x1b[2m$\x1b[0m ");
}

type TerminalCommandResult = {
  output: string;
  exitCode: number | null;
  cancelled: boolean;
  truncated: boolean;
  fullOutputPath: string | null;
};

async function runBlockCommand(cmd: string) {
  const blocks = useTerminalBlocks.getState();
  const blockId = blocks.addBlock({
    source: "user",
    command: cmd,
    output: "",
    status: "running",
    startedAt: Date.now(),
  });

  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const cwd = useWorkspace.getState().root || ".";
    const result = await invoke<TerminalCommandResult>("terminal_exec", { command: cmd, cwd });
    if (result.output) blocks.appendOutput(blockId, result.output);
      blocks.updateBlock(blockId, {
        status: result.cancelled
          ? "cancelled"
          : result.exitCode === 0
            ? "success"
            : "error",
        exitCode: result.exitCode ?? undefined,
        endedAt: Date.now(),
      });
  } catch (e) {
    blocks.updateBlock(blockId, {
      output: e instanceof Error ? e.message : "bash failed",
      status: "error",
      endedAt: Date.now(),
    });
  } finally {
    blockPromptLine();
  }
}
