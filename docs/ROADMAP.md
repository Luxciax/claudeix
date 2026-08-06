# Claudeix 开发路线

## 阶段 A：架构冻结

- 建立独立仓库；
- 确认 CLI/SDK 接入方式；
- 定义内部事件协议；
- 定义会话索引与权限模型；
- 明确第一版删除和保留范围。

## 阶段 B：第一版可运行骨架

- 品牌与包名改为 Claudeix；
- 新增 Claude Code 环境探测；
- 将 Pi Rust Bridge 替换为 Claude Bridge；
- 支持启动、发送、停止和流式事件；
- 保留工作区选择与最近工作区。

## 阶段 C：历史会话

- 扫描 `~/.claude/projects/`；
- 按工作区匹配会话；
- 读取并归一化历史消息；
- 恢复 session id；
- 新建、重命名和归档会话。

## 阶段 D：工具与权限

- Bash / Read / Write / Edit 工具卡片；
- 权限确认弹窗；
- 计划、询问、接受编辑模式；
- 错误、重试和中止状态。

## 阶段 E：第一版发布

- Windows release 构建；
- 便携版、NSIS、MSI；
- 冒烟测试；
- GitHub Release；
- 收集真实使用反馈后再决定编辑器、worktree 和远程功能。
