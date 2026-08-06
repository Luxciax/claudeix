import { execFileSync, spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { createInterface } from "node:readline";

const projectRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const targetPath = join(projectRoot, ".local", "claudeix-permission-deny-test.txt");

if (existsSync(targetPath)) {
  throw new Error(`Refusing to run because the test target already exists: ${targetPath}`);
}

function firstWhere(name) {
  return execFileSync("where.exe", [name], { encoding: "utf8" })
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);
}

function locateClaude() {
  if (process.platform !== "win32") {
    return { command: "claude", prefixArgs: [] };
  }

  const wrapper = firstWhere("claude.cmd");
  const npmRoot = dirname(wrapper);
  const scripts = [
    join(npmRoot, "node_modules", "@anthropic-ai", "claude-code", "cli.js"),
    join(npmRoot, "node_modules", "@cometix", "claude-code", "cli.js"),
  ];
  const script = scripts.find(existsSync);
  if (!script) {
    return { command: wrapper, prefixArgs: [] };
  }
  const bundledNode = join(npmRoot, "node.exe");
  const node = existsSync(bundledNode) ? bundledNode : firstWhere("node.exe");
  return { command: node, prefixArgs: [script] };
}

const { command, prefixArgs } = locateClaude();
const args = [
  ...prefixArgs,
  "-p",
  "--no-session-persistence",
  "--max-turns",
  "2",
  "--output-format",
  "stream-json",
  "--input-format",
  "stream-json",
  "--verbose",
  "--permission-prompt-tool",
  "stdio",
  "--permission-mode",
  "manual",
  "--tools",
  "Write",
];

const child = spawn(command, args, {
  cwd: projectRoot,
  stdio: ["pipe", "pipe", "pipe"],
  windowsHide: true,
});

let sawPermission = false;
let sawSession = false;
let sawResult = false;
let stderr = "";

const timeout = setTimeout(() => {
  child.kill();
}, 120_000);

child.stderr.setEncoding("utf8");
child.stderr.on("data", (chunk) => {
  stderr += chunk;
});

const lines = createInterface({ input: child.stdout });
lines.on("line", (line) => {
  let event;
  try {
    event = JSON.parse(line);
  } catch {
    return;
  }

  if (event.type === "system" && event.subtype === "init" && event.session_id) {
    sawSession = true;
  }
  if (
    event.type === "control_request" &&
    event.request?.subtype === "can_use_tool"
  ) {
    sawPermission = true;
    console.log("PERMISSION_REQUEST_RECEIVED");
    child.stdin.write(
      `${JSON.stringify({
        type: "control_response",
        response: {
          subtype: "success",
          request_id: event.request_id,
          response: {
            behavior: "deny",
            message: "Claudeix stdio integration test denial",
            interrupt: false,
          },
        },
      })}\r\n`,
    );
  }
  if (event.type === "result") {
    sawResult = true;
    console.log("RESULT_RECEIVED");
    child.stdin.end();
  }
});

child.stdin.write(
  `${JSON.stringify({
    type: "user",
    session_id: "",
    message: {
      role: "user",
      content: [
        {
          type: "text",
          text: `You must use the Write tool once to create ${targetPath} with the text CLAUDEIX_PERMISSION_TEST. Do not use another tool.`,
        },
      ],
    },
    parent_tool_use_id: null,
  })}\r\n`,
);

child.on("exit", (code) => {
  clearTimeout(timeout);
  lines.close();

  const failures = [];
  if (!sawSession) failures.push("no session init event was received");
  if (!sawPermission) failures.push("no can_use_tool permission request was received");
  if (!sawResult) failures.push("no result event was received");
  if (existsSync(targetPath)) failures.push(`denied write still created ${targetPath}`);
  if (code !== 0) failures.push(`Claude Code exited with code ${code}`);

  if (failures.length > 0) {
    console.error(failures.join("\n"));
    if (stderr.trim()) console.error(stderr.trim());
    process.exitCode = 1;
    return;
  }

  console.log("CLAUDE_STDIO_PERMISSION_DENY_OK");
});
