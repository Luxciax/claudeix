# Adapter Compatibility Matrix

> Updated: 2026-08-11  
> `Verified` means implemented in this repository and covered by its current safety model.  
> `Reference` means an external implementation exists but has not yet been ported and validated under Claudeix transactions.

| Engine | Discover | Read | Edit text | Delete turn | Truncate | Delete native session | Restore | Native resume | Storage | Status |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|
| Claude Code | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ | JSONL | **Verified baseline** |
| Pi | ✅ | ✅ | ✅ | ❌ | ❌ | ✅ | ✅ | ✅ RPC + CLI path | JSONL graph | **Read / edit / runtime / trash verified** |
| Codex | Ref | Ref | Ref | ❌ | ❌ | ❌ | ❌ | Ref | rollout JSONL | Mirror-sync required |
| OpenCode | Ref | Ref | Ref | ❌ | ❌ | ❌ | ❌ | Ref | SQLite | Storage abstraction test |
| Gemini CLI | Ref | Ref | Ref | ❌ | ❌ | ❌ | ❌ | Ref | adapter-specific | Later |
| Kiro CLI | Ref | Ref | Ref | ❌ | ❌ | ❌ | ❌ | Ref | adapter-specific | Later |
| Grok CLI | Ref | Ref | Ref | ❌ | ❌ | ❌ | ❌ | Ref | adapter-specific | Later |
| Cursor | Ref | Ref | Ref | ❌ | ❌ | ❌ | ❌ | — | adapter-specific | Later |

## Capability policy

The frontend must render actions from `EngineCapabilities`; it must not contain platform branches such as:

```ts
if (engine === "claude") { ... }
```

The initial registry intentionally reports only capabilities Claudeix can honestly provide today:

```text
Claude
✓ read sessions
✓ write sessions
✓ edit message text
✓ delete native session
✓ restore native session
✓ create/resume/stream/interrupt runtime
✓ permission handling
✗ semantic message deletion
✗ truncate-after
```

Pi now exposes transactional text editing plus true native-session recycle-bin deletion and restore. Deleted source JSONL is physically moved out of Pi's session directory into `~/.claudeix/trash/pi/`, and exchange restore preserves a conflicting current version as a second recycle-bin entry. Codex/OpenCode are not registered as writable engines until their adapters inherit the same safety guarantees.

Pi runtime is a native structured RPC transport (`pi --mode rpc`) with exact-session startup via `--session <path>`, stdout JSONL events, stdin JSONL commands, process-exit cleanup, and explicit stop/interrupt support. It reuses the proven frontend Pi RPC client but no longer re-registers the old Pi Desktop WSL/project stack.

Pi mutation safety currently includes:

```text
source revision check
→ external Pi writer probe (same session/workspace)
→ full snapshot
→ second revision check
→ temp candidate write + fsync
→ Pi structure / identity validation
→ atomic replace (write-through on Windows)
→ post-replace validation + revision verification
→ SQLite transaction status audit
→ automatic rollback on post-replace failure
→ snapshot restore transaction
→ crash recovery state detection / keep-current / restore-backup
```

## Promotion gate for every new adapter

An adapter moves from `Reference` to `Verified` only after all applicable checks pass:

```text
discover
→ parse fixture
→ parse real session read-only
→ edit copy
→ native structure validation
→ native CLI resume
→ continue one turn
→ undo
→ native CLI resume again
→ concurrent-write conflict test
→ interrupted-write recovery test
```

Delete/restore capabilities require an additional real-source recycle-bin round trip.

