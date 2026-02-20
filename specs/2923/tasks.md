# Tasks: Issue #2923 - split tau-dashboard-ui lib below oversized-file threshold

1. [x] T1 (RED): capture failing oversized-file guard condition for `crates/tau-dashboard-ui/src/lib.rs` > 4000 lines.
2. [x] T2 (GREEN): move test module out of `lib.rs` to `src/tests.rs` and keep module behavior equivalent.
3. [x] T3 (GREEN): remove temporary UI exemption from `tasks/policies/oversized-file-exemptions.json`.
4. [x] T4 (REGRESSION): rerun `spec_2921` and selected regression slices (`spec_2802`, `spec_2830`, `spec_2834`, `spec_2838`, `spec_2842`, `spec_2846`, `spec_2885`, `spec_2889`, `spec_2893`, `spec_2897`, `spec_2901`, `spec_2905`, `spec_2909`, `spec_2913`, `spec_2917`).
5. [x] T5 (VERIFY): run fmt/clippy/policy guard + sanitized live validation as applicable.
