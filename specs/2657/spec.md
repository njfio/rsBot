# Spec: Issue #2657 - Encrypted API-key persistence via SecretStore (G20 phase 2)

Status: Implemented

## Problem Statement
Tau currently supports encrypted credential-store records but still has API-key-at-rest pathways that can rely on plaintext profile/config fields. G20 phase 2 requires migrating persistence/retrieval behavior so stored API keys flow through encrypted secret-store handling by default.

## Acceptance Criteria

### AC-1 API key persistence avoids plaintext-at-rest profile/config writes
Given API-key auth material that must be persisted,
When persistence routines execute,
Then secrets are written only through encrypted credential-store paths and not as plaintext values in profile/config files.

### AC-2 API key retrieval supports SecretStore-backed encrypted entries
Given API keys persisted via encrypted store,
When provider auth resolves credentials,
Then retrieval succeeds through SecretStore-backed reads with no plaintext leakage in logs/errors.

### AC-3 Compatibility with explicit none/keyed policies remains deterministic
Given explicit credential store encryption configuration,
When persistence and retrieval execute,
Then behavior remains deterministic and compatible with existing none/keyed contracts.

### AC-4 Scoped verification gates pass
Given this scope,
When formatting, linting, and targeted tests run,
Then scoped `fmt`/`clippy`/test gates pass and AC mappings are evidenced.

## Scope

### In Scope
- API-key persistence/retrieval paths in provider/auth command runtime.
- SecretStore-backed integration points needed for encrypted API-key-at-rest behavior.
- Conformance/regression tests for plaintext avoidance + retrieval compatibility.

### Out of Scope
- Replacing credential store schema or introducing unrelated auth UX changes.
- Broader UI/dashboard work outside G20.

## Conformance Cases
- C-01 (conformance): persisted API-key records are encrypted at rest by default flow.
- C-02 (functional): provider auth resolution can load API key from encrypted store path.
- C-03 (regression): explicit `none` mode remains supported when configured.
- C-04 (regression): keyed/passphrase compatibility remains intact.
- C-05 (verify): scoped fmt/clippy/tests pass.

## Success Metrics / Observable Signals
- No plaintext API key persistence in migrated paths.
- Provider auth path remains backward-compatible for explicit policies.
- G20 remaining roadmap evidence is updated with this issue.
