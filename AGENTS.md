# Claudeix Agent Guide

This file is the repository-level implementation guide for agents working on Claudeix.

## Project

Claudeix is a lightweight Windows desktop **multi-Agent native session manager**. The UI remains the Pi Desktop-derived shell the user selected; do not redesign or replace that UI when adding engines. It uses a Tauri 2 Rust shell around a Next.js 15 static-export frontend (React 19, Tailwind 4, zustand). Package manager is pnpm.

Claude Code and Pi are the supported engines for v0.2.3. The project has `EngineRegistry` / `EngineAdapter`, a real Pi native-session adapter, a clean Pi RPC runtime at `src-tauri/src/engines/pi/runtime.rs`, an Engine-aware recycle bin at `src-tauri/src/trash/engine_service.rs`, and a persisted frontend Agent selection used by workspace/plugins/store. Distinguish this new engine implementation from old Pi/Pet/WSL compatibility files left from the original Pi Desktop repository. Do not re-register legacy `pi_bridge.rs` / WSL project-runtime files merely because the Pi engine exists. Do not physically delete legacy files without explicit user approval.

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

1. **Engine registry** — `src-tauri/src/engine/mod.rs` owns engine descriptors, capabilities, generic session APIs, and the adapter boundary. Product code must prefer capabilities over engine-id conditionals.
2. **Claude adapter** — `src-tauri/src/engines/claude.rs` exposes the mature Claude scanner/parser through the generic engine surface. Claude native JSONL under the configured Claude projects directory remains the source of truth.
3. **Pi adapter** — `src-tauri/src/engines/pi.rs` scans native Pi JSONL, rebuilds the current `id -> parentId` chain, maps it to `TranscriptRecord`, emits stable edit targets, blocks edits while an external Pi process may be writing the same session/workspace, and validates native candidates.
4. **Generic mutation transaction** — `src-tauri/src/mutation/mod.rs` provides engine-scoped write leases, source revision checks, snapshots, second conflict checks, fsync, candidate validation, Windows write-through atomic replacement, SQLite transaction state, automatic rollback, mutation restore, and crash-recovery inspection/resolution for file-backed engines.
5. **Claude runtime** — `src-tauri/src/claude/runtime_registry.rs` still owns Claude Code processes, session write leases, permission requests, external-process detection, and runtime events. Do not create a second Claude runtime for another view.
6. **Legacy Claude transcript editor** — `src-tauri/src/transcript/editor.rs` remains the production Claude write path until Claude is deliberately migrated onto the generic mutation layer. It already provides snapshots, conflict checks, atomic replacement, audit, rollback, and startup recovery.
7. **Recycle bin** — `src-tauri/src/trash/service.rs` currently handles Claude native session delete/restore/purge. Do not advertise generic delete-session capability for another engine until it has an equivalent tested path.
8. **Frontend state** — existing Pi-derived session/chat UI remains the product surface. Generic engine APIs live under `src/features/engines` and `src/lib/tauri-api.ts`; do not expose a half-working engine in the UI before its required runtime/session operations are usable.

Important invariants:

- One native session has at most one Claudeix writer at a time.
- Do not edit/delete/restore while Claudeix or an external native Agent process may be writing the session.
- Native session edits are real source edits, not overlays.
- `thinking`, `redacted_thinking`, tool IDs/results, session IDs, UUID/parent relationships, and other structural fields are not editable.
- Every destructive-looking session action must be recoverable by snapshot or recycle bin unless the user explicitly confirms permanent purge.
- Pending mutation recovery must be resolved before the affected native session is allowed to continue mutating.
- A logical message may map to more than one native record (notably Codex mirrors). Never assume one UI message equals one JSONL line.
- Do not assume all engines use files; OpenCode is the explicit SQLite test case for the abstraction.

### Frontend structure

- `src/app/page.tsx` is the current product surface and keeps the lightweight two-column session/chat layout.
- Feature code lives under `src/features/runtime`, `src/features/sessions`, `src/features/transcript`, `src/features/trash`, and `src/features/engines`.
- Next.js is configured for static export because Tauri serves static assets. Do not add server-only API routes or SSR dependencies.
- Path alias: `@/*` → `src/*`.

## Engineering rules

1. Prefer established libraries/platform primitives when they materially reduce risk, but avoid new dependencies for small local logic.
2. For normal development, run the focused affected tests plus type/Rust checks. Before a release, run the full Rust test suite, production frontend build, desktop build, and smoke tests.
3. Preserve native structure and untouched records. File-backed engines may serialize a full candidate file for atomic replacement, but adapter mutation logic must change only the intended native fields/records.
4. Never silently overwrite an externally changed session; return a conflict and require reload/recovery.
5. Do not use a separate runtime implementation for a future workbench; reuse `RuntimeRegistry`.
6. Keep old Pi/Pet/WSL compatibility files unregistered unless explicitly needed; the supported Pi engine is `src-tauri/src/engines/pi.rs`.
7. Before enabling a new engine's write capability, require fixture parsing, real native read, transactional edit, native CLI/app acceptance, snapshot restore, concurrent-change conflict, and interrupted-transaction recovery checks where applicable.
