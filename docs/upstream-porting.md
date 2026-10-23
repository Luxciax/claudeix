# Upstream selective port tracking

Baseline: Claudeix cfdb8181e829669c4d5da29543ec3981466e7a78 on cloud-build/0.2.3.
Implementation contract: [SELECTIVE_PORT_IMPLEMENTATION.md](SELECTIVE_PORT_IMPLEMENTATION.md).

The upstream version/commit associations below are candidates supplied in the proposal. Their source SHA and license have not yet been verified. Local fixes do not claim to copy upstream code.

| Candidate | Local surface | Decision | Evidence / remaining work |
|---|---|---|---|
| v0.15 recycle lifecycle | Claude Trash + EngineTrash | Already Have / Adapt | Existing round-trip, stale-preview, recovery and exchange tests; missing-case audit pending |
| v0.16 pathless diff | Tool inspection/navigation | Deferred | B7; locate active consumers before implementation |
| v0.17 Pi CLI updater | Existing Pi engine | Skip for this port | No upstream CLI update lifecycle import |
| v0.18 models UX | Claude catalog/store/page | Adapt | Native reliability changes in progress; only additive sync preview currently implemented |
| v0.18.1 write-only credentials | Claude Provider | Planned | B3 Vault and migration pending |
| 987524c background console policy | process_utils + active callers | Adapt | Uses existing helper; upstream SHA unverified |
| Remote/Pet/Android/launcher | None | Skip | Outside current scope; retain existing new Pi engine |

No reviewed upstream SHA has been recorded yet. On the next upstream audit, first verify the repository and tags, then record the exact reviewed range and license before copying code.
