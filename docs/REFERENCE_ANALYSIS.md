# Monet 与 Memory Forge 参考分析

## 1. Monet 值得吸收的部分

Monet 的优势不只是“能加载会话”，而是把 Claude Code 的运行、历史和多会话监控拆成了较清楚的层：

- 自动扫描 Claude Code 原生会话；
- 原始 transcript 默认只读；
- 运行时通过 CLI 完成，不重新实现 Claude；
- 历史视图和实时视图共用消息渲染；
- 工作台集中管理多会话、权限、Runner 和外部进程跟随；
- Claude Code 的会话文件与 Monet 自己的标题、标签等元数据分开保存。

Claudeix 应吸收：

1. 原生 JSONL 是会话事实源；
2. 流式结束后重新加载 transcript；
3. 外部终端运行检测；
4. 工具调用分组、权限卡片、上下文和 token 展示；
5. 工作台复用统一消息引擎。

## 2. 不照搬 Monet 的部分

Monet 的归档/详情区域以只读预览为主，交互运行被放到 Workbench。这样便于集中管理多会话 runtime，但对于只想像聊天客户端一样打开会话继续工作的用户，多了一次不必要的跳转。

Claudeix 改为：

```text
Archive 点击会话 → ChatView 直接可输入
ChatView → 可选“加入工作台”
```

工作台是扩展能力，不是运行许可证。

## 3. Memory Forge 值得吸收的部分

- 每条消息可见的编辑入口；
- 编辑前后对照；
- 修改审计记录；
- 一键恢复旧内容；
- 会话读取与编辑 UI 清晰分离。

## 4. 不直接照搬 Memory Forge 的部分

Memory Forge 的 Claude 适配器会读取整个 JSONL、修改目标内容后重新序列化所有行，再直接写回原文件。这个实现简单，但 Claudeix 不采用，原因是：

- 写入期间崩溃可能留下半文件；
- 无文件指纹比较时，可能覆盖 Claude Code 刚写入的新内容；
- 整体重序列化会改变所有行的原始字节；
- thinking、工具链、未知字段和未来 schema 更容易被误伤；
- 缺少独立备份和原子替换边界。

Claudeix 只重写目标 JSONL 行，其余行保持原始字节；并使用备份、校验、临时文件和原子替换事务。

## 5. 最终取舍

```text
Pi Desktop：直接对话体验
Monet：会话扫描、运行监控、消息渲染、可选工作台
Memory Forge：编辑交互、审计和恢复
Claudeix：三者组合，但统一在一个 Session/Runtime 核心上
```
