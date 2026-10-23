# Selective port implementation status

Baseline: cfdb818, cloud-build/0.2.3.
Contract: [SELECTIVE_PORT_IMPLEMENTATION.md](SELECTIVE_PORT_IMPLEMENTATION.md).

## First local implementation batch

- B0: checked out the intended branch; retained both engines; added version notices to historical design documents and an upstream tracking table. Upstream SHA/license verification remains pending; no upstream code copied.
- B1: removed provider-config arguments from model moves; retained empty source providers; rejected duplicate model targets; added explicit load status, stale-load generations, mutation busy state, authoritative saves and revision checking. Windows writers share a persistent exclusive-open lock file; in-process writers are serialized. Temporary files are unique to an operation. Unknown catalog fields survive saves. Failed reads do not become writable empty state.
- B1 preview: added added/existing/not-returned categories and revision invalidation. This first increment only applies selected additions; not-returned models and existing metadata remain untouched. Full paginated discovery, explicit removals and metadata reconciliation are still pending. Authentication headers now follow discovery type, requests time out, redirects are refused and remote error bodies are not returned.
- B2 first integration: Provider/Model deletion uses ConfirmDialog; model-page dialogs trap focus, handle Escape, block closing/repeated actions during saves, preserve drafts and show errors. Focus behavior is implemented but not yet tested in a desktop EXE. Broader dangerous-action adoption remains pending.
- B4 preparation: the active Claude runtime and pi_settings call sites now use hide_console; added a Windows console probe test. Existing Pi runtime and terminal behavior retained.

## Validation

- Frontend: 8 unit tests pass (model move/identity, collision, read failure, concurrent save, stale load, persistence failure, extension preservation, stale settings versus explicit selection).
- pnpm exec tsc --noEmit: passes.
- cargo check --tests: passes on Windows.
- cargo test --lib --locked: 48 passed, 0 failed, including the catalog revision/extension regression and Windows no-console probe. Environment-gated real-CLI tests retain their existing opt-in behavior; this is not evidence of a real Pi/Claude CLI acceptance run.
- git diff --check: passes.
- Windows cloud build and desktop UI smoke: pending; no push/dispatch or local production package build performed.

The baseline Rust lockfile needed additional dependency entries when resolving the existing reqwest default feature graph. No Cargo.toml dependency upgrade was made. The resulting lockfile is included so subsequent verification can use --locked. Vitest 3.2.4 is pinned as a development dependency and its unit command is included in the existing Windows CI.

## Remaining work / stopping boundary

The full B0–B6 program is not complete. Next acceptance boundary is a successful Windows cloud build and desktop smoke for this first batch. After that: finish the remaining B1 discovery contract, B3 Credential Vault/schema migration, B5 session coverage audit, B6 release/updater. B7 remains deferred.

Credentials remain in the existing v1 catalog until B3; this batch must not be advertised as write-only or Vault-backed. No production credentials, native sessions, backups or trash were modified. No release pipeline was triggered. Existing updater installation remains unavailable.
