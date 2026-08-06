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
- 运行中止。

尚未完成：

- 事务式会话编辑与恢复；
- 会话重命名、归档、回收站和删除恢复；
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
