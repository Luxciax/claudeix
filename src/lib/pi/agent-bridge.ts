"use client";

/**
 * Agent bridge — turns the real pi event stream into UI motion:
 *
 *  - agent_start/agent_settled        → task strip visibility (agentRunning)
 *  - tool_execution_start/end         → live task cards (read/edit/bash/…)
 *  - bash tool output                 → streamed into the terminal drawer
 *
 * The scripted showcase (`demo` / startDemo) stays untouched — this module is
 * the real counterpart driving the same surfaces.
 */

import { getPiClient } from "./client";
import { useUI } from "@/lib/store";
import { termBus, ansi } from "@/lib/terminal-bus";
import { initPetBridge } from "@/lib/pet/bridge";
import { useTerminalBlocks } from "@/lib/terminal-blocks";
import {
  BASH_TOOL,
  argCommand,
  toolDetail,
  toolTitle,
} from "./tool-label";

interface ToolRec {
  kind: "bash" | "other";
  /** chars of the bash partialResult already streamed to the terminal */
  streamed: number;
  /** block ID if in block mode */
  blockId?: string;
}

/* ── helpers ── */

function asRecord(v: unknown): Record<string, unknown> {
  return typeof v === "object" && v !== null ? (v as Record<string, unknown>) : {};
}

/** tool result / partialResult → plain text (handles content-block shapes) */
function toText(v: unknown): string {
  if (v == null) return "";
  if (typeof v === "string") return v;
  const o = asRecord(v);
  if (typeof o.output === "string") return o.output;
  if (typeof o.text === "string") return o.text;
  if (Array.isArray(o.content)) {
    return o.content
      .map((c) => {
        const b = asRecord(c);
        return typeof b.text === "string" ? b.text : "";
      })
      .join("");
  }
  try {
    return JSON.stringify(v);
  } catch {
    return String(v);
  }
}

/* ── the bridge ── */

let bridged = false;

export function initAgentBridge() {
  if (bridged) return;
  bridged = true;

  // Initialize pet bridge alongside agent bridge
  initPetBridge();

  const client = getPiClient();
  const recs = new Map<string, ToolRec>();

  client.on("agent_start", () => useUI.getState().beginAgentRun());
  const settle = () => useUI.getState().endAgentRun();
  client.on("agent_settled", settle);
  client.on("agent_end", (e) => {
    if (e.type === "agent_end" && !e.willRetry) settle();
  });

  client.on("tool_execution_start", (e) => {
    if (e.type !== "tool_execution_start") return;
    const args = asRecord(e.args);
    const ui = useUI.getState();

    // pi can start tools before/without agent_start (e.g. sub-turns)
    if (!ui.agentRunning) ui.beginAgentRun();

    const rec: ToolRec = { kind: "other", streamed: 0 };

    if (BASH_TOOL.test(e.toolName)) {
      rec.kind = "bash";
      const cmd = argCommand(args);
      ui.setTerminalOpen(true);

      const blocksStore = useTerminalBlocks.getState();
      if (blocksStore.viewMode === "blocks") {
        // blocks mode: create a command block for this agent bash
        rec.blockId = blocksStore.addBlock({
          source: "agent",
          command: cmd ?? e.toolName,
          output: "",
          status: "running",
          startedAt: Date.now(),
          toolCallId: e.toolCallId,
        });
      } else {
        // classic mode: write to xterm
        termBus.writeln(ansi.dim("$ ") + ansi.bold(cmd ?? e.toolName));
      }
    }

    recs.set(e.toolCallId, rec);
    ui.upsertAgentTask({
      id: e.toolCallId,
      title: toolTitle(e.toolName, args),
      detail: toolDetail(args),
      status: "running",
      tool: e.toolName,
    });
  });

  client.on("tool_execution_update", (e) => {
    if (e.type !== "tool_execution_update") return;
    const rec = recs.get(e.toolCallId);
    if (!rec || rec.kind !== "bash") return;

    const text = toText(e.partialResult);
    if (text.length <= rec.streamed) return;

    const delta = text.slice(rec.streamed);
    rec.streamed = text.length;

    const blocksStore = useTerminalBlocks.getState();
    if (rec.blockId && blocksStore.viewMode === "blocks") {
      // blocks mode: append to the block
      blocksStore.appendOutput(rec.blockId, delta);
    } else {
      // classic mode: write to xterm
      termBus.write(delta.replace(/\r?\n/g, "\r\n"));
    }
  });

  client.on("tool_execution_end", (e) => {
    if (e.type !== "tool_execution_end") return;
    const rec = recs.get(e.toolCallId);
    recs.delete(e.toolCallId);
    const ui = useUI.getState();

    const resultText = toText(e.result);
    ui.patchAgentTask(e.toolCallId, {
      status: e.isError ? "error" : "done",
      ...(resultText
        ? { detail: resultText.replace(/\s+/g, " ").slice(0, 80) }
        : {}),
    });

    if (!rec) return;

    if (rec.kind === "bash") {
      const text = resultText;
      const delta = text.slice(rec.streamed);

      const blocksStore = useTerminalBlocks.getState();
      if (rec.blockId && blocksStore.viewMode === "blocks") {
        // blocks mode: finalize the block
        if (delta) blocksStore.appendOutput(rec.blockId, delta);
        blocksStore.updateBlock(rec.blockId, {
          status: e.isError ? "error" : "success",
          endedAt: Date.now(),
        });
      } else {
        // classic mode: write remaining output to xterm
        if (delta.length > 0) {
          termBus.write(delta.replace(/\r?\n/g, "\r\n"));
        }
        if (!text.endsWith("\n")) termBus.writeln();
      }
      return;
    }

  });
}
