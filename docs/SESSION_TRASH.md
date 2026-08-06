# Claudeix 会话回收站设计

## 1. 目标

Claudeix 支持删除整个 Claude Code 会话，但默认删除行为不是永久删除，而是将原始会话文件移入 Claudeix 回收站。

删除后必须满足：

- Claude Code 和 Claudeix 的正常会话列表中不再出现该会话；
- 原始 JSONL 及能够明确归属于该会话的关联文件完整保留；
- 可预览、导出并恢复到原始位置；
- 不自动清空回收站；
- 永久删除必须经过独立的二次确认。

## 2. 物理位置

为保证尽量使用同文件系统原子移动，每个 Claude 配置目录拥有自己的回收站：

```text
<CLAUDE_CONFIG_DIR>/.claudeix-trash/<trashId>/
├─ manifest.json
├─ operation.json
└─ payload/
   └─ projects/<original-relative-path>.jsonl
```

默认配置对应：

```text
~/.claude/.claudeix-trash/
```

Claudeix SQLite 只维护所有配置目录下回收站条目的统一索引，不把会话正文复制进数据库。

## 3. 回收对象

每次删除先执行归属扫描，只移动能够确定属于目标 session 的文件：

- 主会话 `sourcePath`；
- 元数据中明确引用该根 session 的子代理 transcript；
- Claudeix 为该会话生成的索引、别名、收藏和运行状态；
- 其他能够通过 session ID、root session ID 或显式引用证明归属的附件。

无法证明归属的文件不得猜测性移动，必须留在原位并在删除预览中标记为“未处理”。

## 4. 删除预览与确认

点击“移入回收站”后，先显示：

```text
会话标题
sessionId
工作区
所有待移动文件的完整路径
每个文件的大小和用途
文件总数与总大小
正在运行/被占用状态
```

只有用户确认后才开始移动。运行中的会话必须先停止；外部 Claude 进程正在写入时禁止删除。

## 5. 移入回收站事务

```text
1. 获取 session 写入租约
2. 确认无 Claudeix 或外部 Claude 写入者
3. 枚举并锁定所有目标文件
4. 为每个文件记录路径、大小、mtime 和 SHA-256
5. 创建 trash staging 目录与 operation journal
6. 将文件移动到 staging/payload，并保留原相对路径
7. 重新计算并核对全部 SHA-256
8. 写入 manifest.json 并 fsync
9. 将 staging 原子改名为最终 trashId 目录
10. 更新 SQLite 索引并刷新会话列表
11. 标记事务 committed
```

同卷使用 rename。自定义 `CLAUDE_CONFIG_DIR` 的回收站也位于该配置目录内，因此正常情况下不会跨卷。

任何一步失败都按 journal 将已经移动的文件放回原路径。只有所有文件验证完成后，UI 才显示“已移入回收站”。

## 6. manifest

```json
{
  "trashId": "uuid",
  "sessionId": "...",
  "title": "...",
  "workspaceRoot": "...",
  "configDir": "...",
  "deletedAt": 0,
  "claudeCodeVersion": "...",
  "claudeixVersion": "...",
  "files": [
    {
      "originalPath": "...",
      "payloadPath": "...",
      "purpose": "main_transcript",
      "size": 0,
      "sha256": "..."
    }
  ]
}
```

## 7. 恢复流程

### 7.1 原路径空闲

```text
锁定回收站条目
→ 校验 payload 哈希
→ 创建恢复 journal
→ 按原路径移回全部文件
→ 校验恢复后哈希
→ 重建索引
→ Claude Code resume 探针
→ 标记 restored
```

### 7.2 原路径存在相同文件

如果 SHA-256 完全一致，视为内容已经恢复：

- 不重复覆盖；
- 重新建立索引；
- 将回收站条目标记为“重复副本”；
- 由用户决定是否保留回收站副本。

### 7.3 原路径存在不同文件

禁止静默覆盖，提供两种安全选择：

1. **交换恢复**：先把当前冲突文件作为新的回收站条目移走，再把旧会话恢复到原路径；两份内容都保留。
2. **只恢复为副本**：将 payload 恢复到 Claudeix recovery 目录供查看和导出，但不注册为可 resume 会话。

第一版不自动改写 sessionId 来制造两个可运行副本，因为这会涉及 UUID、父子链、工具关联和子代理引用的完整迁移。

## 8. 回收站 UI

回收站页面显示：

- 会话标题、sessionId、工作区；
- 删除时间、文件数、总大小；
- 主 JSONL 消息预览；
- 是否包含子代理或关联文件；
- 完整性校验状态；
- 恢复、导出、查看清单、永久删除。

支持搜索、按工作区筛选、批量恢复和批量导出。批量操作仍需逐条显示冲突结果。

## 9. 永久删除

默认关闭自动清理，回收站内容永久保留，直到用户主动处理。

永久删除流程：

```text
第一次：展示所有完整路径、文件数量、每项用途和总大小
第二次：输入“永久删除 <会话标题或短 sessionId>”
第三步：再次校验回收站 payload 哈希后执行删除
```

永久删除只删除回收站 payload 与对应 manifest，不影响已经恢复到 Claude 原路径的文件。

可选设置允许用户启用 30/90/180 天自动清理，但默认关闭；自动清理启用前必须明确说明不可恢复，并允许排除收藏条目。

## 10. 数据库索引

```sql
trash_items(
  trash_id TEXT PRIMARY KEY,
  config_dir TEXT NOT NULL,
  session_id TEXT NOT NULL,
  title TEXT,
  workspace_root TEXT,
  manifest_path TEXT NOT NULL,
  deleted_at INTEGER NOT NULL,
  restored_at INTEGER,
  total_files INTEGER NOT NULL,
  total_size INTEGER NOT NULL,
  status TEXT NOT NULL
)
```

状态：

```text
moving
trashed
restoring
restored
conflict
corrupt
purging
```

## 11. 验收测试

必须覆盖：

- 单文件会话删除与恢复；
- 主会话加多个子代理文件；
- 删除中途崩溃后自动回滚；
- 恢复中途崩溃后继续或回滚；
- 原路径同内容冲突；
- 原路径不同内容的交换恢复；
- 自定义 `CLAUDE_CONFIG_DIR`；
- 中文、空格和超长 Windows 路径；
- 外部 Claude 正在写入时禁止删除；
- 永久删除双重确认与完整路径预览。
