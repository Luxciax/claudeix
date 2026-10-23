# Changelog

## [0.2.3] — 2026-08-11

### Fixed
- **Pi 插件 / 商店重新接入** — 原 Pi Desktop 的插件与 `pi-package` 商店实现此前被 `export default ClaudePluginsPage / ClaudeStorePage` 完全遮蔽；现在根据当前 Agent 在 Claude / Pi 原生页面之间切换。
- **Agent 选择跨页面持久化** — Claude / Pi 选择从首页局部 state 提升为全局持久状态；从 Pi 工作区进入插件、商店后不再错误加载 Claude 数据源，应用重启后也保留上次 Agent。
- **Pi settings 路径适配** — 插件/商店读取与会话 Adapter 使用同一目录规则：优先 `PI_CODING_AGENT_DIR`，否则 `~/.pi/agent`。
- **回收站入口可发现性** — Claude / Pi 会话侧栏都增加明确的“回收站 · 可恢复已删除会话”文字入口，不再只依赖 22px 图标。
- **回收站抽屉裁剪** — Claude / Pi 回收站面板改为 Portal + fixed 抽屉，脱离 SessionSidebar 的 `overflow: hidden`，避免点击后面板被 248px 侧栏裁掉。
- **NavRail Agent 状态** — 底部状态点跟随当前 Claude / Pi Agent，而不是始终显示 Claude 状态。

### Verified
- 本机 Pi `settings.json` 可读取，发现 7 个已配置 package 条目。
- npm `pi-package` 商店源返回 HTTP 200 且允许 WebView CORS。
- TypeScript 类型检查通过；Rust 44/44 测试通过。

## [0.2.2] — 2026-08-11

### Added
- **Engine recycle bin** — 新增 `EngineTrashService`，Pi 删除不再是隐藏/软删除，而是把真实原生 JSONL 从 Pi Session 目录移动到 `~/.claudeix/trash/pi/`，保存 SHA-256、原路径、workspace、session id 与 manifest。
- **Pi delete preview** — 删除前精确列出将移动的源文件、大小与占用状态；preview fingerprint 变化时拒绝执行，避免预览后文件被外部修改仍继续删除。
- **Pi restore / exchange restore** — 原路径空闲时直接恢复；原路径存在不同版本时可先将当前版本移入新的回收站条目，再恢复旧版本，两份数据都保留。
- **Engine trash crash recovery** — `.staging-*` 删除事务在下次启动自动回滚；若原路径已被不同内容占用则保留为 recovery-conflict，不猜测性覆盖。
- **Pi recycle-bin UI** — 在现有 Pi SessionSidebar 增加回收站入口与逐会话删除按钮，继续复用现有 Claudeix 删除预览交互风格。
- **Managed/external Pi PID distinction** — Claudeix 自己启动的 Pi 子进程不再被误报为外部写入者；只有当前 resume session 冲突才阻止删除/恢复。

### Verified
- TypeScript `npx tsc --noEmit` 通过。
- Rust 44/44 测试通过。
- 文件系统 round-trip 测试覆盖 `source → trash payload → restore → byte-identical source`。
- exchange restore 测试验证冲突当前版本被完整保存为新的回收站条目。

## [0.2.1] — 2026-08-11

### Added
- **Pi RPC Runtime** — 新增独立 `pi --mode rpc` 进程桥，支持 `--session <path>` 精确 Resume、JSONL stdin/stdout、stderr/exit 事件、显式 Stop，并在 Pi 自行退出后按 PID 自动清理运行状态，避免旧桥残留“幽灵进程”状态。
- **Claude / Pi Engine Switch** — 在现有 Pi → Claude UI 左侧增加轻量 Agent 切换，不重做布局；Claude 继续走原有 Claudeix 面板，Pi 使用同一套桌面交互风格。
- **Pi Native Session UI** — 左侧直接扫描 Pi 原生 Session，不再使用旧 Pi Desktop 自建 SQLite 会话历史作为事实来源；选择会话后以 `--session` 启动真实 Pi Runtime。
- **Pi Native Edit Drawer** — 复用统一 `TranscriptTimeline` 显示和编辑 Pi 原生消息；编辑前自动停止当前 Pi RPC，事务写入成功后再 Resume 原 Session，保持外部写入保护严格有效。
- **Source-authoritative runtime sync** — Pi 回合结束后从 `get_state` 获取真实 `sessionFile/sessionId`，重新扫描并读取源 JSONL，用原生 Session 刷新 UI，避免实时流状态与磁盘事实长期分叉。
- **Generic mutation recovery UI** — AppShell 启动恢复界面同时检查 Claude 事务和多 Engine mutation 事务，可保留当前结果或恢复自动备份。

### Changed
- `TranscriptTimeline` 支持传入 Agent / Source 标签，同一编辑组件可服务 Claude 与 Pi。
- `AgentPanel` 增加宿主回调和 Header Action 插槽，Pi 可以复用成熟 ModelPicker、Steer / Queue、工具调用、Extension UI 与流式交互，而无需复制聊天面板。

### Verified
- `npx tsc --noEmit` 通过。
- Rust 测试 40/40 通过。
- Next.js 静态导出已重新生成，`out/index.html` 包含最新 Engine 切换 UI。

---

## [0.2.0] — 2026-08-11

### Added
- **Multi-Agent Engine Registry** — 新增能力驱动的 `EngineRegistry` / `EngineAdapter` 架构，保留现有 Pi → Claude UI，不重做产品界面。
- **Claude Adapter** — 将现有 Claude 会话扫描、Timeline 与能力描述接入统一 Engine API，原 Claude Runtime / 编辑 / 回收站仍复用成熟实现。
- **Pi Adapter** — 支持 `PI_CODING_AGENT_DIR` / `PI_CODING_AGENT_SESSION_DIR`，扫描 Pi v3 JSONL，会按 `id → parentId` 重建当前分支并映射到统一 TranscriptRecord。
- **Pi transactional editing** — Pi 用户/助手可见文本支持稳定 `entry id + content index + record hash` 定位，候选 JSONL 只修改目标字段；编辑前会阻止可能正在写同一 session/workspace 的外部 Pi 进程。
- **Generic file mutation transaction** — 新增源 revision 检查、完整快照、二次冲突检查、临时文件 fsync、结构校验、Windows write-through 原子替换、提交后 revision 验证、自动回滚和 SQLite 状态审计。
- **Mutation restore / crash recovery** — 支持从 mutation 快照恢复；可识别 `source_before` / `source_after` / `source_unknown` / `source_missing`，并提供保留当前或恢复备份的恢复接口。
- **Pi native compatibility fixture** — 新增 Pi v3 固定夹具与可选真实 CLI 集成测试；使用 `PI_OFFLINE=1` 验证修改后的 JSONL 可被本机 `pi --session <path> --print --mode json` 原生加载。
- 新增 `docs/SESSION_FORMAT_RESEARCH.md` 与 `docs/ADAPTER_COMPATIBILITY_MATRIX.md`。

### Verified
- `npx tsc --noEmit` 通过。
- Rust 测试 36/36 通过，包括真实 Pi CLI 离线兼容探针与崩溃恢复状态检测。
- 事务测试覆盖 `snapshot → atomic replace → committed audit → restore → committed audit`。

---

The sections below are retained from the Pi Desktop UI baseline history.

## [0.5.0] — 2026-08-06

### Fixed
- **主窗口钳制到显示器工作区** — Windows 启动/托盘恢复时，窗口若被系统放置在任务栏之下或屏幕外，自动收缩并钳制回工作区内；修复窗口底部被任务栏遮挡导致输入框下半部分、模型选择器等被裁切的问题（DOM 本身无越界，纯窗口定位问题）

### Changed
- 精简桌面端聊天界面：移除 Editor/ImageViewer/Sidebar 等模块，重构为 SessionSidebar 会话侧栏

---

## [0.4.1] — 2026-08-02

### Added
- **会话自动标题生成** — 首条用户消息发送后，通过独立的临时 Pi 进程自动生成简洁会话标题（无工具/无扩展/无 session 副作用），含并发锁和超时保护
- **原生确认对话框** — 模型页面的删除确认从 `window.confirm` 改为 `@tauri-apps/plugin-dialog`，桌面端体验一致
- `pi_generate_title` Rust 命令注册 + `dialog:allow-message` capability

### Changed
- Agent 活动面板重构为紧凑单行动态指示行（`ActivityLine`），新增 `PiSpark` 旋转辐条和 `ShimmerText` 渐变动画
- `auto_retry_end` 处理逻辑改进：终端失败时将错误写入消息 transcript，避免 banner 消失后丢失上下文
- `appendAssistantError` 精确判断当前回合是否已有 assistant 消息，防止覆盖上一轮内容
- `renameSession` 同步桌面端命名至 Pi CLI 的 `set_session_name`

### Fixed
- 重试全部失败后错误信息随 banner 消失而丢失的问题

---

## [0.4.0] — 2026-08-01

### Added
- **OS 原生通知** — 通过 `tauri-plugin-notification` 实现双后端通知架构（Tauri 插件 + Web API fallback），窗口被遮挡/最小化时自动发送桌面通知
- **Steer/Queue 投递模式** — 流式回复进行中可选择 Steer（立即注入上下文）或 Queue（排队等待回合结束后发送）两种消息投递方式
- **ComposerInput 投递切换** — 新增投递模式切换药丸按钮、双发送按钮（Zap = steer, ListPlus = queue）、动态占位文字、⌘⇧⏎ 快捷键
- **Pet 状态通知** — 宠物进入 review/waiting/failed 等终态时触发桌面通知提醒用户
- **PetBubble 气泡组件** — 宠物对话气泡 UI 组件
- **model-scope 管理** — 新增 `provider/id` 规范引用格式，`modelRef()` / `hasGlobEntry()` / `isModelEnabled()` / `toggleModelEnabled()` 含旧格式兼容扩展
- **pet-restore-main** — 点击宠物气泡可恢复主窗口焦点
- 新增 `tests/pet-notifications.spec.ts` 测试覆盖

### Changed
- **通知系统重构** — `notifications.ts` 完全重写：`isWindowObscured()` 检测原生窗口可见性，`refreshNotificationPermission()` / `showViaPlugin()` 双通道
- **pet bridge** — 增加 `FALLBACK_SESSION_ID` 防止无 session 事件时宠物状态机卡死；`sessionKey()` 辅助函数；修复 body resolution 导致 idle 动画重启
- **Capabilities 扩展** — 新增 `core:window:allow-is-visible`、`core:window:allow-is-minimized`、`notification:default` 权限声明
- ComposerInput、ModelPicker、AgentPanel、MessageBubble、RetryBanner、PetSprite 等组件 UI 调整
- i18n 新增 composer/queue/pet 通知相关 key（en + zh）
- `chat.ts` / `client.ts` / `protocol.ts` / `sessions.ts` / `ext-ui.ts` 协议层重构适配 steer/queue
- `globals.css` 新增 +155 行样式
- `tests/queue.spec.ts` 重构

### Fixed
- 宠物因非模态 `extension_ui_request` 事件误入 "waiting" 状态
- idle 动画因冗余状态推送反复重启

---

## [0.3.1] — 2026-07-31

### Added
- **会话历史按项目隔离** — 侧边栏只展示当前项目的对话列表，切换项目时自动恢复该项目的最近会话上下文，避免跨项目 session 文件错乱 (`dda416f`)
- `chat_store` 新增 `project_root` 列与索引，带向后兼容迁移（旧数据自动回填到 last_project）
- 新增 `switchProject` / `flushActiveSession` API，workspace 切换时先落盘再恢复

### Changed
- `.gitignore` 添加 `.comate/` 目录

---

## [0.3.0] — 2026-07-31

### Added
- **Agent Panel scroll button** — 消息列表右下角新增悬浮滚动按钮；内容超出视口时自动显示，在底部时可跳到顶部，反之跳到底部 (`6a095a6`)
- 终端 bash 流式输出 + 子代理甲板功能 (`53a535d`)
- WSL 支持 + 虚拟滚动组件 (`0b70b9f`)

### Changed
- **Model selection UX** — 改进模型选择的可见性与交互 (`cc076db`)

### Fixed
- CI: 修复 `cc-no-brepro` wrapper 路径硬编码 (`b0b844a`)
- CI: 修复 pnpm 版本冲突 (`4f1a9c5`)

---

## [0.2.0] — 2026-07-28

### Added
- 跨平台 GitHub 发布与内置自动更新
- 自定义宠物支持、模型编辑优化
- 禅模式品牌标题
- 终端抽屉焦点保持与尺寸自适应
- streamdown 流式 Markdown 渲染

### Fixed
- 宠物窗口白屏及启动预加载问题
- 托盘交互与输入框样式
