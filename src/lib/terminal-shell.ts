"use client";

import { termBus, ansi } from "./terminal-bus";
import { useWorkspace } from "./workspace";

/**
 * Interactive shell line-discipline for the terminal drawer.
 * Local line editing (xterm gives us raw keys), then each entered line is
 * executed by Claudeix in the currently selected workspace.
 */

let running = false;
let buffer = "";

type TerminalCommandResult = {
  output: string;
  exitCode: number | null;
  cancelled: boolean;
  truncated: boolean;
  fullOutputPath: string | null;
};

const PROMPT = "\x1b[2m$\x1b[0m ";

export function promptLine() {
  termBus.write(PROMPT);
}

/** Handle raw key data from xterm.onData. */
export function handleTermInput(data: string) {
  if (running) {
    if (data === "\x03") {
      termBus.writeln(ansi.dim("当前命令正在执行；Claudeix 终端暂不支持中途取消。"));
    }
    return; // ignore other typing while a command runs
  }

  switch (data) {
    case "\r": {
      // Enter — run the buffered line
      termBus.write("\r\n");
      const cmd = buffer.trim();
      buffer = "";
      if (!cmd) {
        promptLine();
        return;
      }
      runBash(cmd);
      break;
    }
    case "\x7f": // Backspace
      if (buffer.length > 0) {
        buffer = buffer.slice(0, -1);
        termBus.write("\b \b");
      }
      break;
    case "\x03": // Ctrl-C at prompt — clear the line
      buffer = "";
      termBus.write("^C\r\n");
      promptLine();
      break;
    default:
      // printable chars (incl. paste); drop other control sequences
      if (data >= " " || data === "\t") {
        buffer += data;
        termBus.write(data);
      }
  }
}

async function runBash(cmd: string) {
  running = true;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const cwd = useWorkspace.getState().root || ".";
    const result = await invoke<TerminalCommandResult>("terminal_exec", { command: cmd, cwd });
    writeResult(result);
  } catch (e) {
    termBus.writeln(
      ansi.red(e instanceof Error ? e.message : "bash failed")
    );
  } finally {
    running = false;
    promptLine();
  }
}

/**
 * @param streamed chars already written from `bash_execution_update` events —
 *   only the remainder of `output` is painted. Truncated output can be *shorter*
 *   than what streamed, in which case there is nothing left to write.
 */
function writeResult(r: TerminalCommandResult) {
  if (r.output) {
    // normalize LF → CRLF for xterm; keep the prompt on its own line
    termBus.write(r.output.replace(/\r?\n/g, "\r\n"));
  }
  if (r.output && !r.output.endsWith("\n")) termBus.write("\r\n");
  if (r.cancelled) {
    termBus.writeln(ansi.dim("(cancelled)"));
  } else if (typeof r.exitCode === "number" && r.exitCode !== 0) {
    termBus.writeln(ansi.red(`exit ${r.exitCode}`));
  }
  if (r.truncated) {
    termBus.writeln(
      ansi.dim(`(output truncated${r.fullOutputPath ? ` — full log: ${r.fullOutputPath}` : ""})`)
    );
  }
}
