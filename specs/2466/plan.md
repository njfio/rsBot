# Plan #2466 - RED/GREEN conformance for G16 phase-1 hot-reload behavior

## Approach
1. Add conformance tests first and run targeted RED command.
2. Implement #2465 behavior.
3. Re-run targeted GREEN commands and record evidence snippets.
4. Run scoped regression checks for heartbeat suite.
