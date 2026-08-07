# Claudeix Agent Guide

This file is the repository-level implementation guide for agents working on Claudeix.

## Project

Claudeix is a lightweight Windows desktop client dedicated to Claude Code. It uses a Tauri 2 Rust shell around a Next.js 15 static-export frontend (React 19, Tailwind 4, zustand). Package manager is pnpm.

The repository originated from Pi Desktop, so legacy Pi/Pet/WSL files still exist. They are intentionally **not** part of the current Rust module graph, Tauri command surface, startup flow, or frontend build hooks. Do not re-register them unless the product architecture is explicitly changed. Do not physically delete legacy files without explicit user approval.

## Commands

```bash
pnpm dev            # Next.js browser preview
pnpm tauri:dev      # Full desktop app
pnpm build          # Next.js static export to out/
pnpm tauri:build    # Production desktop bundle (runs pnpm build first)
npx tsc --noEmit    # Typecheck (frontend only; tsconfig excludes src-tauri)
cargo check         # Rust check — run inside src-tauri/
cargo test --lib    # Rust unit/integration tests — run inside src-tauri/
```

On Windows, Rust commands may need the Visual Studio Build Tools environment (`VsDevCmd.bat -arch=x64`).

## Architecture

1. **Session discovery** — `src-tauri/src/claude/scanner.rs` scans Claude Code native JSONL under the configured Claude projects directory. Claude Code remains the source of truth.
2. **Transcript parsing** — `src-tauri/src/claude/parser.rs` normalizes native JSONL into frontend records while retaining source line/hash metadata needed for safe edits.
3. **RuntimeRegistry** — `src-tauri/src/claude/runtime_registry.rs` owns Claude Code processes, session write leases, permission requests, external-process detection, and runtime events. Chat and any future workbench must reuse this registry.
4. **Transcript mutations** — `src-tauri/src/transcript/editor.rs` is the only write path for native Claude session JSONL. It performs full snapshots, fingerprint/hash conflict checks, temp-file validation, fsync, atomic replacement, SQLite transaction/audit logging, automatic rollback, and startup recovery.
5. **Recycle bin** — `src-tauri/src/trash/service.rs` handles delete preview, transactional movement into `<CLAUDE_CONFIG_DIR>/.claudeix-trash`, integrity verification, conflict-aware restore, exchange restore, and guarded permanent purge.
6. **Frontend state** — session, transcript, runtime, and trash features live under `src/features/`. The default UX is the normal chat view: selecting a historical session must allow direct continuation without a workbench step.

Important invariants:

- One Claude session has at most one Claudeix writer at a time.
- Do not edit/delete/restore while Claudeix or an external Claude Code process may be writing the session.
- Native JSONL edits are real source edits, not overlays.
- `thinking`, `redacted_thinking`, tool IDs/results, session IDs, UUID/parent relationships, and other structural fields are not editable.
- Every destructive-looking session action must be recoverable by snapshot or recycle bin unless the user explicitly confirms permanent purge.
- Pending edit recovery blocks runtime/edit/delete until resolved.

### Frontend structure

- `src/app/page.tsx` is the current product surface and keeps the lightweight two-column session/chat layout.
- Feature code lives under `src/features/runtime`, `src/features/sessions`, `src/features/transcript`, and `src/features/trash`.
- Next.js is configured for static export because Tauri serves static assets. Do not add server-only API routes or SSR dependencies.
- Path alias: `@/*` → `src/*`.

## Engineering rules

1. Prefer established libraries/platform primitives when they materially reduce risk, but avoid new dependencies for small local logic.
2. For normal development, run the focused affected tests plus type/Rust checks. Before a release, run the full Rust test suite, production frontend build, desktop build, and smoke tests.
3. Never rewrite an entire Claude JSONL file merely to edit one logical message when untouched data can be preserved.
4. Never silently overwrite an externally changed session; return a conflict and require reload/recovery.
5. Do not use a separate runtime implementation for a future workbench; reuse `RuntimeRegistry`.
6. Keep Pi/Pet/WSL legacy files unregistered until physical cleanup is explicitly approved.
