# Claudeix 具体实现开发文档

## 1. 文档目的

本文档用于指导 Claudeix 从当前原型改造成可实际使用的 Claude Code 桌面客户端。

目标不是继续堆叠页面，而是建立四条稳定闭环：

1. 原生 Claude 会话读取、恢复和继续对话；
2. Claude Code 运行、流式事件、工具与权限交互；
3. 会话正文真实写回、备份、审计和完整恢复；
4. 会话删除、回收站、冲突恢复和受控永久删除。

本文档是实施顺序、接口边界和验收标准。实际代码如果需要偏离本文档，必须先修改文档并说明原因。

### 当前实现状态（2026-08-06）

- 阶段 1 Session Service：已完成；
- 阶段 2 RuntimeRegistry：已完成；
- 阶段 3 工具、权限与外部进程检测：已完成；
- 已实现单会话写入租约、显式 Claude CLI 定位、PID、进程树停止、统一事件协议和结束后磁盘对账；
- Windows npm 安装优先使用 `node + cli.js`，避免提示词经过 `cmd.exe` 二次解析；
- 已实现 `stream-json` 双向 stdin、`can_use_tool` 权限请求、允许 / 拒绝回传、并行权限队列和外部进程探针；
- 当前本机 Claude Code `2.1.219` 的手动询问模式使用 `manual`，Claudeix 兼容旧值 `default` 并统一映射；
- 阶段 4 Transcript Editor 第一批：已完成；
- 已实现用户文本真实写回、Claude 可见回复高级编辑、文件指纹与目标记录 hash 冲突检测；
- 已实现完整 JSONL 自动快照、Windows 写穿式原子替换、修改记录面板和完整快照恢复；
- 下一实施批次为阶段 5：会话回收站与删除恢复；编辑事务审计与启动故障恢复作为加固项继续补齐。

---

## 2. 当前代码基线

当前本地代码是第一轮原型，不是可发布实现。

### 2.1 已有能力

- Tauri 2 桌面壳；
- Windows 窗口、托盘和工作区选择；
- 能检测本机 `claude --version`；
- 能扫描 `~/.claude/projects/**/*.jsonl` 并提取简单会话元数据；
- 能使用 `claude -p --output-format stream-json` 启动一次任务；
- 能接收文本流并显示在简化聊天界面；
- 已完成 Claudeix 名称、包名和应用标识的初步切换。

### 2.2 当前缺陷

#### Rust 后端

- `ClaudeProc` 只有一个全局子进程，无法按会话管理；
- `claude_run` 只支持单次 prompt，不支持稳定的运行时状态机；
- 会话扫描每次递归读取全部 JSONL，缺少索引、分页和文件监听；
- 会话只读取标题和预览，不读取完整历史正文；
- 没有工具事件、权限事件和 usage 的统一解析；
- 没有外部 Claude 进程占用检测；
- 没有 TranscriptEditor、恢复事务和 TrashService；
- `lib.rs` 仍注册 Pi、宠物、插件、Pi 模型等旧模块；
- `chat_store.rs` 仍是 Pi Desktop 自有会话副本逻辑，不应成为 Claude 正文真相源。

#### 前端

- `src/app/page.tsx` 是单文件原型；
- 点击历史会话只切换 session id，不加载历史消息；
- 消息结构只有纯文本，没有工具卡片、思考状态和权限卡片；
- 没有运行状态中心；
- 没有编辑器、编辑记录、恢复入口；
- 没有删除确认和回收站页面；
- 没有错误恢复和事务恢复向导。

### 2.3 实施原则

- 先建立新模块并通过测试，再断开旧 Pi 模块；
- 旧文件不直接删除，先停止注册和引用；
- 真正删除旧模块前单独列出路径、用途和大小并确认；
- Claude 原始 JSONL 是会话正文唯一真相源；
- SQLite 只保存索引、元数据、审计和事务，不保存第二份正文；
- 任何编辑、删除和恢复都必须可中断、可检查、可恢复。

---

## 3. 第一版范围

### 3.1 必须完成

- 自动定位 Claude Code；
- 选择并记住默认工作区；
- 扫描并读取该工作区全部原生 Claude 会话；
- 点击会话后直接查看完整正文并继续对话；
- 新建会话；
- 流式显示 Claude 回复；
- 显示工具调用、工具结果、错误和 usage；
- 支持停止运行；
- 支持默认、接受编辑和计划权限模式；
- 支持编辑用户文本；
- 支持高级编辑 Claude 可见正文；
- 编辑结果真实写回原始 JSONL；
- 支持单次撤销、完整快照恢复和启动故障恢复；
- 支持会话移入回收站；
- 支持回收站预览、恢复、交换恢复和永久删除；
- 生成 Windows 便携版、NSIS 和 MSI。

### 3.2 第一版不做

- 修改 thinking、redacted thinking 或签名字段；
- 修改 tool_use id、tool_result 关联或父子链；
- 任意删除单条 JSONL 记录；
- Git worktree 自动管理；
- SSH 或远程 Claude Code；
- 多 Agent 适配；
- 云同步；
- Monet 式多列工作台完整实现。

工作台只保留架构入口，不阻塞默认聊天页运行。

---

## 4. 目标目录结构

### 4.1 Rust 后端

```text
src-tauri/src/
├─ lib.rs
├─ app_paths.rs
├─ database.rs
├─ models/
│  ├─ mod.rs
│  ├─ session.rs
│  ├─ transcript.rs
│  ├─ runtime.rs
│  ├─ edit.rs
│  └─ trash.rs
├─ claude/
│  ├─ mod.rs
│  ├─ locator.rs
│  ├─ config.rs
│  ├─ scanner.rs
│  ├─ parser.rs
│  ├─ runtime_registry.rs
│  ├─ runtime_process.rs
│  ├─ event_parser.rs
│  ├─ process_probe.rs
│  └─ commands.rs
├─ transcript/
│  ├─ mod.rs
│  ├─ editor.rs
│  ├─ validator.rs
│  ├─ backup.rs
│  ├─ recovery.rs
│  └─ commands.rs
├─ trash/
│  ├─ mod.rs
│  ├─ ownership.rs
│  ├─ service.rs
│  ├─ recovery.rs
│  └─ commands.rs
├─ workspace/
│  ├─ mod.rs
│  └─ commands.rs
└─ shell/
   ├─ tray.rs
   ├─ window.rs
   └─ external.rs
```

### 4.2 前端

```text
src/
├─ app/
│  ├─ page.tsx
│  ├─ archive/page.tsx
│  ├─ trash/page.tsx
│  └─ settings/page.tsx
├─ features/
│  ├─ workspace/
│  ├─ sessions/
│  ├─ transcript/
│  ├─ runtime/
│  ├─ permissions/
│  ├─ editing/
│  └─ trash/
├─ components/
│  ├─ layout/
│  ├─ session/
│  ├─ transcript/
│  ├─ runtime/
│  ├─ editing/
│  └─ trash/
└─ lib/
   ├─ tauri-api.ts
   ├─ event-protocol.ts
   └─ formatting.ts
```

第一阶段不要求一次性移动所有旧组件。新功能先放入新目录，旧组件在替代完成后再停止引用。

---

## 5. 核心数据模型

## 5.1 SessionIdentity

```rust
pub struct SessionIdentity {
    pub config_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub session_id: String,
    pub source_path: PathBuf,
}
```

唯一键：

```text
SHA-256(configDir + canonicalWorkspace + sessionId + canonicalSourcePath)
```

不能只使用 session id。

## 5.2 SessionSummary

```rust
pub struct SessionSummary {
    pub key: String,
    pub session_id: String,
    pub title: String,
    pub preview: String,
    pub workspace_root: String,
    pub source_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub message_count: usize,
    pub model: Option<String>,
    pub archived: bool,
    pub favorite: bool,
    pub trashed: bool,
}
```

## 5.3 TranscriptRecord

```rust
pub struct TranscriptRecord {
    pub line_index: usize,
    pub raw_type: String,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub timestamp: Option<String>,
    pub role: Option<String>,
    pub blocks: Vec<ContentBlock>,
    pub editable_targets: Vec<EditTarget>,
    pub raw_hash: String,
}
```

## 5.4 ContentBlock

```rust
pub enum ContentBlock {
    Text { text: String },
    Thinking { text: String, editable: bool },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, output: Value, is_error: bool },
    Image { media_type: Option<String> },
    SystemEvent { name: String, text: String },
    Unknown { kind: String, raw: Value },
}
```

未知块必须保留，不允许解析器丢弃。

## 5.5 RuntimeKey 与状态

```rust
pub struct RuntimeKey {
    pub config_dir: PathBuf,
    pub workspace_root: PathBuf,
    pub session_id: Option<String>,
}
```

状态：

```rust
pub enum RuntimeStatus {
    Idle,
    Starting,
    Running,
    WaitingPermission,
    Stopping,
    ExternalRunning,
    Editing,
    Trashing,
    Restoring,
    Failed,
}
```

---

## 6. SQLite 设计

数据库位置：

```text
~/.claudeix/claudeix.sqlite
```

## 6.1 sessions

```sql
CREATE TABLE sessions (
  session_key TEXT PRIMARY KEY,
  config_dir TEXT NOT NULL,
  workspace_root TEXT NOT NULL,
  session_id TEXT NOT NULL,
  source_path TEXT NOT NULL,
  title TEXT NOT NULL,
  preview TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  message_count INTEGER NOT NULL,
  source_size INTEGER NOT NULL,
  source_mtime INTEGER NOT NULL,
  source_sha256 TEXT,
  archived INTEGER NOT NULL DEFAULT 0,
  favorite INTEGER NOT NULL DEFAULT 0,
  trashed INTEGER NOT NULL DEFAULT 0
);
```

## 6.2 transcript_edits

```sql
CREATE TABLE transcript_edits (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  transaction_id TEXT NOT NULL,
  session_key TEXT NOT NULL,
  source_path TEXT NOT NULL,
  record_uuid TEXT,
  line_index INTEGER NOT NULL,
  content_index INTEGER,
  field_name TEXT NOT NULL,
  old_content TEXT NOT NULL,
  new_content TEXT NOT NULL,
  backup_path TEXT NOT NULL,
  source_hash_before TEXT NOT NULL,
  source_hash_after TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  operation TEXT NOT NULL
);
```

`operation`：`edit`、`undo`、`snapshot_restore`、`automatic_recovery`。

## 6.3 file_transactions

```sql
CREATE TABLE file_transactions (
  transaction_id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  session_key TEXT NOT NULL,
  status TEXT NOT NULL,
  source_path TEXT NOT NULL,
  temp_path TEXT,
  backup_path TEXT,
  source_hash_before TEXT,
  source_hash_after TEXT,
  payload_json TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
);
```

状态：

```text
prepared
backed_up
temp_written
validated
replaced
verified
committed
rolled_back
failed
```

## 6.4 trash_items 与 trash_files

```sql
CREATE TABLE trash_items (
  trash_id TEXT PRIMARY KEY,
  session_key TEXT NOT NULL,
  session_id TEXT NOT NULL,
  title TEXT NOT NULL,
  workspace_root TEXT NOT NULL,
  original_source_path TEXT NOT NULL,
  trash_root TEXT NOT NULL,
  status TEXT NOT NULL,
  file_count INTEGER NOT NULL,
  total_size INTEGER NOT NULL,
  deleted_at INTEGER NOT NULL,
  restored_at INTEGER
);

CREATE TABLE trash_files (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  trash_id TEXT NOT NULL,
  original_path TEXT NOT NULL,
  trash_path TEXT NOT NULL,
  purpose TEXT NOT NULL,
  size INTEGER NOT NULL,
  sha256 TEXT NOT NULL
);
```

---

## 7. Tauri 命令接口

所有前端调用统一通过 `src/lib/tauri-api.ts`，页面组件不直接散落 `invoke()`。

## 7.1 环境与工作区

```rust
claude_status() -> ClaudeStatus
claude_config() -> ClaudeConfig
workspace_get_default() -> Option<WorkspaceInfo>
workspace_pick() -> Option<WorkspaceInfo>
workspace_set_default(path: String) -> WorkspaceInfo
workspace_list_recent() -> Vec<WorkspaceInfo>
```

## 7.2 会话读取

```rust
session_scan(workspace_root: String, force: bool) -> Vec<SessionSummary>
session_get(session_key: String) -> SessionDetail
session_refresh(session_key: String) -> SessionDetail
session_rename(session_key: String, title: String) -> SessionSummary
session_archive(session_key: String, archived: bool) -> SessionSummary
session_favorite(session_key: String, favorite: bool) -> SessionSummary
```

## 7.3 Claude 运行

```rust
runtime_start(request: RuntimeStartRequest) -> RuntimeSnapshot
runtime_stop(runtime_id: String) -> RuntimeSnapshot
runtime_get(runtime_id: String) -> RuntimeSnapshot
runtime_list() -> Vec<RuntimeSnapshot>
runtime_permission_decide(request_id: String, decision: PermissionDecision) -> Result<(), String>
runtime_probe_external(session_key: String) -> ExternalRuntimeInfo
```

`RuntimeStartRequest`：

```rust
pub struct RuntimeStartRequest {
    pub workspace_root: String,
    pub session_key: Option<String>,
    pub prompt: String,
    pub permission_mode: String,
    pub model: Option<String>,
    pub effort: Option<String>,
}
```

## 7.4 会话编辑

```rust
transcript_edit_preview(request: EditRequest) -> EditPreview
transcript_edit_apply(request: EditRequest) -> EditResult
transcript_edit_history(session_key: String) -> Vec<EditAuditEntry>
transcript_edit_undo(edit_id: i64) -> RestorePreview
transcript_snapshot_list(session_key: String) -> Vec<SnapshotInfo>
transcript_snapshot_restore(snapshot_id: String) -> RestoreResult
transcript_recovery_scan() -> Vec<RecoveryIssue>
transcript_recovery_execute(transaction_id: String, action: RecoveryAction) -> RecoveryResult
```

## 7.5 回收站

```rust
trash_delete_preview(session_key: String) -> DeletePreview
trash_move(request: TrashMoveRequest) -> TrashItem
trash_list() -> Vec<TrashItem>
trash_get(trash_id: String) -> TrashDetail
trash_restore_preview(trash_id: String) -> TrashRestorePreview
trash_restore(request: TrashRestoreRequest) -> TrashRestoreResult
trash_purge_preview(trash_id: String) -> PurgePreview
trash_purge(request: PurgeRequest) -> PurgeResult
```

---

## 8. 运行时实现

## 8.1 RuntimeRegistry

替换当前全局 `ClaudeProc`。

```rust
pub struct RuntimeRegistry {
    runtimes: Mutex<HashMap<String, RuntimeHandle>>,
    session_leases: Mutex<HashMap<String, String>>,
}
```

- `runtime_id` 标识一次运行；
- `session_key -> runtime_id` 保证同会话单写入者；
- 新会话在拿到 Claude 返回的 session id 后补全租约；
- 进程结束必须释放租约；
- 应用异常退出后，启动时通过进程探针和 JSONL mtime 重新判断状态。

## 8.2 CLI 启动

第一版继续使用 CLI：

```text
claude -p <prompt>
  --output-format stream-json
  --verbose
  --include-partial-messages
  --permission-mode <mode>
  [--resume <sessionId>]
```

后端必须：

- 以 canonical workspace 为 `cwd`；
- 使用明确找到的 Claude 可执行路径，不依赖不稳定 PATH；
- Windows 不显示额外控制台窗口；
- 保存 PID、启动时间和 ExecutablePath；
- stdout、stderr 分离；
- 每个事件带 runtime id 和 session key；
- 停止时只终止 Claudeix 创建的进程树。

## 8.3 内部事件协议

统一 Tauri 事件：

```text
claudeix://runtime-event
```

```ts
type RuntimeEvent =
  | { type: "runtime_started"; runtimeId: string; sessionKey?: string }
  | { type: "session_identified"; runtimeId: string; sessionId: string; sessionKey: string }
  | { type: "assistant_started"; messageId: string }
  | { type: "text_delta"; messageId: string; text: string }
  | { type: "thinking_state"; messageId: string; active: boolean }
  | { type: "tool_started"; toolUseId: string; name: string; input: unknown }
  | { type: "tool_finished"; toolUseId: string; output: unknown; isError: boolean }
  | { type: "permission_requested"; requestId: string; tool: string; input: unknown }
  | { type: "usage"; inputTokens?: number; outputTokens?: number; costUsd?: number }
  | { type: "stderr"; text: string }
  | { type: "runtime_completed"; exitCode: number | null }
  | { type: "runtime_failed"; message: string; recoverable: boolean };
```

前端不得直接解析 Claude 原始 JSON。

## 8.4 运行结束对账

流式 UI 只是临时状态。

进程结束后：

1. 等待 JSONL 文件稳定；
2. 重新扫描当前 session；
3. 重新解析完整正文；
4. 用磁盘记录替换临时流式消息；
5. 更新搜索索引和会话摘要；
6. 若磁盘没有落地，保留临时消息并显示“等待会话写入”。

---

## 9. 会话扫描与解析

## 9.1 扫描策略

首次启动：

- 扫描所有 `~/.claude/projects/**/*.jsonl`；
- 每个文件只读取必要头尾信息生成摘要；
- 保存 size、mtime 和可选 SHA-256；
- 只对当前工作区加载完整正文。

后续刷新：

- size 和 mtime 不变时不重复解析；
- 文件变化时更新摘要和 FTS；
- 文件监听事件做 300ms 合并，避免连续写入触发多次解析。

## 9.2 解析要求

- 支持 string 与 array 两种 `message.content`；
- 保留未知顶级字段；
- 保留未知 content block；
- tool_use 与 tool_result 按 id 关联；
- system-reminder 等私有标签作为 SystemEvent 渲染；
- 不把隐藏系统记录误当用户正文；
- 标题默认使用第一条可见用户文本；
- preview 使用最近一条可见用户或助手文本；
- 解析错误只跳过坏行并记录诊断，不让整个会话消失。

## 9.3 分支与父子关系

Claude JSONL 可能包含 `uuid`、`parentUuid` 等链结构。

第一版策略：

- 构建记录图；
- 默认显示当前文件中最后一个有效叶节点的祖先链；
- 无法构建链时退回文件顺序；
- UI 标记“包含分支”；
- 编辑时只能修改当前显示链中的允许字段；
- 不在第一版创建或重写分支。

---

## 10. 会话编辑实现

详细产品规则见 `SESSION_EDITING.md`，本节描述具体代码流程。

## 10.1 EditRequest

```rust
pub struct EditRequest {
    pub session_key: String,
    pub source_path: String,
    pub line_index: usize,
    pub record_uuid: Option<String>,
    pub content_index: Option<usize>,
    pub field: String,
    pub expected_record_hash: String,
    pub expected_file_fingerprint: String,
    pub new_content: String,
}
```

## 10.2 字段白名单

默认允许：

```text
user.message.content                 string
user.message.content[i].text         text block
```

高级模式允许：

```text
assistant.message.content[i].text    visible text block
```

禁止：

```text
thinking
redacted_thinking
signature
tool_use
tool_result
uuid
parentUuid
sessionId
timestamp
model
usage
```

## 10.3 保存事务

```text
1. 获取 session lease 的编辑锁
2. 确认 RuntimeRegistry 无运行进程
3. 检测外部 Claude 进程
4. canonicalize sourcePath 并验证位于 Claude projects 根目录
5. 重新读取文件并验证 fingerprint
6. 验证目标行、uuid、字段和旧内容 hash
7. 创建 file_transactions 记录：prepared
8. 复制完整原文件到 backups：backed_up
9. 只修改目标 JSONL 行
10. 其他行保持原始字节
11. 写同目录临时文件：temp_written
12. flush + fsync
13. 逐行 JSON 校验和 invariant 校验：validated
14. 原子替换 sourcePath：replaced
15. 重新解析并比较禁止字段：verified
16. 写入 transcript_edits：committed
17. 刷新会话索引和前端
18. 释放编辑锁
```

## 10.4 临时文件命名

```text
<source>.claudeix-edit-<transactionId>.tmp
```

临时文件必须与源文件位于同一目录，保证原子替换在同一文件系统中进行。

## 10.5 Windows 替换

优先使用 Windows 原生替换语义，要求：

- 目标存在时可替换；
- 写穿磁盘；
- 不经过先删后移的窗口；
- 失败时源文件保持不变。

实现前需要做独立集成测试，确认杀进程和断电模拟下不会留下空文件。

## 10.6 撤销与快照恢复

撤销单次编辑：

- 读取对应 audit 的 `old_content`；
- 以新的编辑事务写回；
- 生成新的 `undo` 审计记录；
- 不删除原审计。

完整快照恢复：

- 将备份文件作为候选源；
- 先备份当前 sourcePath；
- 校验 session id 和 JSONL；
- 原子替换；
- 生成 `snapshot_restore` 记录。

## 10.7 启动故障恢复

应用启动时扫描非终态 `file_transactions`。

处理：

- `prepared/backed_up/temp_written/validated`：源文件通常未替换，清理临时文件并标记回滚；
- `replaced`：比较源文件和备份，提供继续验证或恢复备份；
- `verified`：补写审计并提交；
- 状态不明确：禁止该会话继续运行，进入恢复向导。

---

## 11. 回收站实现

详细产品规则见 `SESSION_TRASH.md`。

## 11.1 删除预览

`trash_delete_preview` 必须返回：

```rust
pub struct DeletePreview {
    pub session_key: String,
    pub title: String,
    pub session_id: String,
    pub workspace_root: String,
    pub runtime_status: RuntimeStatus,
    pub files: Vec<DeleteFilePreview>,
    pub total_size: u64,
    pub warnings: Vec<String>,
}
```

每个文件包含：

- 完整路径；
- 大小；
- SHA-256；
- 用途；
- 归属证据。

## 11.2 文件归属

主文件：当前 `sourcePath`。

关联文件只有在至少满足一项时才纳入：

- 文件内容明确包含 root session id；
- metadata 明确指向主 session；
- 路径和内部 id 均匹配；
- Claude 官方格式提供确定父子关系。

仅凭文件名相似不能移动。

## 11.3 回收站位置

```text
<CLAUDE_CONFIG_DIR>/.claudeix-trash/<trashId>/
├─ manifest.json
├─ payload/
│  ├─ main.jsonl
│  └─ related/...
└─ journal.json
```

优先放在同一配置目录，以便同磁盘移动。

## 11.4 删除事务

```text
1. 获取 session 删除锁
2. 确认无内部或外部运行进程
3. 重新计算文件列表与哈希
4. 用户预览结果仍匹配
5. 创建 trash manifest 和 journal
6. 逐个移动文件到 payload
7. 每移动一个文件就更新 journal
8. 校验 payload 文件数量、大小和哈希
9. 标记 session 为 trashed
10. 从正常会话索引隐藏
11. 提交事务
```

任何一步失败时根据 journal 把已移动文件移回原路径。

## 11.5 恢复策略

### 原路径不存在

- 校验 payload；
- 创建缺失父目录；
- 移回原路径；
- 校验哈希；
- 重建会话索引；
- 执行只读 resume 探针或解析验证。

### 原路径存在且哈希相同

- 判定已经恢复；
- 不覆盖；
- 更新回收站状态。

### 原路径存在且哈希不同

提供：

1. 交换恢复：把当前文件先移入新的回收站条目，再恢复旧文件；
2. 恢复为只读副本：放入 `~/.claudeix/recovery/`，不注册为可运行会话；
3. 取消。

第一版不自动改写 session id。

## 11.6 永久删除

必须再次读取并展示：

- 完整路径；
- 文件数；
- 总大小；
- 哈希；
- 删除后不可恢复说明。

确认短语：

```text
永久删除 <标题或短 sessionId>
```

永久删除只影响回收站 payload 和 manifest，不影响已经恢复的源文件。

---

## 12. 前端状态设计

## 12.1 stores

```text
workspace-store
session-store
transcript-store
runtime-store
permission-store
editing-store
trash-store
ui-store
```

页面不直接持有跨会话运行状态。

## 12.2 默认聊天页

布局：

```text
左侧：工作区 + 会话列表 + 搜索 + 回收站入口
顶部：标题 + 模型 + 权限模式 + 运行状态
中部：历史正文 + 流式正文 + 工具卡片 + 权限卡片
底部：输入框 + 附件 + 停止
```

点击会话后：

1. 加载 SessionDetail；
2. 恢复滚动位置；
3. 查询 RuntimeRegistry；
4. 若空闲，输入框立即可用；
5. 若外部运行，进入跟随状态；
6. 不出现“在工作台打开”。

## 12.3 消息编辑 UI

消息右上角菜单：

```text
复制
编辑
查看原始记录
定位 JSONL 行
```

编辑弹窗：

- 原内容只读区；
- 新内容编辑区；
- 字符和行数变化；
- diff；
- 文件路径和目标字段；
- 风险说明；
- 保存按钮。

保存成功后自动打开“修改记录”抽屉。

## 12.4 修改记录

每条记录显示：

- 时间；
- 消息角色；
- 目标行；
- 修改前后预览；
- 完整 diff；
- 备份文件；
- 撤销；
- 恢复整个快照。

## 12.5 删除会话 UI

删除按钮不能直接执行。

流程：

```text
会话菜单 → 移入回收站
→ 后端生成删除预览
→ 展示完整文件清单、用途、大小和总大小
→ 用户确认
→ 执行移动
→ 跳转下一会话
```

## 12.6 回收站页面

功能：

- 按删除时间、工作区和大小筛选；
- 预览会话标题、session id 和正文摘要；
- 查看文件清单；
- 恢复；
- 交换恢复；
- 永久删除；
- 默认无“清空回收站”快捷按钮。

---

## 13. 依赖调整

Rust 预计新增：

```toml
sha2
hex
uuid = { features = ["v4", "serde"] }
tempfile
fs2
notify
chrono
sysinfo
```

是否实际全部引入以实现阶段为准，避免无必要依赖。

前端优先复用现有：

- Zustand；
- React Virtuoso；
- Streamdown；
- Motion；
- Lucide。

第一版不引入新的大型状态框架或编辑器。

---

## 14. 旧 Pi 代码迁移计划

### 14.1 第一阶段：停止新功能依赖

- 新代码不再 import `src/lib/pi/*`；
- 新页面不再使用 Pi store；
- `Claudeix` 新模块独立工作。

### 14.2 第二阶段：停止注册

通过编译后，从 `lib.rs` 停止注册：

- Pi RPC；
- Pi 模型；
- Pi 设置；
- Pi CLI 更新；
- 宠物窗口；
- Pi SQLite 会话副本。

此阶段只断开引用，不删除文件。

### 14.3 第三阶段：清理候选

确认新版本稳定后，生成候选清单：

```text
路径
大小
用途
是否被引用
删除收益
```

得到单独确认后再删除。

---

## 15. 分阶段实施顺序

## 阶段 0：仓库整理

目标：建立可开发基线。

任务：

- 保留当前原型代码；
- 新增模块目录；
- 建立数据库迁移；
- 建立统一错误类型和事件协议；
- 将 `.local/`、构建日志等本地文件加入忽略；
- 不删除旧 Pi 文件。

验收：

- `npx tsc --noEmit` 通过；
- `cargo check` 通过；
- 当前原型仍能启动。

## 阶段 1：Session Service

目标：完整读取 Claude 原生会话。

任务：

- Claude locator；
- config dir；
- scanner；
- parser；
- session_get；
- 会话正文 UI；
- 点击历史会话加载完整消息。

验收：

- 选择已有工作区后列出会话；
- 正文数量和 JSONL 可见记录一致；
- 工具调用和工具结果正确配对；
- 坏行不会让整个会话消失。

## 阶段 2：RuntimeRegistry

目标：普通会话页直接运行。

任务：

- 替换 ClaudeProc；
- 单会话写入租约；
- 统一事件解析；
- 新建、resume、停止；
- 进程路径与 PID 记录；
- 运行结束磁盘对账。

验收：

- 点击历史会话后直接发送；
- `--resume` 使用正确 session id 与 cwd；
- 同一会话第二次启动被拒绝；
- 停止只关闭 target Claude 子进程；
- 报告实际 ExecutablePath。

## 阶段 3：工具与权限

目标：达到基本 Claude Code 客户端可用度。

任务：

- 工具卡片；
- 权限卡片；
- permission mode；
- usage；
- 错误和重试；
- 外部运行跟随。

验收：

- Bash、Read、Write、Edit 可正确显示；
- 权限请求可允许或拒绝；
- 外部 Claude 运行时不会重复 resume。

## 阶段 4：TranscriptEditor

目标：真实修改原始会话并可恢复。

任务：

- 编辑目标生成；
- 编辑预览；
- 写入事务；
- 备份；
- 原子替换；
- 审计；
- 撤销；
- 快照恢复；
- 启动故障恢复。

验收：

- 编辑后 JSONL 原文已变化；
- Claude Code resume 读取修改内容；
- 其他行和禁止字段不变化；
- 撤销和快照恢复可反复执行；
- 中断写入后源文件仍完整。

## 阶段 5：TrashService

目标：安全删除和恢复会话。

任务：

- 删除预览；
- 文件归属；
- journaled move；
- 回收站 UI；
- 原路径恢复；
- 交换恢复；
- 永久删除。

验收：

- 移入回收站后 Claude 原目录中会话消失；
- 回收站 payload 哈希与源文件一致；
- 恢复后 Claudeix 重新识别；
- 原路径冲突时两份内容都保留；
- 永久删除必须双重确认。

## 阶段 6：旧模块脱钩与发布

目标：形成 Claudeix v0.1.0。

任务：

- 停止注册 Pi 和宠物模块；
- 清理导航入口；
- 更新 README；
- 完整构建；
- 冒烟测试；
- 发布便携版、NSIS 和 MSI。

验收：

- 冷启动、工作区、历史会话、直接聊天、编辑恢复、回收站全部通过；
- `npx tsc --noEmit` 通过；
- `cargo check` 通过；
- `pnpm build` 通过；
- `pnpm tauri:build` 生成三份产物；
- 实际启动路径为 Claudeix release 产物。

---

## 16. 测试计划

## 16.1 Rust 单元测试

### parser

- string content；
- array text blocks；
- thinking；
- tool_use/tool_result；
- 未知 block；
- 坏 JSON 行；
- 多分支链；
- system-reminder；
- 图片记录。

### validator

- 禁止字段变化被拒绝；
- tool id 变化被拒绝；
- parentUuid 变化被拒绝；
- session id 变化被拒绝；
- 其他原始行保持字节一致。

### editor

- 正常保存；
- fingerprint 冲突；
- 目标行变化；
- 临时文件写失败；
- replace 失败；
- 备份恢复；
- 重复撤销。

### trash

- 正常移动；
- 中途失败回滚；
- 相同哈希恢复；
- 不同哈希交换恢复；
- 永久删除确认不匹配；
- manifest 损坏。

## 16.2 前端测试

- store 状态转换；
- 会话切换不会串消息；
- 运行中禁止编辑；
- 编辑 diff；
- 回收站冲突选项；
- 权限卡片；
- 临时流式消息与磁盘消息替换。

## 16.3 故障注入

开发构建提供隐藏测试开关：

```text
fail_after_backup
fail_after_temp_write
fail_after_replace
fail_after_first_trash_move
fail_before_trash_commit
```

用于验证事务恢复，不进入正式 UI。

## 16.4 手工验收数据

至少准备：

- 普通纯文本会话；
- 包含 Bash/Read/Edit 的会话；
- 包含图片的会话；
- 包含 thinking 的会话；
- 包含子代理的会话；
- 具有分支关系的会话；
- 一份故意损坏一行的 JSONL；
- 原路径冲突的回收站恢复场景。

测试数据必须是副本，不直接使用唯一真实会话做破坏性测试。

---

## 17. 构建与提交规则

每个阶段独立提交，不把多个高风险模块混在同一提交。

建议提交顺序：

```text
refactor: introduce Claude session service
feat: load native Claude transcripts
refactor: add session runtime registry
feat: render Claude tools and permissions
feat: add transactional transcript editing
feat: add transcript recovery workflow
feat: add session recycle bin
chore: detach legacy Pi modules
```

每次提交前：

```text
git diff --check
npx tsc --noEmit
cargo check
```

高风险提交额外运行目标模块测试。

不在同一提交中同时进行：

- 大规模文件移动；
- 会话编辑核心逻辑；
- 回收站核心逻辑；
- UI 全面重构。

---

## 18. 第一实施批次

第一批代码只完成 Session Service，不碰会话编辑和删除。

具体任务：

1. 新增 `app_paths.rs` 和 Claude config dir 解析；
2. 新增 session models；
3. 把 `claude_sessions` 拆成 scanner 和 parser；
4. 新增 `session_get` 返回完整历史；
5. 前端拆出 `session-store`、`transcript-store` 和 `tauri-api`；
6. 点击历史会话时加载正文；
7. 保持现有 `claude_run` 暂时可用；
8. 建立 parser 单元测试；
9. 通过类型检查和 cargo check；
10. 使用本机旧 Claude 工作区做只读验证。

第一批明确不做：

- 写入 JSONL；
- 删除文件；
- 停止注册旧 Pi 模块；
- 完整 RuntimeRegistry；
- Release 构建。

第一批通过后再进入 RuntimeRegistry，避免同时重写读取和运行两条链路。
