# Plan #2476 - implement startup template builtin fallback and source diagnostics

## Approach
1. Add built-in startup template constant from a checked-in `.md.j2` asset.
2. Add template diagnostics model (`source`, `template_path`, `reason_code`) to startup prompt composition output.
3. Implement source-resolution flow:
   - try workspace template
   - if unavailable/invalid/render-failed, try builtin template
   - if builtin fails, fall back to default composition
4. Add C-01..C-03 tests and keep `spec_2471` tests intact.

## Interfaces
- Extend `StartupPromptComposition` with `template_report`.
- Keep public function signatures stable.

## Risks
- Risk: compatibility regressions in no-template path.
  Mitigation: explicit regression test plus existing `spec_2471` suite execution.
