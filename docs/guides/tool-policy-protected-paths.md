## Tool Policy Protected Paths

Date: 2026-02-14  
Story: #1440  
Task: #1441

### Scope

Tool policy now enforces protected identity/system paths for file mutation tools (`write`, `edit`).

Default protected paths (per allowed root):

- `AGENTS.md`
- `SOUL.md`
- `USER.md`
- `.tau/rbac-policy.json`
- `.tau/trust-roots.json`
- `.tau/channel-policy.json`

### Deny Contract

Protected path mutation attempts fail with deterministic policy diagnostics:

- `policy_rule: "protected_path"`
- `decision: "deny"`
- `reason_code: "protected_path_denied"`
- `action: "tool:write"` or `action: "tool:edit"`
- `path` and matched `protected_path`

### Override Flow

For controlled maintenance windows, operators can allow protected mutations:

```bash
export TAU_ALLOW_PROTECTED_PATH_MUTATIONS=1
```

Additional protected paths can be appended:

```bash
export TAU_PROTECTED_PATHS=\"/abs/path/one,/abs/path/two\"
```

### Operator Diagnostics

`tool_policy_to_json` now includes:

- `protected_paths`
- `allow_protected_path_mutations`

Use existing `--print-tool-policy` workflow to verify effective configuration.

