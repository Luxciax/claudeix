# Claudeix 会话内容编辑设计

## 1. 目标

允许用户修正 Claude Code 会话中的错误上下文，而不是重开会话，同时避免破坏 transcript 的 thinking 签名、工具关联和父子关系。

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

第一版不提供“删除消息”。需要去掉噪音时先提供 Claudeix 本地隐藏；物理裁剪会话放到分支会话功能之后。

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
11. 原子替换源文件
12. 记录审计日志
13. 重新读取会话并刷新全文索引
14. 释放锁
```

Windows 使用可替换现有文件并要求写穿的原子移动；Unix 使用同目录 rename。

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

默认每个会话保留最近 20 个备份；清理备份必须单独确认。

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

## 7. UI 流程

```text
消息右上角“编辑”
→ 显示原内容 / 新内容
→ 风险说明
→ 保存前 diff
→ 确认写入
→ 成功后刷新会话
→ 右侧“修改记录”可查看和恢复
```

运行中点击编辑时显示：

```text
当前会话正在运行，停止后才能修改历史。
```

## 8. 助手消息高级编辑

用户开启高级模式后，允许只修改 `assistant.message.content[].text`，但必须满足：

- 该 turn 已结束并写入磁盘；
- 目标不是 thinking/redacted_thinking；
- 同一 content 数组中的其他块逐字节保持不变；
- tool_use 与 tool_result 关联校验通过；
- 保存后显示“可能改变后续模型理解”的高风险确认。

## 9. 失败与回滚

- 写临时文件失败：源文件不动；
- 校验失败：源文件不动，保留诊断信息；
- 原子替换失败：源文件不动；
- 替换后应用重载失败：自动提供“从本次备份恢复”；
- resume 后出现 transcript 相关 400：立即停止继续写入，提示恢复备份或创建分支。
