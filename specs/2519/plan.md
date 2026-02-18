# Plan #2519

Approach:
- Implement story scope via task #2520 as a single focused slice across tool, agent core, and events diagnostics.

Risks:
- Medium: ambiguous payload shape could break downstream usage.

Mitigations:
- Use explicit fields (`react_response`, `emoji`, `message_id`, `reason_code`) and conformance tests.
