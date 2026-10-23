# Claudeix 架构设计

> 版本说明：本文保留一期 Claude-only 设计历史。v0.2.3 已支持 Claude 与新 Pi Engine；本轮开发以 AGENTS.md 和 [选择性移植实施文档](SELECTIVE_PORT_IMPLEMENTATION.md) 为准，不据此移除现有双引擎。

## 1. 产品定位

Claudeix 是 Claude Code 的专用桌面客户端，不是会话查看器，也不是必须先把会话拖进“工作台”才能运行的调度面板。

默认流程应当和 Pi Desktop 一样直接：

```text
选择工作区 → 选择/新建会话 → 直接对话 → 工具执行 → 权限确认
```

Claudeix 只做 Claude Code，一期重点解决：

1. 自动读取 `~/.claude/projects/` 的原生会话；
2. 在普通会话页直接启动、恢复和停止 Claude Code；
3. 图形化展示文本、思考状态、工具调用、权限和执行结果；
4. 直接修改 Claude 原始会话内容，并保留备份、差异和完整恢复能力；
5. 支持会话删除、回收站、冲突恢复和受控永久删除；
6. 后续提供可选的多会话工作台，但不让工作台成为运行前置条件。

## 2. 核心设计决策

### 2.1 普通会话页可直接运行

Monet 将“归档浏览”和“交互运行”拆成两个产品区域，工作台持有运行状态、权限卡片和并行列，因此归档页只读。这是 Monet 的产品与状态管理选择，不是 Claude Code 的技术限制。

Claudeix 改为：

- `ChatView`：默认主界面，单会话直接运行；
- `WorkbenchView`：可选，多会话并行；
- `ArchiveView`：搜索、筛选和批量管理。

三个界面共享同一个 `RuntimeRegistry`，不会各自启动一套 Claude 进程。

### 2.2 一个会话只能有一个写入者

Claude Code 同一 session 被多个进程同时恢复时，消息可能交错写入同一 transcript。因此 Claudeix 对每个会话建立写入租约：

```text
RuntimeKey = claudeConfigDir + canonicalWorkspace + sessionId
```

状态：

```text
idle
starting
running
waiting_permission
stopping
external_running
editing
failed
```

规则：

- 普通会话页和工作台不能同时拥有同一会话；
- 已由 Claudeix 运行时，另一个视图只能“接管显示”，不能重复启动；
- 检测到外部终端正在写入时，只允许跟随读取、停止外部进程或创建分支；
- 会话编辑期间禁止启动和恢复该会话。

### 2.3 会话读取与会话写入彻底分层

```text
SessionScanner      只扫描文件和索引
TranscriptParser   只解析 JSONL
SessionIndex       保存标题、标签、搜索索引
RuntimeRegistry    管理 Claude Code 进程
TranscriptEditor   唯一允许改原始 JSONL 的模块
TrashService       唯一允许移动或永久删除原始会话文件的模块
```

除 `TranscriptEditor` 和 `TrashService` 外，任何模块不得改变 `~/.claude/projects/`。

## 3. 总体结构

```text
Claudeix Desktop
├─ Tauri Shell
│  ├─ window / tray / startup
│  ├─ workspace persistence
│  └─ OS process inspection
├─ Claude Locator
│  ├─ native install
│  ├─ npm / winget compatibility
│  └─ version and health probe
├─ Runtime Registry
│  ├─ acquire / release session lease
│  ├─ spawn / resume / stop
│  ├─ stream-json parser
│  ├─ permission routing
│  └─ external process follow
├─ Session Service
│  ├─ discover ~/.claude/projects
│  ├─ workspace → session index
│  ├─ transcript parser
│  ├─ file watcher
│  └─ metadata / FTS index
├─ Transcript Editor
│  ├─ editable-block policy
│  ├─ optimistic concurrency check
│  ├─ backup / temp / atomic replace
│  ├─ JSONL invariant validation
│  └─ audit / restore
├─ Trash Service
│  ├─ ownership scan / delete preview
│  ├─ journaled move to trash
│  ├─ conflict-aware restore
│  ├─ integrity verification
│  └─ guarded permanent purge
├─ Frontend Stores
│  ├─ workspace
│  ├─ sessions
│  ├─ runtime
│  ├─ chat
│  ├─ permissions
│  ├─ transcript edits
│  └─ trash
└─ React UI
   ├─ ChatView
   ├─ WorkbenchView
   ├─ ArchiveView
   ├─ message editor
   ├─ edit audit drawer
   ├─ recycle bin
   └─ tool / permission cards
```

## 4. Claude Code 接入

第一版继续使用官方 CLI 的非交互流式模式，不解析终端 TUI：

```text
claude -p
  --output-format stream-json
  --include-partial-messages
  --resume <sessionId>
```

新会话不传 `--resume`。

后端职责：

- 以工作区为 `cwd` 启动 Claude Code；
- 解析每行 JSON 事件；
- 保存返回的真实 session id；
- 将工具、权限、用量和文本转为内部协议；
- 只终止 Claudeix 自己启动的进程；
- 运行结束后以磁盘 JSONL 为最终事实源重新对账。

后续可替换为 Agent SDK，但 UI 不直接依赖 CLI 原始字段。

## 5. 内部运行事件

```ts
type RuntimeEvent =
  | { type: "session_started"; sessionId: string; cwd: string }
  | { type: "assistant_started"; messageId: string }
  | { type: "text_delta"; messageId: string; text: string }
  | { type: "thinking_status"; messageId: string; active: boolean }
  | { type: "tool_started"; id: string; name: string; input: unknown }
  | { type: "tool_finished"; id: string; output: unknown; isError: boolean }
  | { type: "permission_requested"; request: PermissionRequest }
  | { type: "usage"; inputTokens?: number; outputTokens?: number; costUsd?: number }
  | { type: "completed"; sessionId: string }
  | { type: "failed"; message: string; recoverable: boolean };
```

流式内容是临时展示层；会话完成后必须重新读取原生 JSONL，避免 UI 状态和磁盘记录长期分叉。

## 6. 会话体系

### 6.1 数据源

| 数据 | 位置 | 权限 |
|---|---|---|
| Claude 原生 transcript | `~/.claude/projects/**/*.jsonl` | 默认只读，编辑事务时写入 |
| Claudeix 索引与元数据 | `~/.claudeix/claudeix.sqlite` | 读写 |
| 编辑备份 | `~/.claudeix/backups/<sessionId>/` | 读写 |
| 会话锁 | `~/.claudeix/locks/` | 读写 |
| 会话回收站 | `<CLAUDE_CONFIG_DIR>/.claudeix-trash/<trashId>/` | 读写 |

SQLite 只保存：

- 会话别名、标签、收藏、归档；
- transcript 文件指纹；
- 全文检索索引；
- 编辑审计记录；
- UI 和运行时状态。

不复制一份“Claudeix 自有会话正文”作为第二真相源。

### 6.2 会话身份

```text
SessionIdentity
├─ configDir
├─ workspaceRoot
├─ sessionId
└─ sourcePath
```

不能只用 `sessionId`，因为自定义 `CLAUDE_CONFIG_DIR`、导入会话和不同工作区可能造成路径歧义。

## 7. UI 信息架构

### 7.1 默认聊天页

```text
┌──────────────┬──────────────────────────────────────┐
│ 工作区/会话  │ 工作区 · 模型 · 权限 · 运行状态     │
│              ├──────────────────────────────────────┤
│ 最近会话     │ 对话时间线                           │
│ 搜索/归档    │ ├─ 用户消息     [编辑]               │
│              │ ├─ Claude 文本  [高级编辑]           │
│              │ ├─ 工具卡片                         │
│              │ └─ 权限卡片                         │
│ 新建会话     ├──────────────────────────────────────┤
│ 回收站/工作台│ 输入框 · 附件 · 停止                 │
└──────────────┴──────────────────────────────────────┘
```

点击历史会话后直接进入可输入状态，不出现“在工作台打开后才能运行”。

### 7.2 工作台

工作台只增加：

- 多列并行会话；
- 会话监控总览；
- 跨会话广播；
- 并行权限处理；
- 运行队列。

它复用普通聊天页的 `ChatSurface`、`RuntimeRegistry` 和 `PermissionCenter`。

## 8. 会话内容编辑

详细规则见 `docs/SESSION_EDITING.md`。

第一版原则：

- 保存成功后原始 `sourcePath` 已被修改后的 JSONL 原子替换；
- Claude Code 后续 resume 直接读取修改后的内容；
- 默认允许编辑用户文本；
- 助手可见文本放在高级模式；
- `thinking`、`redacted_thinking`、签名、工具 ID、父子链和元数据不可编辑；
- 工具调用与工具结果第一版只读；
- 正在运行或被外部 Claude 进程占用的会话不可编辑；
- 保存前显示 diff；
- 每次写入先备份，再用临时文件校验并原子替换；
- 支持从审计记录一键恢复。

详细恢复设计包括单次撤销、完整快照恢复和未完成事务启动恢复。

## 9. 会话删除与回收站

详细规则见 `docs/SESSION_TRASH.md`。

第一版原则：

- 会话删除默认是把真实 JSONL 从 Claude projects 移入同配置目录下的 Claudeix 回收站；
- 删除前列出全部完整路径、大小和用途；
- 主会话及能够明确归属的子代理文件作为一个事务移动；
- 回收站默认不自动清空；
- 恢复回原路径前进行哈希、结构和冲突校验；
- 原路径冲突时支持“交换恢复”，保证新旧两份内容都不丢；
- 永久删除需要独立的双重确认。

## 10. 权限模型

| 模式 | 行为 |
|---|---|
| 询问 | 写文件、命令和高影响操作弹窗确认 |
| 接受编辑 | 文件编辑自动允许，命令仍询问 |
| 计划 | 只规划，不执行修改 |

`dangerously-skip-permissions` 不进入第一版默认界面。

## 11. 模块迁移

| Pi Desktop 模块 | Claudeix 处理 |
|---|---|
| Tauri 窗口/托盘 | 保留并重命名 |
| workspace.ts | 保留，去除 Pi 依赖 |
| SessionSidebar | 保留交互，替换数据源 |
| chat.ts | 拆为 RuntimeStore + TranscriptStore |
| pi_bridge.rs | 重写为 claude_runtime.rs |
| pi/sessions.ts | 重写为 session_service.ts |
| Pi SQLite 会话副本 | 删除，改为 Claude JSONL + 索引 |
| Pi 设置/模型/插件 | 移除 |
| 宠物系统 | 移除 |

## 12. 第一版验收标准

1. Windows 自动发现 Claude Code；
2. 选择并记住默认工作区；
3. 自动读取该工作区已有会话；
4. 点击历史会话即可直接继续，不需要工作台；
5. 新建会话并流式输出；
6. 展示 Bash、Read、Write、Edit 等工具卡片；
7. 支持停止和权限确认；
8. 支持用户消息编辑，且修改真正写回原始 JSONL；
9. 支持单次撤销、完整快照恢复和故障事务恢复；
10. 支持会话移入回收站、预览、恢复和冲突交换恢复；
11. 编辑、删除和恢复期间阻止并发运行；
12. 安装版与便携版可运行。

## 13. 暂不进入第一版

- 任意修改 thinking / redacted thinking；
- 修改工具调用 ID、工具输入和工具结果；
- 物理删除任意 JSONL 行；
- 从任意消息自动重建分支 session；
- Git worktree 自动隔离；
- SSH/远程主机；
- 多 Agent 适配；
- 云同步。
