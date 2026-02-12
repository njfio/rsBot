# Dynamic Tool Registration

## Purpose
Enable runtime-managed tool catalogs with explicit lifecycle controls so tools can be added, replaced, and removed safely during agent execution.

## Scope
Implemented in `crates/tau-agent-core/src/lib.rs` on `Agent`.

## New Runtime APIs
- `register_tool(...)`
  - now replaces existing tools by name safely
  - clears tool-result cache on replacement to prevent stale result reuse
- `has_tool(name)`
- `registered_tool_names()`
- `unregister_tool(name)`
- `clear_tools()`
- `with_scoped_tool(...)`
- `with_scoped_tools(...)`

## Scoped Lifecycle Behavior
Scoped registration helpers:
1. register temporary tools
2. clear tool-result cache before execution
3. run provided async workload
4. restore previous tool catalog state
5. clear tool-result cache again

This ensures scoped overrides do not leak stale cached outputs or persist unexpectedly after the scope exits.

## Compatibility
- Existing startup-time `register_tool(...)` usage remains unchanged.
- No CLI argument changes are required.

## Validation Coverage
Added in `crates/tau-agent-core/src/lib.rs`:
- Unit:
  - registry presence/lifecycle helpers
- Functional:
  - scoped tool visible only during scope
- Integration:
  - scoped tool lifecycle supports real prompt tool execution
- Regression:
  - scoped replacement restores original tool and avoids stale cache reuse
