# Claudeix 架构设计

## 1. 产品定位

Claudeix 是面向 Claude Code 的 Windows 桌面客户端，重点解决四件事：

1. 按工作区读取、恢复和管理 Claude Code 历史会话；
2. 在图形界面中运行 Claude Code，并实时展示文本、工具调用和执行状态；
3. 将权限询问、计划模式、命令执行和错误恢复做成可理解的桌面交互；
4. 保留轻量桌面壳、最近工作区、托盘和本地优先体验。

第一版不做完整 IDE，不内置重型代码编辑器，也不同时适配其他 Agent。

## 2. 总体结构

```text
Claudeix Desktop
├─ Tauri Shell
│  ├─ window / tray / startup
│  ├─ workspace persistence
│  └─ Claude Code sidecar lifecycle
├─ Claude Bridge
│  ├─ process spawn / stop / restart
│  ├─ stream-json parser
│  ├─ permission request routing
│  └─ session resume
├─ Session Service
│  ├─ discover ~/.claude/projects
│  ├─ workspace → sessions index
│  ├─ transcript normalization
│  └─ rename / archive metadata
├─ Frontend Stores
│  ├─ workspace
│  ├─ runtime
│  ├─ chat
│  ├─ sessions
│  └─ permissions
└─ React UI
   ├─ session sidebar
   ├─ chat timeline
   ├─ tool cards
   ├─ permission dialog
   └─ composer
```

## 3. 进程接入方案

第一版使用 Claude Code CLI 的非交互流式模式，不解析彩色终端 UI。

建议启动形态：

```text
claude -p --output-format stream-json --input-format stream-json
```

后端职责：

- 在选定工作区作为 `cwd` 启动 Claude Code；
- 每行解析 JSON 事件并转换成 Claudeix 内部事件；
- 保存当前 Claude session id；
- 切换历史会话时重启进程并传入恢复参数；
- 中止任务时只终止 Claudeix 启动的 Claude Code 子进程。

后续可切换到 Claude Agent SDK，但 UI 和内部事件模型不依赖具体接入方式。

## 4. 内部事件模型

```ts
type ClaudeEvent =
  | { type: "session_started"; sessionId: string; cwd: string }
  | { type: "text_delta"; messageId: string; text: string }
  | { type: "thinking_delta"; messageId: string; text: string }
  | { type: "tool_started"; id: string; name: string; input: unknown }
  | { type: "tool_finished"; id: string; output: unknown; isError: boolean }
  | { type: "permission_requested"; request: PermissionRequest }
  | { type: "usage"; inputTokens?: number; outputTokens?: number; costUsd?: number }
  | { type: "completed" }
  | { type: "failed"; message: string };
```

前端只消费此事件模型，不直接依赖 Claude CLI 的原始 JSON 字段。

## 5. 会话体系

### 5.1 数据来源

- Claude Code 原生会话：`~/.claude/projects/`；
- Claudeix 元数据：`~/.claudeix/state.json` 或 SQLite；
- 不复制和篡改原始 Claude 会话文件。

### 5.2 会话索引

每个会话至少保存：

```text
sessionId
workspaceRoot
title
preview
createdAt
updatedAt
sourcePath
archived
```

### 5.3 第一版操作

- 按工作区列出历史会话；
- 打开并恢复会话；
- 新建会话；
- 修改 Claudeix 显示标题；
- 从侧栏移除/归档；
- 删除原始 Claude 会话暂不开放，避免误删。

## 6. 权限与安全

第一版支持三种模式：

| 模式 | 行为 |
|---|---|
| 询问 | 写文件、执行命令等高影响操作弹窗确认 |
| 接受编辑 | 文件编辑自动允许，命令仍询问 |
| 计划 | 只规划，不执行修改 |

`dangerously-skip-permissions` 不进入第一版默认界面，后续作为高级设置加入并显示持续警告。

## 7. UI 布局

```text
┌──────────────┬──────────────────────────────────────┐
│ 工作区/会话  │ 顶栏：工作区 · 模型 · 权限 · 状态   │
│              ├──────────────────────────────────────┤
│ 最近会话     │ 对话时间线                           │
│              │ ├─ 用户消息                         │
│              │ ├─ Claude 文本                      │
│              │ └─ 工具执行卡片                     │
│              ├──────────────────────────────────────┤
│ 新建会话     │ 输入框 · 附件 · 停止                 │
└──────────────┴──────────────────────────────────────┘
```

第一版延续 Pi Desktop 的轻量界面，不保留 Pi 宠物、Pi 插件商店、Pi 模型渠道配置和 Pi 专属协议页面。

## 8. 模块迁移

| Pi Desktop 模块 | Claudeix 处理 |
|---|---|
| Tauri 窗口/托盘 | 保留并重命名 |
| workspace.ts | 保留，去除 Pi 依赖 |
| SessionSidebar | 保留 UI，替换数据源 |
| chat.ts | 保留消息状态结构，替换事件解析 |
| pi_bridge.rs | 重写为 claude_bridge.rs |
| pi/sessions.ts | 重写为 claude/sessions.ts |
| Pi 设置/模型/插件 | 第一版移除入口 |
| 宠物系统 | 第一版禁用，代码后续再剥离 |

## 9. 第一版验收标准

1. Windows 上自动发现 `claude` 可执行文件；
2. 可选择并记住默认工作区；
3. 可读取该工作区已有 Claude Code 会话；
4. 可恢复会话并继续对话；
5. 可新建会话、流式输出文本；
6. 可展示 Bash、Read、Write、Edit 等工具调用卡片；
7. 可停止当前运行；
8. 错误明确显示，不静默丢失；
9. 安装版与便携版都能运行；
10. 不修改用户原始 Claude 会话文件。

## 10. 暂不进入第一版

- 内置代码编辑器；
- Git worktree 并行会话；
- SSH/远程主机；
- 多 Agent 适配；
- 云同步；
- 自动更新签名；
- MCP 图形化配置中心；
- 完整会话删除与压缩。
