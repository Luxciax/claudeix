# Native Session Format Research

> Status: active research  
> Baseline: Claudeix v0.1.2 (`77ed381`)  
> Updated: 2026-08-11

## 1. Purpose

Claudeix is being generalized from a Claude Code-only client into a multi-Agent native-session control center while preserving the existing Pi-derived UI.

This document records the native persistence semantics that an `EngineAdapter` must understand before it is allowed to mutate a session.

The safety rule is strict:

```text
format knowledge from reference projects
        +
Claudeix transactional writer
        =
production SessionMutator
```

Reference implementations may teach us **what needs to change**, but their direct write path is not automatically safe enough to copy.

## 2. Common abstraction

The core must not assume:

- one UI message equals one JSONL line;
- every Agent uses JSONL;
- the latest physical record list is the visible branch;
- editing one logical message changes only one native record;
- a session id uniquely identifies a source without engine/instance context.

Minimum native locator concept:

```text
NativeLocator
├─ engine instance
├─ native session identity
├─ source revision
└─ engine-private record locator
```

The engine-private locator must remain inside the adapter.

---

## 3. Claude Code

### Storage

Claudeix currently scans the configured Claude projects directory recursively for native `*.jsonl` transcripts and excludes `agent-*` subagent files.

Default conceptual location:

```text
<CLAUDE_CONFIG_DIR>/projects/**/*.jsonl
```

### Session identity

Claudeix currently derives identity from:

```text
configDir
workspaceRoot / cwd
sessionId
sourcePath
```

This should become the first concrete implementation of the future generic `SessionRef`.

### Record structure already handled by Claudeix

Relevant top-level fields include:

```text
type
uuid / id
parentUuid / parent_uuid
timestamp / createdAt / created_at
sessionId / session_id
cwd / workspaceRoot / workspace_root
message
```

Message data:

```text
message.role
message.content: string | ContentBlock[]
```

Known content blocks include:

```text
text
thinking / redacted_thinking
tool_use
tool_result
image
unknown
```

### Edit locator

Claudeix parser already retains:

```text
line_index
raw_hash
record uuid
content index / editable target
```

This is stronger than a plain line number because the editor can reject a write if the source record changed externally.

### Write policy

Current Claudeix `TranscriptEditor` remains the required safety baseline:

```text
fingerprint / hash conflict check
→ snapshot
→ minimum native mutation
→ temp file
→ parse / invariant validation
→ fsync
→ atomic replace
→ audit transaction
→ rollback / startup recovery
```

Claudeix is intentionally stricter than Memory Forge here: structural data such as thinking signatures, tool ids/results, session ids and UUID/parent relationships are not user-editable.

### Current verified capability status

```text
read session       implemented
edit text          implemented
snapshot / undo    implemented
delete session     implemented via TrashService
restore session    implemented via TrashService
native resume      implemented
delete turn        not yet implemented
truncate           not yet implemented
```

---

## 4. Pi

Memory Forge currently provides the clearest reference implementation for Pi native session parsing/editing.

### Storage and keying

Observed model:

```text
<pi sessions root>/<project key>/<session stem>.jsonl
```

Memory Forge uses:

```text
sessionKey = <project_key>::<file_stem>
```

and can resume by exact session path:

```text
pi --session "<path>"
```

### Graph semantics

Pi must not be treated as a flat transcript.

Native entries contain stable ids and may contain:

```text
id
parentId
```

Memory Forge reconstructs the current visible chain by:

```text
build id -> entry index
→ start from latest entry id
→ follow parentId backwards
→ reverse the resulting chain
```

This is important for future branch-aware delete/truncate operations.

### Entry types observed

```text
session
session_info
message
compaction
branch_summary
custom_message
```

`session` carries at least native session id and cwd. `session_info` may carry a display name.

### Message edit semantics

Memory Forge identifies editable Pi text with an adapter-private target concept equivalent to:

```text
session key
entry id
content index
field = text
```

Only user/assistant message roles are treated as normal editable text. `parentId` and other structural fields remain unchanged.

### Integration decision

Do not copy Memory Forge's final `fs::write` behavior. Port its locator/parser/branch semantics into the Claudeix transaction layer.

Pi is the preferred **second adapter** because it verifies that the core can support a branch graph while reusing the existing Pi-derived UI.

### Transactional mutation status (2026-08-11)

Pi text editing has now been moved behind the generic file mutation transaction layer. The native locator remains adapter-private (`line index + entry id + content index + record hash`), while the commit path is shared infrastructure.

Verified test flow:

```text
parse Pi v3 graph
→ produce edit target
→ build minimum JSONL candidate
→ snapshot original
→ atomic replace
→ verify session id / cwd / current parentId chain
→ audit committed
→ restore from snapshot through a second transaction
→ verify original bytes restored
```

The local Pi CLI installed on the development machine also confirms current support for:

```text
--session <path|id>
--session-id <id>
--session-dir <dir>
--continue
--resume
--fork <path|id>
--mode text|json|rpc
--print
```

This means the future runtime adapter can use an exact session path instead of relying on interactive session selection.

The CLI also rejects a resumed session before model execution when the session header's stored `cwd` no longer exists. The adapter/runtime layer must therefore surface a missing-workspace state instead of treating the transcript as normally resumable.

---

## 5. Codex

Memory Forge provides a useful current implementation for modern Codex rollout JSONL.

### Storage

Conceptual default:

```text
<CODEX_HOME>/sessions/**/*.jsonl
```

### Payload variants

Observed payload types include:

```text
message
user_message
agent_message
reasoning
function_call
function_call_output
custom tool / web search / patch events
```

### Critical mirror rule

Modern Codex data can contain multiple native representations of the same logical message.

Memory Forge explicitly deduplicates mirrored messages when building the UI and calls a mirror synchronization routine after edits.

Therefore the generic mutation model must support:

```text
1 logical EditMessage
        ↓
N native record mutations
```

An adapter that edits only the visible `message` record is incomplete.

### Runtime target

Resume command reference:

```text
codex resume <session_id>
```

The eventual runtime adapter should prefer Codex's structured app-server/native protocol when available; PTY parsing is a fallback, not the primary contract.

---

## 6. OpenCode

Memory Forge is the key reference showing why session storage cannot be modeled as `PathBuf` alone.

Observed storage is SQLite rather than one JSONL file per session.

Its edit path updates native message-part data in SQLite.

Required core implication:

```text
NativeStorageKind
├─ Jsonl
├─ Json
├─ Sqlite
├─ DirectoryTree
└─ Custom
```

The adapter owns transactions specific to its storage backend, while the common mutation layer owns higher-level snapshot/revision/audit guarantees where applicable.

---

## 7. Memory Forge audit conclusions

### Reuse

Use Memory Forge as a format-semantics reference for:

- Claude/Codex/Pi/OpenCode native locators;
- Pi current-branch reconstruction;
- Codex mirrored-message synchronization;
- heterogeneous storage backends;
- resume command discovery;
- multi-platform session listing/search concepts.

### Do not copy verbatim

Several JSONL adapters serialize the modified records and directly overwrite the native session file.

Claudeix already has a stronger write safety model, so the migration rule is:

```text
Memory Forge: how to mutate the right native records
Claudeix:     how to commit those mutations safely
```

---

## 8. Next research tasks

1. Build fixture corpus from sanitized Claude/Pi/Codex sessions.
2. Implement generic `SessionRef` + adapter-private `OriginPointer` without weakening current Claude hashes.
3. ✅ Pi read-only adapter: scan + current-branch Timeline mapping into the existing transcript record model.
4. Map existing `TranscriptEditor` into a generic mutation transaction API.
5. Enable Pi source edits only after the Pi adapter passes parse → edit → resume → undo tests.
6. Add Codex after Pi, specifically testing mirrored message synchronization.
7. Add OpenCode after file-based adapters prove the abstraction does not assume files.

## 9. Source references

- Memory Forge: https://github.com/voidcraft-dev/memory-forge-rs
- Memory Forge platform adapters: https://github.com/voidcraft-dev/memory-forge-rs/tree/main/src-tauri/src/platforms
- Monet Engine Adapter guide: https://github.com/zenolab124/monet/blob/main/ENGINE_ADAPTERS.md
- Claudeix repository: https://github.com/Luxciax/claudeix

