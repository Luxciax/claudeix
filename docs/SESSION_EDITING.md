# Claudeix 会话内容编辑设计

## 当前实现说明

第一版的真实写回、完整快照和故障恢复闭环已经实现。编辑与完整快照恢复都会同时写入 `~/.claudeix/claudeix.db`：事务状态保存在 `edit_transactions`，成功提交的修改审计保存在 `transcript_edits`。SQLite 使用 WAL 与 `synchronous=FULL`。

如果普通运行中的某一步在原 JSONL 已被替换后失败，Claudeix 会立刻使用本次操作前的完整快照做原子回滚。只有进程崩溃、断电等无法继续执行回滚的情况会留下 pending transaction；下一次启动必须先在恢复向导中处理，且该会话在此期间禁止运行、编辑、恢复和删除。

## 1. 目标

允许用户修正 Claude Code 会话中的错误上下文，而不是重开会话，同时避免破坏 transcript 的 thinking 签名、工具关联和父子关系。

编辑不是 Claudeix 本地覆盖层，也不是只改显示结果。保存成功后必须满足：

- 直接更新原始 Claude Code JSONL 的同一路径；
- Claude Code 下一次 `--resume <sessionId>` 读取到修改后的内容；
- Claudeix 重新扫描后看到的内容与磁盘原文一致；
- 修改前原文件已经完整备份，可随时恢复。

原始 JSONL 始终是唯一正文真相源。Claudeix SQLite 只保存索引、审计和恢复信息，不保存一份独立正文来假装“编辑成功”。

## 2. 第一版可编辑范围

| 内容 | 默认状态 | 原因 |
|---|---|---|
| 用户 text | 可编辑 | 风险最低，最符合“修正错误提示”场景 |
| 助手可见 text | 高级模式 | 可能改变后续上下文，但不应改签名块 |
| thinking | 禁止 | 可能带签名，修改后会导致 API 400 |
| redacted_thinking | 禁止 | 必须原样保留 |
| tool_use id/name/input | 禁止 | 会破坏 tool_result 关联和执行语义 |
| tool_result | 禁止 | 可能让历史与真实执行结果不一致 |
| sessionId / uuid / parentUuid | 禁止 | 会破坏会话链和恢复定位 |
| timestamp / usage / model | 禁止 | 元数据不应由正文编辑器改变 |

第一版不提供“删除单条消息”。需要去掉噪音时先编辑文本；物理裁剪 JSONL 行放到分支会话功能之后。

删除整个会话属于独立的回收站能力，见 `docs/SESSION_TRASH.md`。

## 3. 编辑目标定位

前端不传文件路径和行号拼接字符串，而是传结构化目标：

```ts
interface EditTarget {
  sessionId: string;
  sourcePath: string;
  recordUuid?: string;
  lineIndex: number;
  contentIndex: number | null;
  field: "content" | "text";
  expectedRecordHash: string;
  expectedFileFingerprint: string;
}
```

后端必须再次验证：

- `sourcePath` 位于配置的 Claude projects 根目录；
- 当前行仍是同一个 uuid/record；
- 当前内容 hash 与编辑器打开时一致；
- 文件 fingerprint 没有变化；
- 字段属于允许编辑白名单。

任一条件不满足则拒绝保存，并要求用户重新加载。

## 4. 写入事务

```text
1. 获取会话编辑锁
2. 确认无 Claudeix runtime / 外部 Claude 进程正在写入
3. 重新读取源文件并比较 fingerprint
4. 复制完整源文件到备份目录
5. 只解析并修改目标行
6. 其他行保持原始字节
7. 写入同文件系统临时文件
8. flush + fsync
9. 从头验证每行均为合法 JSON
10. 验证 sessionId、uuid、tool 关联和禁止字段未变化
11. 用临时文件原子替换原 `sourcePath`
12. 记录审计日志
13. 重新读取会话并刷新全文索引
14. 释放锁
```

Windows 使用可替换现有文件并要求写穿的原子移动；Unix 使用同目录 rename。

第 11 步完成后，原路径上的内容已经是编辑后的 JSONL。备份只用于恢复，不参与正常读取。

## 5. 备份

```text
~/.claudeix/backups/<sessionId>/
├─ 20260806T123000Z-before.jsonl
├─ 20260806T124512Z-before.jsonl
└─ manifest.json
```

`manifest.json` 记录：

- 原 sourcePath；
- 编辑时间；
- 文件 SHA-256；
- Claude Code 版本；
- 编辑目标；
- old/new 内容 hash；
- Claudeix 版本。

默认不自动清理备份。用户可以自行设置保留上限，但任何清理都必须先展示完整路径、文件数量和总大小，再单独确认。

## 6. 审计表

```sql
transcript_edits(
  id INTEGER PRIMARY KEY,
  session_id TEXT NOT NULL,
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
  created_at INTEGER NOT NULL
)
```

恢复旧版本也生成一条新的审计记录，不删除历史。

## 7. 恢复体系

恢复分为三层，全部真正写回原始 JSONL：

### 7.1 撤销某次消息编辑

从审计记录中取出该次修改的 `old_content`，重新执行一笔正常编辑事务：

```text
当前文件再次备份
→ 校验目标消息仍存在
→ 写回旧内容
→ 原子替换原 JSONL
→ 新增“恢复”审计记录
```

因此撤销本身也可以再次撤销，不会把历史链条抹掉。

### 7.2 恢复完整会话快照

当会话结构损坏、目标消息已经不存在，或用户希望整体回到某个时间点时，使用完整备份恢复：

```text
停止当前会话
→ 锁定 sourcePath
→ 先备份当前版本
→ 校验目标快照 SHA-256 与 JSONL
→ 原子替换原 JSONL
→ 重建索引
→ 用 Claude Code 做只读 resume 探针
```

恢复前必须显示：快照时间、当前/目标文件哈希、消息数量变化、文件大小变化和主要 diff。

### 7.3 故障自动恢复

以下情况立即提供一键恢复到“本次操作前”快照：

- 替换后 JSONL 无法重新解析；
- 会话 ID、父子链或工具关联校验失败；
- Claude Code resume 返回 transcript 结构错误；
- 应用崩溃后发现未完成的编辑事务日志。

事务日志状态：

```text
prepared → backed_up → replaced → verified → committed
```

启动时发现未进入 `committed` 的事务，Claudeix 必须先进入恢复向导，不得继续对该会话写入。

## 8. UI 流程

```text
消息右上角“编辑”
→ 显示原内容 / 新内容
→ 风险说明
→ 保存前 diff
→ 确认写入
→ 成功后刷新会话
→ 右侧“修改记录”可查看和恢复
```

修改记录提供：

- 查看前后 diff；
- 撤销这一笔；
- 恢复到此次修改前的完整快照；
- 导出当前 JSONL 或备份 JSONL；
- 校验当前会话完整性。

运行中点击编辑时显示：

```text
当前会话正在运行，停止后才能修改历史。
```

## 9. 助手消息高级编辑

用户开启高级模式后，允许只修改 `assistant.message.content[].text`，但必须满足：

- 该 turn 已结束并写入磁盘；
- 目标不是 thinking/redacted_thinking；
- 同一 content 数组中的其他块逐字节保持不变；
- tool_use 与 tool_result 关联校验通过；
- 保存后显示“可能改变后续模型理解”的高风险确认。

## 10. 失败与回滚

- 写临时文件失败：源文件不动；
- 校验失败：源文件不动，保留诊断信息；
- 原子替换失败：源文件不动；
- 替换后应用重载失败：自动提供“从本次备份恢复”；
- resume 后出现 transcript 相关 400：立即停止继续写入，提示恢复备份或创建分支。

任何恢复都必须先保留当前版本，禁止用“恢复”覆盖掉用户后来产生的新内容而没有备份。
