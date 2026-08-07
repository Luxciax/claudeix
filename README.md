# Claudeix

Claudeix 是一个面向 Claude Code 的轻量 Windows 桌面客户端。

它不是多 Agent 聚合器，也不打算做成重型 IDE。第一阶段专注于：

- 选择并记住默认工作区；
- 发现该工作区已有的 Claude Code 会话；
- 恢复或新建 Claude Code 会话；
- 在桌面 UI 中运行 Claude Code；
- 流式显示输出；
- 支持计划、询问权限和接受编辑模式；
- 随时停止当前任务。

## 当前状态

`v0.1.0` 试水版本正在开发中，已完成：

- Claude Code 环境探测；
- `~/.claude/projects/` 会话发现；
- 工作区选择与最近工作区持久化；
- Claude Code `stream-json` 运行桥；
- 会话恢复参数；
- 原生 JSONL 完整历史消息回放；
- 文本、思考、工具调用、工具结果和未知记录渲染；
- 坏 JSON 行容错与解析诊断；
- 子代理文件不会重复显示成主会话；
- 点击历史会话后可直接继续对话；
- 按会话管理的 `RuntimeRegistry` 与单写入租约；
- 历史会话通过正确工作区和 session id 直接 `--resume`；
- Rust 后端统一解析文本、工具、usage、错误和会话标识事件；
- 运行状态、PID、工具执行卡片和进程树停止；
- 运行结束后重新读取原生 JSONL，与临时流式 UI 对账；
- Windows npm 安装会绕过 `.cmd`，直接运行 Node CLI 入口；
- 计划 / 手动询问 / 接受编辑权限模式；
- Claude Code `stdio` 双向权限协议；
- Bash、Write、Edit 等工具的允许 / 拒绝卡片；
- 多个并行权限请求的状态管理；
- 外部 Claude Code 进程占用检测与发送前复查；
- 外部进程退出后的手动重新检测；
- 用户历史消息可直接编辑并真实写回 Claude 原始 JSONL；
- Claude 可见回复提供默认关闭的高级编辑模式；
- 每次编辑前自动保存完整 JSONL 快照；
- 修改记录面板可恢复任意完整快照，恢复前再次备份当前版本；
- 编辑与完整快照恢复使用 SQLite 持久化事务状态机：`prepared → backed_up → replaced → verified → committed`；
- `~/.claudeix/claudeix.db` 使用 WAL + `synchronous=FULL` 保存事务和修改审计；
- 普通失败如果已经覆盖原 JSONL，会立即自动恢复本次操作前快照；
- 应用崩溃/断电留下未完成事务时，启动恢复向导可选择保留已修改版本或恢复操作前备份；
- 有未完成写入事务的会话会被后端硬锁，禁止继续运行、编辑、恢复或删除；
- 删除前展示全部待移动文件的完整路径、用途、单文件大小和总大小；
- 主会话与 `<sessionId>/` 明确归属目录中的子代理/附件事务式移入 Claude 配置目录内回收站；
- 未完成删除事务启动后自动回滚；原路径出现不同内容时转成可见 `recovery_conflict` 条目；
- 回收站支持 SHA-256 校验、安全原路径恢复和不同内容的交换恢复；
- 交换恢复会先把当前冲突版本保存成新的回收站条目，两份内容均保留；
- 回收站默认不自动清理；永久删除必须先预览完整路径，再输入指定确认短语；
- 运行中止。

尚未完成：

- 会话重命名与归档；
- Git worktree 并行会话；
- 自动更新签名。

## 环境要求

- Windows 10/11 x64；
- 已安装并登录 Claude Code；
- Node.js 与 pnpm（仅开发时需要）；
- Rust 与 Visual Studio C++ Build Tools（仅构建时需要）。

Claude Code Windows 安装：

```powershell
irm https://claude.ai/install.ps1 | iex
```

## 本地开发

```powershell
pnpm install
pnpm tauri:dev
```

可选的 Claude Code `stdio` 权限拒绝集成测试：

```powershell
pnpm test:claude-stdio
```

该测试使用 `--no-session-persistence`，并拒绝测试用 Write 请求，不应创建会话或目标文件。

## 架构文档

- [架构设计](docs/ARCHITECTURE.md)
- [开发路线](docs/ROADMAP.md)
- [具体实现计划](docs/IMPLEMENTATION_PLAN.md)
- [会话编辑设计](docs/SESSION_EDITING.md)
- [回收站设计](docs/SESSION_TRASH.md)

## 项目基座

Claudeix 从 Pi Desktop 的 Tauri + Next.js 桌面壳演化而来，但使用独立仓库和独立提交历史。后续会逐步移除未使用的 Pi 专属模块。
