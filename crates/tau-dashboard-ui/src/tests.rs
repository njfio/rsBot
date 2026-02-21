use super::{
    contains_markdown_contract_syntax, extract_assistant_stream_tokens,
    extract_first_fenced_code_block, render_tau_ops_dashboard_shell,
    render_tau_ops_dashboard_shell_for_route, render_tau_ops_dashboard_shell_with_context,
    TauOpsDashboardAlertFeedRow, TauOpsDashboardAuthMode, TauOpsDashboardChatMessageRow,
    TauOpsDashboardChatSessionOptionRow, TauOpsDashboardChatSnapshot,
    TauOpsDashboardCommandCenterSnapshot, TauOpsDashboardConnectorHealthRow,
    TauOpsDashboardMemoryGraphEdgeRow, TauOpsDashboardMemoryGraphNodeRow, TauOpsDashboardRoute,
    TauOpsDashboardSessionGraphEdgeRow, TauOpsDashboardSessionGraphNodeRow,
    TauOpsDashboardSessionTimelineRow, TauOpsDashboardShellContext, TauOpsDashboardSidebarState,
    TauOpsDashboardTheme,
};

#[test]
fn unit_contains_markdown_contract_syntax_rejects_plain_text() {
    assert!(!contains_markdown_contract_syntax("plain response"));
}

#[test]
fn unit_contains_markdown_contract_syntax_accepts_fenced_code_only() {
    assert!(contains_markdown_contract_syntax(
        "```rust\nfn main() {}\n```"
    ));
}

#[test]
fn unit_contains_markdown_contract_syntax_rejects_pipe_without_table_delimiter() {
    assert!(!contains_markdown_contract_syntax("left|right"));
}

#[test]
fn unit_contains_markdown_contract_syntax_accepts_each_non_table_marker_path() {
    assert!(contains_markdown_contract_syntax("# heading"));
    assert!(contains_markdown_contract_syntax("intro\n# heading"));
    assert!(contains_markdown_contract_syntax("- item"));
    assert!(contains_markdown_contract_syntax("intro\n- item"));
    assert!(contains_markdown_contract_syntax(
        "[docs](https://example.com)"
    ));
}

#[test]
fn unit_extract_first_fenced_code_block_parses_language_and_code_payload() {
    assert_eq!(
        extract_first_fenced_code_block("prefix ```rust\nfn main() {}\n``` suffix"),
        Some(("rust".to_string(), "fn main() {}".to_string()))
    );
}

#[test]
fn unit_extract_assistant_stream_tokens_normalizes_whitespace() {
    assert_eq!(
        extract_assistant_stream_tokens("stream   one\ntwo"),
        vec!["stream".to_string(), "one".to_string(), "two".to_string()]
    );
}

#[test]
fn unit_extract_assistant_stream_tokens_ignores_blank_content() {
    assert!(extract_assistant_stream_tokens("   \n\t  ").is_empty());
}

#[test]
fn functional_render_shell_includes_foundation_markers() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-shell\""));
    assert!(html.contains("id=\"tau-ops-header\""));
    assert!(html.contains("id=\"tau-ops-sidebar\""));
    assert!(html.contains("id=\"tau-ops-command-center\""));
}

#[test]
fn regression_render_shell_includes_prd_component_contract_markers() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-component=\"HealthBadge\""));
    assert!(html.contains("data-component=\"StatCard\""));
    assert!(html.contains("data-component=\"AlertFeed\""));
    assert!(html.contains("data-component=\"DataTable\""));
}

#[test]
fn spec_c01_deploy_route_renders_wizard_root_and_steps() {
    let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
    assert!(html.contains("id=\"tau-ops-deploy-panel\""));
    assert!(html.contains("id=\"tau-ops-deploy-wizard-steps\""));
    assert!(html.contains("data-wizard-step=\"model\""));
    assert!(html.contains("data-wizard-step=\"review\""));
    assert!(html.contains("aria-hidden=\"false\""));
}

#[test]
fn spec_c02_deploy_route_renders_model_catalog_marker() {
    let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
    assert!(html.contains("id=\"tau-ops-deploy-model-catalog\""));
    assert!(html.contains("data-component=\"ModelCatalogDropdown\""));
}

#[test]
fn spec_c03_deploy_route_renders_validation_and_review_markers() {
    let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
    assert!(html.contains("id=\"tau-ops-deploy-validation\""));
    assert!(html.contains("data-component=\"StepValidation\""));
    assert!(html.contains("id=\"tau-ops-deploy-review\""));
    assert!(html.contains("data-component=\"DeployReviewSummary\""));
}

#[test]
fn spec_c04_deploy_route_renders_deploy_action_marker() {
    let html = render_tau_ops_dashboard_shell_for_route("/ops/deploy");
    assert!(html.contains("id=\"tau-ops-deploy-submit\""));
    assert!(html.contains("data-action=\"deploy-agent\""));
    assert!(html.contains("data-success-redirect-template=\"/ops/agents/{agent_id}\""));
}

#[test]
fn spec_c05_non_deploy_route_hides_deploy_panel_markers() {
    let html = render_tau_ops_dashboard_shell_for_route("/ops");
    assert!(html.contains("id=\"tau-ops-deploy-panel\""));
    assert!(html.contains("aria-hidden=\"true\""));
    assert!(html.contains("data-panel-visible=\"false\""));
}

#[test]
fn spec_c01_stream_contract_declares_websocket_connect_on_load() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-stream-contract\""));
    assert!(html.contains("data-stream-transport=\"websocket\""));
    assert!(html.contains("data-stream-connect-on-load=\"true\""));
}

#[test]
fn spec_c02_stream_contract_declares_heartbeat_target() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-heartbeat-target=\"tau-ops-kpi-grid\""));
}

#[test]
fn spec_c03_stream_contract_declares_alert_feed_target() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-alert-feed-target=\"tau-ops-alert-feed-list\""));
}

#[test]
fn spec_c04_stream_contract_declares_chat_token_stream_without_polling() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-chat-stream-mode=\"websocket\""));
    assert!(html.contains("data-chat-polling=\"disabled\""));
}

#[test]
fn spec_c05_stream_contract_declares_connector_health_target() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-connector-health-target=\"tau-ops-connector-table-body\""));
}

#[test]
fn spec_c06_stream_contract_declares_reconnect_backoff_strategy() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-reconnect-strategy=\"exponential-backoff\""));
    assert!(html.contains("data-reconnect-base-ms=\"250\""));
    assert!(html.contains("data-reconnect-max-ms=\"8000\""));
}

#[test]
fn spec_c01_accessibility_contract_section_marker_exists() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-accessibility-contract\""));
    assert!(html.contains("data-axe-contract=\"required\""));
}

#[test]
fn spec_c02_accessibility_keyboard_navigation_markers_exist() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-skip-to-main\""));
    assert!(html.contains("data-keyboard-navigation=\"true\""));
}

#[test]
fn spec_c03_accessibility_live_region_markers_exist() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-live-announcer\""));
    assert!(html.contains("aria-live=\"polite\""));
    assert!(html.contains("aria-atomic=\"true\""));
}

#[test]
fn spec_c04_accessibility_focus_indicator_markers_exist() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-focus-visible-contract=\"true\""));
    assert!(html.contains("data-focus-ring-token=\"tau-focus-ring\""));
}

#[test]
fn spec_c05_accessibility_reduced_motion_marker_exists() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-reduced-motion-contract=\"prefers-reduced-motion\""));
    assert!(html.contains("data-reduced-motion-behavior=\"suppress-nonessential-animation\""));
}

#[test]
fn spec_c01_performance_contract_declares_wasm_budget_marker() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-performance-contract\""));
    assert!(html.contains("data-wasm-budget-gzip-kb=\"500\""));
}

#[test]
fn spec_c02_performance_contract_declares_lcp_budget_marker() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-lcp-budget-ms=\"1500\""));
}

#[test]
fn spec_c03_performance_contract_declares_layout_shift_skeleton_markers() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-layout-shift-budget=\"0.00\""));
    assert!(html.contains("data-layout-shift-mitigation=\"skeletons\""));
}

#[test]
fn spec_c04_performance_contract_declares_websocket_processing_budget_marker() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("data-websocket-process-budget-ms=\"50\""));
}

#[test]
fn functional_spec_2786_c03_shell_exposes_auth_bootstrap_markers() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-auth-shell\""));
    assert!(html.contains("data-auth-mode=\"token\""));
    assert!(html.contains("data-login-required=\"true\""));
    assert!(html.contains("id=\"tau-ops-login-shell\""));
    assert!(html.contains("id=\"tau-ops-protected-shell\""));
}

#[test]
fn conformance_spec_2786_c03_shell_login_route_marks_login_panel_visible() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::PasswordSession,
        active_route: TauOpsDashboardRoute::Login,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(html.contains("data-auth-mode=\"password-session\""));
    assert!(html.contains("data-active-route=\"login\""));
    assert!(html.contains("id=\"tau-ops-login-shell\""));
    assert!(html.contains("aria-hidden=\"false\""));
    assert!(html.contains("id=\"tau-ops-protected-shell\""));
}

#[test]
fn regression_spec_2786_c03_shell_none_mode_marks_auth_not_required() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::None,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(html.contains("data-auth-mode=\"none\""));
    assert!(html.contains("data-login-required=\"false\""));
}

#[test]
fn functional_spec_2790_c01_sidebar_includes_14_ops_route_links() {
    let html = render_tau_ops_dashboard_shell();
    assert_eq!(html.matches("data-nav-item=").count(), 14);

    let expected_routes = [
        "/ops",
        "/ops/agents",
        "/ops/agents/default",
        "/ops/chat",
        "/ops/sessions",
        "/ops/memory",
        "/ops/memory-graph",
        "/ops/tools-jobs",
        "/ops/channels",
        "/ops/config",
        "/ops/training",
        "/ops/safety",
        "/ops/diagnostics",
        "/ops/deploy",
    ];

    for route in expected_routes {
        assert!(
            html.contains(&format!("href=\"{route}\"")),
            "missing nav route {route}"
        );
    }
}

#[test]
fn functional_spec_2790_c02_breadcrumb_markers_reflect_ops_route() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-breadcrumbs\""));
    assert!(html.contains("data-breadcrumb-current=\"command-center\""));
    assert!(html.contains("id=\"tau-ops-breadcrumb-current\""));
}

#[test]
fn functional_spec_2790_c03_breadcrumb_markers_reflect_login_route() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::PasswordSession,
        active_route: TauOpsDashboardRoute::Login,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(html.contains("id=\"tau-ops-breadcrumbs\""));
    assert!(html.contains("data-breadcrumb-current=\"login\""));
    assert!(html.contains("id=\"tau-ops-breadcrumb-current\""));
}

#[test]
fn functional_spec_2794_c02_c03_route_context_tokens_match_expected_values() {
    let route_cases = [
        (TauOpsDashboardRoute::Ops, "ops", "command-center"),
        (TauOpsDashboardRoute::Agents, "agents", "agent-fleet"),
        (
            TauOpsDashboardRoute::AgentDetail,
            "agent-detail",
            "agent-detail",
        ),
        (TauOpsDashboardRoute::Chat, "chat", "chat"),
        (TauOpsDashboardRoute::Sessions, "sessions", "sessions"),
        (TauOpsDashboardRoute::Memory, "memory", "memory"),
        (
            TauOpsDashboardRoute::MemoryGraph,
            "memory-graph",
            "memory-graph",
        ),
        (TauOpsDashboardRoute::ToolsJobs, "tools-jobs", "tools-jobs"),
        (TauOpsDashboardRoute::Channels, "channels", "channels"),
        (TauOpsDashboardRoute::Config, "config", "config"),
        (TauOpsDashboardRoute::Training, "training", "training"),
        (TauOpsDashboardRoute::Safety, "safety", "safety"),
        (
            TauOpsDashboardRoute::Diagnostics,
            "diagnostics",
            "diagnostics",
        ),
        (TauOpsDashboardRoute::Deploy, "deploy", "deploy"),
        (TauOpsDashboardRoute::Login, "login", "login"),
    ];

    for (route, expected_active_route, expected_breadcrumb) in route_cases {
        let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
            auth_mode: TauOpsDashboardAuthMode::Token,
            active_route: route,
            theme: TauOpsDashboardTheme::Dark,
            sidebar_state: TauOpsDashboardSidebarState::Expanded,
            command_center: TauOpsDashboardCommandCenterSnapshot::default(),
            chat: TauOpsDashboardChatSnapshot::default(),
        });
        assert!(html.contains(&format!("data-active-route=\"{expected_active_route}\"")));
        assert!(html.contains(&format!(
            "data-breadcrumb-current=\"{expected_breadcrumb}\""
        )));
    }
}

#[test]
fn functional_spec_2798_c01_c02_c03_shell_exposes_responsive_and_theme_contract_markers() {
    let html = render_tau_ops_dashboard_shell();
    assert!(html.contains("id=\"tau-ops-shell-controls\""));
    assert!(html.contains("id=\"tau-ops-sidebar-toggle\""));
    assert!(html.contains("id=\"tau-ops-sidebar-hamburger\""));
    assert!(html.contains("data-sidebar-mobile-default=\"collapsed\""));
    assert!(html.contains("data-sidebar-state=\"expanded\""));
    assert!(html.contains("data-theme=\"dark\""));
    assert!(html.contains("id=\"tau-ops-theme-toggle-dark\""));
    assert!(html.contains("id=\"tau-ops-theme-toggle-light\""));
}

#[test]
fn functional_spec_2798_c02_shell_sidebar_collapsed_state_updates_toggle_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(html.contains("data-sidebar-state=\"collapsed\""));
    assert!(html.contains("data-sidebar-target-state=\"expanded\""));
    assert!(html.contains("aria-expanded=\"false\""));
    assert!(html.contains("href=\"/ops?theme=dark&amp;sidebar=expanded\""));
}

#[test]
fn functional_spec_2798_c03_shell_light_theme_state_updates_theme_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(html.contains("data-theme=\"light\""));
    assert!(html.contains(
        "id=\"tau-ops-theme-toggle-dark\" data-theme-option=\"dark\" aria-pressed=\"false\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-theme-toggle-light\" data-theme-option=\"light\" aria-pressed=\"true\""
    ));
    assert!(html.contains("href=\"/ops/chat?theme=dark&amp;sidebar=expanded\""));
}

#[test]
fn functional_spec_2830_c01_chat_route_renders_send_form_and_fallback_transcript_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"false\" data-active-session-key=\"default\""));
    assert!(html.contains("id=\"tau-ops-chat-send-form\" action=\"/ops/chat/send\" method=\"post\" data-session-key=\"default\""));
    assert!(html.contains(
        "id=\"tau-ops-chat-session-key\" type=\"hidden\" name=\"session_key\" value=\"default\""
    ));
    assert!(
        html.contains("id=\"tau-ops-chat-theme\" type=\"hidden\" name=\"theme\" value=\"dark\"")
    );
    assert!(html.contains(
        "id=\"tau-ops-chat-sidebar\" type=\"hidden\" name=\"sidebar\" value=\"expanded\""
    ));
    assert!(html.contains("id=\"tau-ops-chat-transcript\" data-message-count=\"1\""));
    assert!(html.contains("id=\"tau-ops-chat-message-row-0\" data-message-role=\"system\""));
    assert!(html.contains("No chat messages yet."));
}

#[test]
fn functional_spec_2830_c02_chat_route_renders_snapshot_message_rows_for_active_session() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-42".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![],
            message_rows: vec![
                TauOpsDashboardChatMessageRow {
                    role: "user".to_string(),
                    content: "first message".to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "assistant".to_string(),
                    content: "second message".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains("data-active-session-key=\"session-42\""));
    assert!(html.contains("id=\"tau-ops-chat-send-form\" action=\"/ops/chat/send\" method=\"post\" data-session-key=\"session-42\""));
    assert!(
        html.contains("id=\"tau-ops-chat-theme\" type=\"hidden\" name=\"theme\" value=\"light\"")
    );
    assert!(html.contains(
        "id=\"tau-ops-chat-sidebar\" type=\"hidden\" name=\"sidebar\" value=\"collapsed\""
    ));
    assert!(html.contains("id=\"tau-ops-chat-transcript\" data-message-count=\"2\""));
    assert!(html.contains("id=\"tau-ops-chat-message-row-0\" data-message-role=\"user\""));
    assert!(html.contains("id=\"tau-ops-chat-message-row-1\" data-message-role=\"assistant\""));
    assert!(html.contains("first message"));
    assert!(html.contains("second message"));
}

#[test]
fn functional_spec_2872_c01_chat_route_renders_new_session_form_contract_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-c01".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-new-session-form\" action=\"/ops/chat/new\" method=\"post\" data-active-session-key=\"chat-c01\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-chat-new-session-key\" type=\"text\" name=\"session_key\" value=\"\""
    ));
    assert!(html
        .contains("id=\"tau-ops-chat-new-theme\" type=\"hidden\" name=\"theme\" value=\"light\""));
    assert!(html.contains(
        "id=\"tau-ops-chat-new-sidebar\" type=\"hidden\" name=\"sidebar\" value=\"collapsed\""
    ));
    assert!(html.contains("id=\"tau-ops-chat-new-session-button\" type=\"submit\""));
}

#[test]
fn functional_spec_2881_c01_chat_route_renders_multiline_compose_contract_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-multiline".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-input\" name=\"message\" placeholder=\"Type a message for the active session\" rows=\"4\" data-multiline-enabled=\"true\" data-newline-shortcut=\"shift-enter\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-chat-input-shortcut-hint\" data-shortcut-contract=\"shift-enter\""
    ));
}

#[test]
fn functional_spec_2862_c01_c02_c03_chat_route_renders_token_counter_marker_contract() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-usage".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![],
            message_rows: vec![],
            session_detail_usage_input_tokens: 13,
            session_detail_usage_output_tokens: 21,
            session_detail_usage_total_tokens: 34,
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"false\" data-active-session-key=\"session-usage\" data-panel-visible=\"true\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-chat-token-counter\" data-session-key=\"session-usage\" data-input-tokens=\"13\" data-output-tokens=\"21\" data-total-tokens=\"34\""
        ));
}

#[test]
fn regression_spec_2862_c04_non_chat_routes_keep_hidden_chat_token_counter_marker_contract() {
    let ops_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-c01".to_string(),
            session_detail_usage_input_tokens: 0,
            session_detail_usage_output_tokens: 0,
            session_detail_usage_total_tokens: 0,
            ..TauOpsDashboardChatSnapshot::default()
        },
    });
    assert!(ops_html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"chat-c01\" data-panel-visible=\"false\""
        ));
    assert!(ops_html.contains(
            "id=\"tau-ops-chat-token-counter\" data-session-key=\"chat-c01\" data-input-tokens=\"0\" data-output-tokens=\"0\" data-total-tokens=\"0\""
        ));

    let sessions_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-c01".to_string(),
            session_detail_usage_input_tokens: 0,
            session_detail_usage_output_tokens: 0,
            session_detail_usage_total_tokens: 0,
            ..TauOpsDashboardChatSnapshot::default()
        },
    });
    assert!(sessions_html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"chat-c01\" data-panel-visible=\"false\""
        ));
    assert!(sessions_html.contains(
            "id=\"tau-ops-chat-token-counter\" data-session-key=\"chat-c01\" data-input-tokens=\"0\" data-output-tokens=\"0\" data-total-tokens=\"0\""
        ));
}

#[test]
fn functional_spec_2866_c01_c02_chat_route_renders_inline_tool_card_for_tool_rows_only() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-tool-session".to_string(),
            message_rows: vec![
                TauOpsDashboardChatMessageRow {
                    role: "user".to_string(),
                    content: "run tool".to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "tool".to_string(),
                    content: "{\"result\":\"ok\"}".to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "assistant".to_string(),
                    content: "tool completed".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains("id=\"tau-ops-chat-message-row-1\" data-message-role=\"tool\""));
    assert!(html.contains(
        "id=\"tau-ops-chat-tool-card-1\" data-tool-card=\"true\" data-inline-result=\"true\""
    ));
    assert!(!html.contains("id=\"tau-ops-chat-tool-card-0\""));
    assert!(!html.contains("id=\"tau-ops-chat-tool-card-2\""));
}

#[test]
fn regression_spec_2866_c04_non_chat_routes_keep_hidden_chat_tool_card_markers() {
    let ops_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-tool-session".to_string(),
            message_rows: vec![TauOpsDashboardChatMessageRow {
                role: "tool".to_string(),
                content: "{\"result\":\"ok\"}".to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });
    assert!(ops_html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"chat-tool-session\" data-panel-visible=\"false\""
        ));
    assert!(ops_html.contains(
        "id=\"tau-ops-chat-tool-card-0\" data-tool-card=\"true\" data-inline-result=\"true\""
    ));

    let sessions_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-tool-session".to_string(),
            message_rows: vec![TauOpsDashboardChatMessageRow {
                role: "tool".to_string(),
                content: "{\"result\":\"ok\"}".to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });
    assert!(sessions_html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"chat-tool-session\" data-panel-visible=\"false\""
        ));
    assert!(sessions_html.contains(
        "id=\"tau-ops-chat-tool-card-0\" data-tool-card=\"true\" data-inline-result=\"true\""
    ));
}

#[test]
fn functional_spec_2870_c01_c02_chat_route_renders_markdown_and_code_markers() {
    let markdown_code_message = "## Build report\n- item one\n[docs](https://example.com)\n|k|v|\n|---|---|\n|a|b|\n```rust\nfn main() {}\n```";
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-markdown-code".to_string(),
            message_rows: vec![
                TauOpsDashboardChatMessageRow {
                    role: "user".to_string(),
                    content: "show report".to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "assistant".to_string(),
                    content: markdown_code_message.to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "assistant".to_string(),
                    content: "plain response".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains("id=\"tau-ops-chat-message-row-1\" data-message-role=\"assistant\""));
    assert!(html.contains("id=\"tau-ops-chat-markdown-1\" data-markdown-rendered=\"true\""));
    assert!(html.contains(
            "id=\"tau-ops-chat-code-block-1\" data-code-block=\"true\" data-language=\"rust\" data-code=\"fn main() {}\""
        ));
    assert!(!html.contains("id=\"tau-ops-chat-markdown-0\""));
    assert!(!html.contains("id=\"tau-ops-chat-code-block-0\""));
    assert!(!html.contains("id=\"tau-ops-chat-code-block-2\""));
}

#[test]
fn regression_spec_2870_c04_non_chat_routes_keep_hidden_markdown_and_code_markers() {
    let markdown_code_message = "## Build report\n- item one\n[docs](https://example.com)\n|k|v|\n|---|---|\n|a|b|\n```rust\nfn main() {}\n```";
    let ops_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-markdown-code".to_string(),
            message_rows: vec![TauOpsDashboardChatMessageRow {
                role: "assistant".to_string(),
                content: markdown_code_message.to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });
    assert!(ops_html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"chat-markdown-code\" data-panel-visible=\"false\""
        ));
    assert!(ops_html.contains("id=\"tau-ops-chat-markdown-0\" data-markdown-rendered=\"true\""));
    assert!(ops_html.contains(
            "id=\"tau-ops-chat-code-block-0\" data-code-block=\"true\" data-language=\"rust\" data-code=\"fn main() {}\""
        ));

    let sessions_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-markdown-code".to_string(),
            message_rows: vec![TauOpsDashboardChatMessageRow {
                role: "assistant".to_string(),
                content: markdown_code_message.to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });
    assert!(sessions_html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"chat-markdown-code\" data-panel-visible=\"false\""
        ));
    assert!(
        sessions_html.contains("id=\"tau-ops-chat-markdown-0\" data-markdown-rendered=\"true\"")
    );
    assert!(sessions_html.contains(
            "id=\"tau-ops-chat-code-block-0\" data-code-block=\"true\" data-language=\"rust\" data-code=\"fn main() {}\""
        ));
}

#[test]
fn functional_spec_2834_c01_chat_route_renders_session_selector_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-session-selector\" data-active-session-key=\"default\" data-option-count=\"1\""
        ));
    assert!(html.contains("id=\"tau-ops-chat-session-options\""));
    assert!(html.contains(
        "id=\"tau-ops-chat-session-option-0\" data-session-key=\"default\" data-selected=\"true\""
    ));
    assert!(html.contains("data-session-link=\"default\""));
    assert!(html.contains("href=\"/ops/chat?theme=dark&amp;sidebar=expanded&amp;session=default\""));
}

#[test]
fn functional_spec_2834_c02_chat_route_keeps_active_session_selected_in_selector_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-beta".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![],
            message_rows: vec![TauOpsDashboardChatMessageRow {
                role: "user".to_string(),
                content: "chat from beta".to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html
        .contains("id=\"tau-ops-chat-session-selector\" data-active-session-key=\"session-beta\""));
    assert!(html.contains(
            "id=\"tau-ops-chat-session-option-0\" data-session-key=\"session-beta\" data-selected=\"true\""
        ));
    assert!(html
        .contains("href=\"/ops/chat?theme=light&amp;sidebar=collapsed&amp;session=session-beta\""));
    assert!(html.contains(
            "id=\"tau-ops-chat-session-key\" type=\"hidden\" name=\"session_key\" value=\"session-beta\""
        ));
    assert!(html.contains("chat from beta"));
}

#[test]
fn functional_spec_2834_c03_chat_route_adds_missing_active_session_option_marker() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-zeta".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![TauOpsDashboardChatSessionOptionRow {
                session_key: "session-alpha".to_string(),
                selected: false,
                entry_count: 0,
                usage_total_tokens: 0,
                validation_is_valid: true,
                updated_unix_ms: 0,
            }],
            message_rows: vec![TauOpsDashboardChatMessageRow {
                role: "user".to_string(),
                content: "zeta transcript".to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-session-selector\" data-active-session-key=\"session-zeta\" data-option-count=\"2\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-chat-session-option-0\" data-session-key=\"session-alpha\" data-selected=\"false\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-chat-session-option-1\" data-session-key=\"session-zeta\" data-selected=\"true\""
        ));
}

#[test]
fn functional_spec_2901_c01_c03_chat_route_renders_assistant_token_stream_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "chat-stream".to_string(),
            message_rows: vec![
                TauOpsDashboardChatMessageRow {
                    role: "user".to_string(),
                    content: "operator request".to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "assistant".to_string(),
                    content: "stream one two".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-message-row-1\" data-message-role=\"assistant\" data-assistant-token-stream=\"true\" data-token-count=\"3\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-chat-token-stream-1\" data-token-stream=\"assistant\" data-token-count=\"3\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-chat-token-1-0\" data-token-index=\"0\" data-token-value=\"stream\""
    ));
    assert!(html
        .contains("id=\"tau-ops-chat-token-1-1\" data-token-index=\"1\" data-token-value=\"one\""));
    assert!(html
        .contains("id=\"tau-ops-chat-token-1-2\" data-token-index=\"2\" data-token-value=\"two\""));
    assert!(!html.contains("id=\"tau-ops-chat-token-stream-0\""));
}

#[test]
fn functional_spec_2905_c01_c03_memory_route_renders_search_panel_and_empty_state_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
            "id=\"tau-ops-memory-panel\" data-route=\"/ops/memory\" aria-hidden=\"false\" data-panel-visible=\"true\" data-query=\"\" data-result-count=\"0\""
        ));
    assert!(
        html.contains("id=\"tau-ops-memory-search-form\" action=\"/ops/memory\" method=\"get\"")
    );
    assert!(html.contains("id=\"tau-ops-memory-query\" type=\"search\" name=\"query\" value=\"\""));
    assert!(html.contains("id=\"tau-ops-memory-results\" data-result-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-memory-empty-state\" data-empty-state=\"true\""));
}

#[test]
fn functional_spec_2909_c01_c03_memory_route_renders_scope_filter_controls() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-workspace-filter\" type=\"text\" name=\"workspace_id\" value=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-channel-filter\" type=\"text\" name=\"channel_id\" value=\"\""
    ));
    assert!(html
        .contains("id=\"tau-ops-memory-actor-filter\" type=\"text\" name=\"actor_id\" value=\"\""));
}

#[test]
fn functional_spec_2913_c01_c03_memory_route_renders_type_filter_control() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-type-filter\" type=\"text\" name=\"memory_type\" value=\"\""
    ));
}

#[test]
fn functional_spec_2917_c01_c03_memory_route_renders_create_form_and_status_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
            "id=\"tau-ops-memory-panel\" data-route=\"/ops/memory\" aria-hidden=\"false\" data-panel-visible=\"true\" data-query=\"\" data-result-count=\"0\" data-workspace-id=\"\" data-channel-id=\"\" data-actor-id=\"\" data-memory-type=\"\" data-create-status=\"idle\" data-created-memory-id=\"\""
        ));
    assert!(
            html.contains("id=\"tau-ops-memory-create-status\" data-create-status=\"idle\" data-created-memory-id=\"\"")
        );
    assert!(
        html.contains("id=\"tau-ops-memory-create-form\" action=\"/ops/memory\" method=\"post\"")
    );
    assert!(html.contains(
        "id=\"tau-ops-memory-create-entry-id\" type=\"text\" name=\"entry_id\" value=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-create-summary\" type=\"text\" name=\"summary\" value=\"\""
    ));
    assert!(
        html.contains("id=\"tau-ops-memory-create-tags\" type=\"text\" name=\"tags\" value=\"\"")
    );
    assert!(
        html.contains("id=\"tau-ops-memory-create-facts\" type=\"text\" name=\"facts\" value=\"\"")
    );
    assert!(html.contains(
            "id=\"tau-ops-memory-create-source-event-key\" type=\"text\" name=\"source_event_key\" value=\"\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-memory-create-workspace-id\" type=\"text\" name=\"workspace_id\" value=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-create-channel-id\" type=\"text\" name=\"channel_id\" value=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-create-actor-id\" type=\"text\" name=\"actor_id\" value=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-create-memory-type\" type=\"text\" name=\"memory_type\" value=\"\""
    ));
    assert!(html.contains(
            "id=\"tau-ops-memory-create-importance\" type=\"number\" step=\"0.01\" name=\"importance\" value=\"\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-memory-create-relation-target-id\" type=\"text\" name=\"relation_target_id\" value=\"\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-memory-create-relation-type\" type=\"text\" name=\"relation_type\" value=\"\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-memory-create-relation-weight\" type=\"number\" step=\"0.01\" name=\"relation_weight\" value=\"\""
        ));
}

#[test]
fn functional_spec_2921_c01_c03_memory_route_renders_edit_form_and_status_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-edit-status\" data-edit-status=\"idle\" data-edited-memory-id=\"\""
    ));
    assert!(html.contains("id=\"tau-ops-memory-edit-form\" action=\"/ops/memory\" method=\"post\""));
    assert!(html.contains(
        "id=\"tau-ops-memory-edit-operation\" type=\"hidden\" name=\"operation\" value=\"edit\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-edit-entry-id\" type=\"text\" name=\"entry_id\" value=\"\""
    ));
    assert!(html
        .contains("id=\"tau-ops-memory-edit-summary\" type=\"text\" name=\"summary\" value=\"\""));
    assert!(html.contains(
        "id=\"tau-ops-memory-edit-memory-type\" type=\"text\" name=\"memory_type\" value=\"\""
    ));
    assert!(html.contains(
            "id=\"tau-ops-memory-edit-importance\" type=\"number\" step=\"0.01\" name=\"importance\" value=\"\""
        ));
}

#[test]
fn regression_spec_2921_memory_edit_status_updated_renders_updated_message_marker() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            memory_create_status: "updated".to_string(),
            memory_create_created_entry_id: "mem-edit-1".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    let edit_status_marker = "id=\"tau-ops-memory-edit-status\" data-edit-status=\"updated\" data-edited-memory-id=\"mem-edit-1\"";
    assert!(html.contains(edit_status_marker));
    let edit_section = &html[html
        .find(edit_status_marker)
        .expect("edit status marker should be rendered when status is updated")..];
    assert!(edit_section.contains(">Memory entry updated.</p>"));
}

#[test]
fn regression_spec_2917_memory_create_status_created_renders_created_message_marker() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            memory_create_status: "created".to_string(),
            memory_create_created_entry_id: "mem-create-1".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-memory-create-status\" data-create-status=\"created\" data-created-memory-id=\"mem-create-1\""
        ));
    assert!(html.contains("Memory entry created."));
}

#[test]
fn regression_spec_2917_memory_create_status_updated_renders_updated_message_marker() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            memory_create_status: "updated".to_string(),
            memory_create_created_entry_id: "mem-create-1".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-memory-create-status\" data-create-status=\"updated\" data-created-memory-id=\"mem-create-1\""
        ));
    assert!(html.contains("Memory entry updated."));
}

#[test]
fn functional_spec_3060_c01_memory_route_renders_delete_form_and_confirmation_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-panel\" data-route=\"/ops/memory\" aria-hidden=\"false\" data-panel-visible=\"true\" data-query=\"\" data-result-count=\"0\" data-workspace-id=\"\" data-channel-id=\"\" data-actor-id=\"\" data-memory-type=\"\" data-create-status=\"idle\" data-created-memory-id=\"\" data-edit-status=\"idle\" data-edited-memory-id=\"\" data-delete-status=\"idle\" data-deleted-memory-id=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-delete-status\" data-delete-status=\"idle\" data-deleted-memory-id=\"\""
    ));
    assert!(
        html.contains("id=\"tau-ops-memory-delete-form\" action=\"/ops/memory\" method=\"post\"")
    );
    assert!(html.contains(
        "id=\"tau-ops-memory-delete-operation\" type=\"hidden\" name=\"operation\" value=\"delete\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-delete-entry-id\" type=\"text\" name=\"entry_id\" value=\"\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-delete-confirm\" type=\"checkbox\" name=\"confirm_delete\" value=\"true\""
    ));
}

#[test]
fn regression_spec_3060_c04_non_memory_routes_keep_hidden_memory_delete_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-panel\" data-route=\"/ops/memory\" aria-hidden=\"true\" data-panel-visible=\"false\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-delete-status\" data-delete-status=\"idle\" data-deleted-memory-id=\"\""
    ));
    assert!(
        html.contains("id=\"tau-ops-memory-delete-form\" action=\"/ops/memory\" method=\"post\"")
    );
}

#[test]
fn functional_spec_3064_c01_memory_route_renders_detail_panel_default_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Memory,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-detail-panel\" data-detail-visible=\"false\" data-memory-id=\"\" data-memory-type=\"\" data-embedding-source=\"\" data-embedding-model=\"\" data-embedding-reason-code=\"\" data-embedding-dimensions=\"0\" data-relation-count=\"0\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-detail-embedding\" data-embedding-source=\"\" data-embedding-model=\"\" data-embedding-reason-code=\"\" data-embedding-dimensions=\"0\""
    ));
    assert!(html.contains("id=\"tau-ops-memory-relations\" data-relation-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-memory-relations-empty-state\" data-empty-state=\"true\""));
}

#[test]
fn regression_spec_3064_c04_non_memory_routes_keep_hidden_detail_panel_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-panel\" data-route=\"/ops/memory\" aria-hidden=\"true\" data-panel-visible=\"false\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-detail-panel\" data-detail-visible=\"false\" data-memory-id=\"\""
    ));
}

#[test]
fn functional_spec_3068_c01_memory_graph_route_renders_graph_panel_default_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::MemoryGraph,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-graph-panel\" data-route=\"/ops/memory-graph\" aria-hidden=\"false\" data-panel-visible=\"true\" data-node-count=\"0\" data-edge-count=\"0\""
    ));
    assert!(html.contains("id=\"tau-ops-memory-graph-nodes\" data-node-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-memory-graph-edges\" data-edge-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-memory-graph-empty-state\" data-empty-state=\"true\""));
}

#[test]
fn regression_spec_3068_c03_non_memory_graph_routes_keep_hidden_graph_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-graph-panel\" data-route=\"/ops/memory-graph\" aria-hidden=\"true\" data-panel-visible=\"false\""
    ));
    assert!(html.contains("id=\"tau-ops-memory-graph-nodes\" data-node-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-memory-graph-edges\" data-edge-count=\"0\""));
}

#[test]
fn functional_spec_3070_c01_c02_memory_graph_route_renders_node_size_markers_from_importance() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::MemoryGraph,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            memory_graph_node_rows: vec![
                TauOpsDashboardMemoryGraphNodeRow {
                    memory_id: "mem-size-low".to_string(),
                    memory_type: "fact".to_string(),
                    importance: "0.1000".to_string(),
                },
                TauOpsDashboardMemoryGraphNodeRow {
                    memory_id: "mem-size-high".to_string(),
                    memory_type: "goal".to_string(),
                    importance: "0.9000".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-graph-node-0\" data-memory-id=\"mem-size-low\" data-memory-type=\"fact\" data-importance=\"0.1000\" data-node-size-bucket=\"small\" data-node-size-px=\"13.60\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-graph-node-1\" data-memory-id=\"mem-size-high\" data-memory-type=\"goal\" data-importance=\"0.9000\" data-node-size-bucket=\"large\" data-node-size-px=\"26.40\""
    ));
}

#[test]
fn functional_spec_3078_c02_memory_graph_route_renders_node_color_markers_from_memory_type() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::MemoryGraph,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            memory_graph_node_rows: vec![
                TauOpsDashboardMemoryGraphNodeRow {
                    memory_id: "mem-color-fact".to_string(),
                    memory_type: "fact".to_string(),
                    importance: "0.5000".to_string(),
                },
                TauOpsDashboardMemoryGraphNodeRow {
                    memory_id: "mem-color-event".to_string(),
                    memory_type: "event".to_string(),
                    importance: "0.5000".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-graph-node-0\" data-memory-id=\"mem-color-fact\" data-memory-type=\"fact\" data-importance=\"0.5000\" data-node-size-bucket=\"medium\" data-node-size-px=\"20.00\" data-node-color-token=\"fact\" data-node-color-hex=\"#2563eb\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-graph-node-1\" data-memory-id=\"mem-color-event\" data-memory-type=\"event\" data-importance=\"0.5000\" data-node-size-bucket=\"medium\" data-node-size-px=\"20.00\" data-node-color-token=\"event\" data-node-color-hex=\"#7c3aed\""
    ));
}

#[test]
fn functional_spec_3082_c02_memory_graph_route_renders_edge_style_markers_from_relation_type() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::MemoryGraph,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            memory_graph_edge_rows: vec![
                TauOpsDashboardMemoryGraphEdgeRow {
                    source_memory_id: "mem-edge-a".to_string(),
                    target_memory_id: "mem-edge-b".to_string(),
                    relation_type: "related_to".to_string(),
                    effective_weight: "0.4200".to_string(),
                },
                TauOpsDashboardMemoryGraphEdgeRow {
                    source_memory_id: "mem-edge-b".to_string(),
                    target_memory_id: "mem-edge-c".to_string(),
                    relation_type: "updates".to_string(),
                    effective_weight: "0.5500".to_string(),
                },
                TauOpsDashboardMemoryGraphEdgeRow {
                    source_memory_id: "mem-edge-c".to_string(),
                    target_memory_id: "mem-edge-d".to_string(),
                    relation_type: "contradicts".to_string(),
                    effective_weight: "0.6600".to_string(),
                },
                TauOpsDashboardMemoryGraphEdgeRow {
                    source_memory_id: "mem-edge-d".to_string(),
                    target_memory_id: "mem-edge-e".to_string(),
                    relation_type: "depends_on".to_string(),
                    effective_weight: "0.7700".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
        "id=\"tau-ops-memory-graph-edge-0\" data-source-memory-id=\"mem-edge-a\" data-target-memory-id=\"mem-edge-b\" data-relation-type=\"related_to\" data-relation-weight=\"0.4200\" data-edge-style-token=\"solid\" data-edge-stroke-dasharray=\"none\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-graph-edge-1\" data-source-memory-id=\"mem-edge-b\" data-target-memory-id=\"mem-edge-c\" data-relation-type=\"updates\" data-relation-weight=\"0.5500\" data-edge-style-token=\"dashed\" data-edge-stroke-dasharray=\"6 4\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-graph-edge-2\" data-source-memory-id=\"mem-edge-c\" data-target-memory-id=\"mem-edge-d\" data-relation-type=\"contradicts\" data-relation-weight=\"0.6600\" data-edge-style-token=\"dotted\" data-edge-stroke-dasharray=\"2 4\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-memory-graph-edge-3\" data-source-memory-id=\"mem-edge-d\" data-target-memory-id=\"mem-edge-e\" data-relation-type=\"depends_on\" data-relation-weight=\"0.7700\" data-edge-style-token=\"dashed\" data-edge-stroke-dasharray=\"6 4\""
    ));
}

#[test]
fn functional_spec_2838_c01_c02_c03_sessions_route_renders_sessions_panel_list_rows_and_links() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-beta".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![
                TauOpsDashboardChatSessionOptionRow {
                    session_key: "session-alpha".to_string(),
                    selected: false,
                    entry_count: 0,
                    usage_total_tokens: 0,
                    validation_is_valid: true,
                    updated_unix_ms: 0,
                },
                TauOpsDashboardChatSessionOptionRow {
                    session_key: "session-beta".to_string(),
                    selected: true,
                    entry_count: 0,
                    usage_total_tokens: 0,
                    validation_is_valid: true,
                    updated_unix_ms: 0,
                },
            ],
            message_rows: vec![],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
        "id=\"tau-ops-sessions-panel\" data-route=\"/ops/sessions\" aria-hidden=\"false\""
    ));
    assert!(html.contains("id=\"tau-ops-sessions-list\" data-session-count=\"2\""));
    assert!(html.contains(
        "id=\"tau-ops-sessions-row-0\" data-session-key=\"session-alpha\" data-selected=\"false\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-sessions-row-1\" data-session-key=\"session-beta\" data-selected=\"true\""
    ));
    assert!(html.contains(
        "href=\"/ops/chat?theme=light&amp;sidebar=collapsed&amp;session=session-alpha\""
    ));
    assert!(html
        .contains("href=\"/ops/chat?theme=light&amp;sidebar=collapsed&amp;session=session-beta\""));
}

#[test]
fn functional_spec_2838_c04_sessions_route_renders_empty_state_marker_when_no_sessions_discovered()
{
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "default".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![],
            message_rows: vec![],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
        "id=\"tau-ops-sessions-panel\" data-route=\"/ops/sessions\" aria-hidden=\"false\""
    ));
    assert!(html.contains("id=\"tau-ops-sessions-list\" data-session-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-sessions-empty-state\" data-empty-state=\"true\""));
    assert!(html.contains("No sessions discovered yet."));
}

#[test]
fn functional_spec_2893_c01_sessions_route_renders_row_metadata_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-beta".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![
                TauOpsDashboardChatSessionOptionRow {
                    session_key: "session-alpha".to_string(),
                    selected: false,
                    entry_count: 0,
                    usage_total_tokens: 0,
                    validation_is_valid: true,
                    updated_unix_ms: 0,
                },
                TauOpsDashboardChatSessionOptionRow {
                    session_key: "session-beta".to_string(),
                    selected: true,
                    entry_count: 0,
                    usage_total_tokens: 0,
                    validation_is_valid: true,
                    updated_unix_ms: 0,
                },
            ],
            message_rows: vec![],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-sessions-row-0\" data-session-key=\"session-alpha\" data-selected=\"false\" data-entry-count=\"0\" data-total-tokens=\"0\" data-is-valid=\"true\" data-updated-unix-ms=\"0\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-sessions-row-1\" data-session-key=\"session-beta\" data-selected=\"true\" data-entry-count=\"0\" data-total-tokens=\"0\" data-is-valid=\"true\" data-updated-unix-ms=\"0\""
        ));
}

#[test]
fn functional_spec_2842_c01_c03_c05_sessions_route_renders_detail_panel_and_empty_timeline_contracts(
) {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-empty".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![TauOpsDashboardChatSessionOptionRow {
                session_key: "session-empty".to_string(),
                selected: true,
                entry_count: 0,
                usage_total_tokens: 0,
                validation_is_valid: true,
                updated_unix_ms: 0,
            }],
            message_rows: vec![],
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-empty".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-session-detail-panel\" data-route=\"/ops/sessions/session-empty\" data-session-key=\"session-empty\" aria-hidden=\"false\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-validation-report\" data-entries=\"0\" data-duplicates=\"0\" data-invalid-parent=\"0\" data-cycles=\"0\" data-is-valid=\"true\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-usage-summary\" data-input-tokens=\"0\" data-output-tokens=\"0\" data-total-tokens=\"0\" data-estimated-cost-usd=\"0.000000\""
        ));
    assert!(html.contains("id=\"tau-ops-session-message-timeline\" data-entry-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-session-message-empty-state\" data-empty-state=\"true\""));
}

#[test]
fn functional_spec_2842_c02_c04_sessions_route_renders_detail_timeline_rows_and_usage_contracts() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-alpha".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![TauOpsDashboardChatSessionOptionRow {
                session_key: "session-alpha".to_string(),
                selected: true,
                entry_count: 0,
                usage_total_tokens: 0,
                validation_is_valid: true,
                updated_unix_ms: 0,
            }],
            message_rows: vec![
                TauOpsDashboardChatMessageRow {
                    role: "user".to_string(),
                    content: "first detail message".to_string(),
                },
                TauOpsDashboardChatMessageRow {
                    role: "assistant".to_string(),
                    content: "second detail message".to_string(),
                },
            ],
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-alpha".to_string(),
            session_detail_timeline_rows: vec![
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 0,
                    role: "user".to_string(),
                    content: "first detail message".to_string(),
                },
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 1,
                    role: "assistant".to_string(),
                    content: "second detail message".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains("id=\"tau-ops-session-message-timeline\" data-entry-count=\"2\""));
    assert!(html.contains(
        "id=\"tau-ops-session-message-row-0\" data-entry-id=\"0\" data-message-role=\"user\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-session-message-row-1\" data-entry-id=\"1\" data-message-role=\"assistant\""
    ));
    assert!(html.contains("first detail message"));
    assert!(html.contains("second detail message"));
    assert!(html.contains(
            "id=\"tau-ops-session-usage-summary\" data-input-tokens=\"0\" data-output-tokens=\"0\" data-total-tokens=\"0\" data-estimated-cost-usd=\"0.000000\""
        ));
}

#[test]
fn functional_spec_2897_c01_c02_session_detail_timeline_exposes_complete_content_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-coverage".to_string(),
            send_form_action: "/ops/chat/send".to_string(),
            send_form_method: "post".to_string(),
            session_options: vec![TauOpsDashboardChatSessionOptionRow {
                session_key: "session-coverage".to_string(),
                selected: true,
                entry_count: 0,
                usage_total_tokens: 0,
                validation_is_valid: true,
                updated_unix_ms: 0,
            }],
            message_rows: vec![],
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-coverage".to_string(),
            session_detail_timeline_rows: vec![
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 0,
                    role: "system".to_string(),
                    content: "system coverage message".to_string(),
                },
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 1,
                    role: "user".to_string(),
                    content: "user coverage message".to_string(),
                },
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 2,
                    role: "assistant".to_string(),
                    content: "assistant coverage message".to_string(),
                },
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 3,
                    role: "tool".to_string(),
                    content: "tool coverage output".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains("id=\"tau-ops-session-message-timeline\" data-entry-count=\"4\""));
    assert!(html.contains(
            "id=\"tau-ops-session-message-row-0\" data-entry-id=\"0\" data-message-role=\"system\" data-message-content=\"system coverage message\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-message-row-1\" data-entry-id=\"1\" data-message-role=\"user\" data-message-content=\"user coverage message\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-message-row-2\" data-entry-id=\"2\" data-message-role=\"assistant\" data-message-content=\"assistant coverage message\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-message-row-3\" data-entry-id=\"3\" data-message-role=\"tool\" data-message-content=\"tool coverage output\""
        ));
}

#[test]
fn functional_spec_2846_c01_c04_c05_sessions_route_renders_graph_panel_summary_and_empty_state() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-empty".to_string(),
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-empty".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-session-graph-panel\" data-route=\"/ops/sessions/session-empty\" data-session-key=\"session-empty\" aria-hidden=\"false\""
        ));
    assert!(html.contains("id=\"tau-ops-session-graph-nodes\" data-node-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-session-graph-edges\" data-edge-count=\"0\""));
    assert!(html.contains("id=\"tau-ops-session-graph-empty-state\" data-empty-state=\"true\""));
}

#[test]
fn functional_spec_2846_c02_c03_sessions_route_renders_graph_node_and_edge_rows() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-graph".to_string(),
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-graph".to_string(),
            session_graph_node_rows: vec![
                TauOpsDashboardSessionGraphNodeRow {
                    entry_id: 1,
                    role: "system".to_string(),
                },
                TauOpsDashboardSessionGraphNodeRow {
                    entry_id: 2,
                    role: "user".to_string(),
                },
                TauOpsDashboardSessionGraphNodeRow {
                    entry_id: 3,
                    role: "assistant".to_string(),
                },
            ],
            session_graph_edge_rows: vec![
                TauOpsDashboardSessionGraphEdgeRow {
                    source_entry_id: 1,
                    target_entry_id: 2,
                },
                TauOpsDashboardSessionGraphEdgeRow {
                    source_entry_id: 2,
                    target_entry_id: 3,
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains("id=\"tau-ops-session-graph-nodes\" data-node-count=\"3\""));
    assert!(html.contains("id=\"tau-ops-session-graph-edges\" data-edge-count=\"2\""));
    assert!(html.contains(
        "id=\"tau-ops-session-graph-node-0\" data-entry-id=\"1\" data-message-role=\"system\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-session-graph-node-1\" data-entry-id=\"2\" data-message-role=\"user\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-session-graph-node-2\" data-entry-id=\"3\" data-message-role=\"assistant\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-session-graph-edge-0\" data-source-entry-id=\"1\" data-target-entry-id=\"2\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-session-graph-edge-1\" data-source-entry-id=\"2\" data-target-entry-id=\"3\""
    ));
}

#[test]
fn functional_spec_2885_c01_sessions_route_renders_timeline_row_branch_form_contracts() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-branch-source".to_string(),
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-branch-source".to_string(),
            session_detail_timeline_rows: vec![
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 7,
                    role: "user".to_string(),
                    content: "branch anchor message".to_string(),
                },
                TauOpsDashboardSessionTimelineRow {
                    entry_id: 8,
                    role: "assistant".to_string(),
                    content: "downstream reply".to_string(),
                },
            ],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-session-branch-form-0\" action=\"/ops/sessions/branch\" method=\"post\" data-source-session-key=\"session-branch-source\" data-entry-id=\"7\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-branch-source-session-key-0\" type=\"hidden\" name=\"source_session_key\" value=\"session-branch-source\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-session-branch-entry-id-0\" type=\"hidden\" name=\"entry_id\" value=\"7\""
    ));
    assert!(
            html.contains("id=\"tau-ops-session-branch-target-session-key-0\" type=\"text\" name=\"target_session_key\" value=\"\"")
        );
    assert!(html.contains(
        "id=\"tau-ops-session-branch-theme-0\" type=\"hidden\" name=\"theme\" value=\"dark\""
    ));
    assert!(html.contains(
            "id=\"tau-ops-session-branch-sidebar-0\" type=\"hidden\" name=\"sidebar\" value=\"expanded\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-branch-submit-0\" type=\"submit\" data-confirmation-required=\"true\""
        ));
}

#[test]
fn functional_spec_2889_c01_sessions_route_renders_reset_confirmation_form_contracts() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-reset-target".to_string(),
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-reset-target".to_string(),
            session_detail_timeline_rows: vec![TauOpsDashboardSessionTimelineRow {
                entry_id: 3,
                role: "assistant".to_string(),
                content: "reset candidate row".to_string(),
            }],
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-session-reset-form\" action=\"/ops/sessions/session-reset-target\" method=\"post\" data-session-key=\"session-reset-target\" data-confirmation-required=\"true\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-session-reset-session-key\" type=\"hidden\" name=\"session_key\" value=\"session-reset-target\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-session-reset-theme\" type=\"hidden\" name=\"theme\" value=\"light\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-session-reset-sidebar\" type=\"hidden\" name=\"sidebar\" value=\"collapsed\""
    ));
    assert!(html.contains(
            "id=\"tau-ops-session-reset-confirm\" type=\"hidden\" name=\"confirm_reset\" value=\"true\""
        ));
    assert!(html.contains(
        "id=\"tau-ops-session-reset-submit\" type=\"submit\" data-confirmation-required=\"true\""
    ));
}

#[test]
fn regression_spec_2842_session_detail_panel_stays_hidden_on_non_sessions_route() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot {
            active_session_key: "session-alpha".to_string(),
            session_detail_visible: true,
            session_detail_route: "/ops/sessions/session-alpha".to_string(),
            ..TauOpsDashboardChatSnapshot::default()
        },
    });

    assert!(html.contains(
            "id=\"tau-ops-session-detail-panel\" data-route=\"/ops/sessions/session-alpha\" data-session-key=\"session-alpha\" aria-hidden=\"true\""
        ));
}

#[test]
fn functional_spec_2806_c01_c02_c03_command_center_snapshot_markers_render() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "healthy".to_string(),
            health_reason: "no recent transport failures observed".to_string(),
            rollout_gate: "hold".to_string(),
            control_mode: "paused".to_string(),
            control_paused: true,
            action_pause_enabled: false,
            action_resume_enabled: true,
            action_refresh_enabled: true,
            last_action_request_id: "dashboard-action-90210".to_string(),
            last_action_name: "pause".to_string(),
            last_action_actor: "ops-user".to_string(),
            last_action_timestamp_unix_ms: 90210,
            timeline_range: "1h".to_string(),
            timeline_point_count: 9,
            timeline_last_timestamp_unix_ms: 811,
            queue_depth: 3,
            failure_streak: 1,
            processed_case_count: 8,
            alert_count: 2,
            widget_count: 6,
            timeline_cycle_count: 9,
            timeline_invalid_cycle_count: 1,
            primary_alert_code: "dashboard_queue_backlog".to_string(),
            primary_alert_severity: "warning".to_string(),
            primary_alert_message: "runtime backlog detected (queue_depth=3)".to_string(),
            alert_feed_rows: vec![],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("data-health-state=\"healthy\""));
    assert!(html.contains("data-health-reason=\"no recent transport failures observed\""));
    assert_eq!(html.matches("data-kpi-card=").count(), 6);
    assert!(html.contains("data-kpi-card=\"queue-depth\" data-kpi-value=\"3\""));
    assert!(html.contains("data-kpi-card=\"failure-streak\" data-kpi-value=\"1\""));
    assert!(html.contains("data-kpi-card=\"processed-cases\" data-kpi-value=\"8\""));
    assert!(html.contains("data-kpi-card=\"alert-count\" data-kpi-value=\"2\""));
    assert!(html.contains("data-kpi-card=\"widget-count\" data-kpi-value=\"6\""));
    assert!(html.contains("data-kpi-card=\"timeline-cycles\" data-kpi-value=\"9\""));
    assert!(html.contains("data-alert-count=\"2\""));
    assert!(html.contains("data-primary-alert-code=\"dashboard_queue_backlog\""));
    assert!(html.contains("data-primary-alert-severity=\"warning\""));
    assert!(html.contains("runtime backlog detected (queue_depth=3)"));
    assert!(html.contains("data-timeline-cycle-count=\"9\""));
    assert!(html.contains("data-timeline-invalid-cycle-count=\"1\""));
    assert!(html.contains("data-control-mode=\"paused\""));
    assert!(html.contains("data-rollout-gate=\"hold\""));
    assert!(html.contains("data-control-paused=\"true\""));
    assert!(html.contains("id=\"tau-ops-control-action-pause\" data-action-enabled=\"false\""));
    assert!(html.contains("id=\"tau-ops-control-action-resume\" data-action-enabled=\"true\""));
    assert!(html.contains("id=\"tau-ops-control-action-refresh\" data-action-enabled=\"true\""));
    assert!(html.contains("data-last-action-request-id=\"dashboard-action-90210\""));
    assert!(html.contains("data-last-action-name=\"pause\""));
    assert!(html.contains("data-last-action-actor=\"ops-user\""));
    assert!(html.contains("data-last-action-timestamp=\"90210\""));
    assert!(html.contains("id=\"tau-ops-queue-timeline-chart\""));
    assert!(html.contains("data-component=\"TimelineChart\""));
    assert!(html.contains("data-timeline-range=\"1h\""));
    assert!(html.contains("data-timeline-point-count=\"9\""));
    assert!(html.contains("data-timeline-last-timestamp=\"811\""));
}

#[test]
fn functional_spec_2854_c01_command_center_panel_visible_on_ops_route() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(
        html.contains("id=\"tau-ops-command-center\" data-route=\"/ops\" aria-hidden=\"false\"")
    );
}

#[test]
fn functional_spec_2854_c02_c03_command_center_panel_hidden_on_non_ops_routes() {
    let chat_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(chat_html
        .contains("id=\"tau-ops-command-center\" data-route=\"/ops\" aria-hidden=\"true\""));

    let sessions_html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });
    assert!(sessions_html
        .contains("id=\"tau-ops-command-center\" data-route=\"/ops\" aria-hidden=\"true\""));
}

#[test]
fn functional_spec_2858_c01_c03_chat_route_panel_visibility_state_contracts() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Chat,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"false\" data-active-session-key=\"default\" data-panel-visible=\"true\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-sessions-panel\" data-route=\"/ops/sessions\" aria-hidden=\"true\" data-panel-visible=\"false\""
        ));
}

#[test]
fn functional_spec_2858_c02_c04_sessions_route_panel_visibility_state_contracts() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Sessions,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"default\" data-panel-visible=\"false\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-sessions-panel\" data-route=\"/ops/sessions\" aria-hidden=\"false\" data-panel-visible=\"true\""
        ));
}

#[test]
fn regression_spec_2858_c05_ops_route_panels_remain_hidden_with_visibility_state_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot::default(),
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains(
            "id=\"tau-ops-chat-panel\" data-route=\"/ops/chat\" aria-hidden=\"true\" data-active-session-key=\"default\" data-panel-visible=\"false\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-sessions-panel\" data-route=\"/ops/sessions\" aria-hidden=\"true\" data-panel-visible=\"false\""
        ));
}

#[test]
fn functional_spec_2810_c01_c02_c03_command_center_control_markers_render() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "healthy".to_string(),
            health_reason: "operator pause action is active".to_string(),
            rollout_gate: "hold".to_string(),
            control_mode: "paused".to_string(),
            control_paused: true,
            action_pause_enabled: false,
            action_resume_enabled: true,
            action_refresh_enabled: true,
            last_action_request_id: "dashboard-action-90210".to_string(),
            last_action_name: "pause".to_string(),
            last_action_actor: "ops-user".to_string(),
            last_action_timestamp_unix_ms: 90210,
            timeline_range: "1h".to_string(),
            timeline_point_count: 2,
            timeline_last_timestamp_unix_ms: 811,
            queue_depth: 1,
            failure_streak: 0,
            processed_case_count: 2,
            alert_count: 2,
            widget_count: 2,
            timeline_cycle_count: 2,
            timeline_invalid_cycle_count: 1,
            primary_alert_code: "dashboard_queue_backlog".to_string(),
            primary_alert_severity: "warning".to_string(),
            primary_alert_message: "runtime backlog detected (queue_depth=1)".to_string(),
            alert_feed_rows: vec![],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-control-panel\""));
    assert!(html.contains("data-control-mode=\"paused\""));
    assert!(html.contains("data-rollout-gate=\"hold\""));
    assert!(html.contains("data-control-paused=\"true\""));
    assert!(html.contains("id=\"tau-ops-control-action-pause\" data-action-enabled=\"false\""));
    assert!(html.contains("id=\"tau-ops-control-action-resume\" data-action-enabled=\"true\""));
    assert!(html.contains("id=\"tau-ops-control-action-refresh\" data-action-enabled=\"true\""));
    assert!(html.contains("data-last-action-request-id=\"dashboard-action-90210\""));
    assert!(html.contains("data-last-action-name=\"pause\""));
    assert!(html.contains("data-last-action-actor=\"ops-user\""));
    assert!(html.contains("data-last-action-timestamp=\"90210\""));
}

#[test]
fn functional_spec_2826_c01_c02_control_actions_expose_confirmation_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "healthy".to_string(),
            health_reason: "operator controls are ready".to_string(),
            rollout_gate: "pass".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "1h".to_string(),
            timeline_point_count: 1,
            timeline_last_timestamp_unix_ms: 811,
            queue_depth: 0,
            failure_streak: 0,
            processed_case_count: 1,
            alert_count: 1,
            widget_count: 1,
            timeline_cycle_count: 1,
            timeline_invalid_cycle_count: 0,
            primary_alert_code: "dashboard_healthy".to_string(),
            primary_alert_severity: "info".to_string(),
            primary_alert_message: "dashboard runtime health is nominal".to_string(),
            alert_feed_rows: vec![],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-control-action-pause\""));
    assert!(html.contains(
            "id=\"tau-ops-control-action-pause\" data-action-enabled=\"true\" data-action=\"pause\" data-confirm-required=\"true\" data-confirm-title=\"Confirm pause action\" data-confirm-body=\"Pause command-center processing until resumed.\" data-confirm-verb=\"pause\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-control-action-resume\" data-action-enabled=\"false\" data-action=\"resume\" data-confirm-required=\"true\" data-confirm-title=\"Confirm resume action\" data-confirm-body=\"Resume command-center processing.\" data-confirm-verb=\"resume\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-control-action-refresh\" data-action-enabled=\"true\" data-action=\"refresh\" data-confirm-required=\"true\" data-confirm-title=\"Confirm refresh action\" data-confirm-body=\"Refresh command-center state from latest runtime artifacts.\" data-confirm-verb=\"refresh\""
        ));
}

#[test]
fn functional_spec_2814_c01_c02_c03_timeline_chart_and_range_markers_render() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "healthy".to_string(),
            health_reason: "no recent transport failures observed".to_string(),
            rollout_gate: "pass".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "6h".to_string(),
            timeline_point_count: 2,
            timeline_last_timestamp_unix_ms: 811,
            queue_depth: 1,
            failure_streak: 0,
            processed_case_count: 2,
            alert_count: 2,
            widget_count: 2,
            timeline_cycle_count: 2,
            timeline_invalid_cycle_count: 1,
            primary_alert_code: "dashboard_queue_backlog".to_string(),
            primary_alert_severity: "warning".to_string(),
            primary_alert_message: "runtime backlog detected (queue_depth=1)".to_string(),
            alert_feed_rows: vec![],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-queue-timeline-chart\""));
    assert!(html.contains("data-component=\"TimelineChart\""));
    assert!(html.contains("data-timeline-range=\"6h\""));
    assert!(html.contains("data-timeline-point-count=\"2\""));
    assert!(html.contains("data-timeline-last-timestamp=\"811\""));
    assert!(html.contains(
        "id=\"tau-ops-timeline-range-1h\" data-range-option=\"1h\" data-range-selected=\"false\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-timeline-range-6h\" data-range-option=\"6h\" data-range-selected=\"true\""
    ));
    assert!(html.contains(
        "id=\"tau-ops-timeline-range-24h\" data-range-option=\"24h\" data-range-selected=\"false\""
    ));
    assert!(html.contains("href=\"/ops?theme=light&amp;sidebar=collapsed&amp;range=1h\""));
    assert!(html.contains("href=\"/ops?theme=light&amp;sidebar=collapsed&amp;range=6h\""));
    assert!(html.contains("href=\"/ops?theme=light&amp;sidebar=collapsed&amp;range=24h\""));
}

#[test]
fn functional_spec_2850_c01_c02_c04_recent_cycles_table_renders_panel_and_summary_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Light,
        sidebar_state: TauOpsDashboardSidebarState::Collapsed,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            timeline_range: "6h".to_string(),
            timeline_point_count: 2,
            timeline_last_timestamp_unix_ms: 811,
            timeline_cycle_count: 2,
            timeline_invalid_cycle_count: 1,
            ..TauOpsDashboardCommandCenterSnapshot::default()
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(
        html.contains("id=\"tau-ops-data-table\" data-route=\"/ops\" data-timeline-range=\"6h\"")
    );
    assert!(html.contains(
            "id=\"tau-ops-timeline-summary-row\" data-row-kind=\"summary\" data-last-timestamp=\"811\" data-point-count=\"2\" data-cycle-count=\"2\" data-invalid-cycle-count=\"1\""
        ));
    assert!(!html.contains("id=\"tau-ops-timeline-empty-row\""));
}

#[test]
fn functional_spec_2850_c03_recent_cycles_table_renders_empty_state_marker() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            timeline_range: "1h".to_string(),
            timeline_point_count: 0,
            timeline_last_timestamp_unix_ms: 0,
            timeline_cycle_count: 0,
            timeline_invalid_cycle_count: 0,
            ..TauOpsDashboardCommandCenterSnapshot::default()
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(
        html.contains("id=\"tau-ops-data-table\" data-route=\"/ops\" data-timeline-range=\"1h\"")
    );
    assert!(html.contains(
            "id=\"tau-ops-timeline-summary-row\" data-row-kind=\"summary\" data-last-timestamp=\"0\" data-point-count=\"0\" data-cycle-count=\"0\" data-invalid-cycle-count=\"0\""
        ));
    assert!(html.contains("id=\"tau-ops-timeline-empty-row\" data-empty-state=\"true\""));
}

#[test]
fn functional_spec_2818_c01_c02_alert_feed_row_markers_render_for_snapshot_alerts() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "degraded".to_string(),
            health_reason: "runtime backlog detected".to_string(),
            rollout_gate: "hold".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "1h".to_string(),
            timeline_point_count: 1,
            timeline_last_timestamp_unix_ms: 900,
            queue_depth: 1,
            failure_streak: 0,
            processed_case_count: 1,
            alert_count: 2,
            widget_count: 1,
            timeline_cycle_count: 1,
            timeline_invalid_cycle_count: 0,
            primary_alert_code: "dashboard_queue_backlog".to_string(),
            primary_alert_severity: "warning".to_string(),
            primary_alert_message: "runtime backlog detected (queue_depth=1)".to_string(),
            alert_feed_rows: vec![
                TauOpsDashboardAlertFeedRow {
                    code: "dashboard_queue_backlog".to_string(),
                    severity: "warning".to_string(),
                    message: "runtime backlog detected (queue_depth=1)".to_string(),
                },
                TauOpsDashboardAlertFeedRow {
                    code: "dashboard_cycle_log_invalid_lines".to_string(),
                    severity: "warning".to_string(),
                    message: "runtime events log contains 1 malformed line(s)".to_string(),
                },
            ],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-alert-feed-list\""));
    assert!(html.contains(
            "id=\"tau-ops-alert-row-0\" data-alert-code=\"dashboard_queue_backlog\" data-alert-severity=\"warning\""
        ));
    assert!(html.contains(
            "id=\"tau-ops-alert-row-1\" data-alert-code=\"dashboard_cycle_log_invalid_lines\" data-alert-severity=\"warning\""
        ));
    assert!(html.contains("runtime backlog detected (queue_depth=1)"));
}

#[test]
fn functional_spec_2818_c03_alert_feed_row_markers_render_nominal_fallback_alert() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "healthy".to_string(),
            health_reason: "dashboard runtime health is nominal".to_string(),
            rollout_gate: "pass".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "1h".to_string(),
            timeline_point_count: 1,
            timeline_last_timestamp_unix_ms: 900,
            queue_depth: 0,
            failure_streak: 0,
            processed_case_count: 1,
            alert_count: 1,
            widget_count: 1,
            timeline_cycle_count: 1,
            timeline_invalid_cycle_count: 0,
            primary_alert_code: "dashboard_healthy".to_string(),
            primary_alert_severity: "info".to_string(),
            primary_alert_message: "dashboard runtime health is nominal".to_string(),
            alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                code: "dashboard_healthy".to_string(),
                severity: "info".to_string(),
                message: "dashboard runtime health is nominal".to_string(),
            }],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-alert-feed-list\""));
    assert!(html.contains(
            "id=\"tau-ops-alert-row-0\" data-alert-code=\"dashboard_healthy\" data-alert-severity=\"info\""
        ));
    assert!(html.contains("dashboard runtime health is nominal"));
}

#[test]
fn functional_spec_2822_c03_connector_health_table_renders_fallback_row_markers() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "healthy".to_string(),
            health_reason: "dashboard runtime health is nominal".to_string(),
            rollout_gate: "pass".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "1h".to_string(),
            timeline_point_count: 1,
            timeline_last_timestamp_unix_ms: 900,
            queue_depth: 0,
            failure_streak: 0,
            processed_case_count: 1,
            alert_count: 1,
            widget_count: 1,
            timeline_cycle_count: 1,
            timeline_invalid_cycle_count: 0,
            primary_alert_code: "dashboard_healthy".to_string(),
            primary_alert_severity: "info".to_string(),
            primary_alert_message: "dashboard runtime health is nominal".to_string(),
            alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                code: "dashboard_healthy".to_string(),
                severity: "info".to_string(),
                message: "dashboard runtime health is nominal".to_string(),
            }],
            connector_health_rows: vec![],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-connector-health-table\""));
    assert!(html.contains("id=\"tau-ops-connector-table-body\""));
    assert!(html.contains(
            "id=\"tau-ops-connector-row-0\" data-channel=\"none\" data-mode=\"unknown\" data-liveness=\"unknown\" data-events-ingested=\"0\" data-provider-failures=\"0\""
        ));
}

#[test]
fn functional_spec_2822_c01_c02_connector_health_table_rows_render_for_snapshot_connectors() {
    let html = render_tau_ops_dashboard_shell_with_context(TauOpsDashboardShellContext {
        auth_mode: TauOpsDashboardAuthMode::Token,
        active_route: TauOpsDashboardRoute::Ops,
        theme: TauOpsDashboardTheme::Dark,
        sidebar_state: TauOpsDashboardSidebarState::Expanded,
        command_center: TauOpsDashboardCommandCenterSnapshot {
            health_state: "degraded".to_string(),
            health_reason: "connector retry in progress".to_string(),
            rollout_gate: "hold".to_string(),
            control_mode: "running".to_string(),
            control_paused: false,
            action_pause_enabled: true,
            action_resume_enabled: false,
            action_refresh_enabled: true,
            last_action_request_id: "none".to_string(),
            last_action_name: "none".to_string(),
            last_action_actor: "none".to_string(),
            last_action_timestamp_unix_ms: 0,
            timeline_range: "1h".to_string(),
            timeline_point_count: 1,
            timeline_last_timestamp_unix_ms: 900,
            queue_depth: 0,
            failure_streak: 0,
            processed_case_count: 1,
            alert_count: 1,
            widget_count: 1,
            timeline_cycle_count: 1,
            timeline_invalid_cycle_count: 0,
            primary_alert_code: "dashboard_healthy".to_string(),
            primary_alert_severity: "info".to_string(),
            primary_alert_message: "dashboard runtime health is nominal".to_string(),
            alert_feed_rows: vec![TauOpsDashboardAlertFeedRow {
                code: "dashboard_healthy".to_string(),
                severity: "info".to_string(),
                message: "dashboard runtime health is nominal".to_string(),
            }],
            connector_health_rows: vec![TauOpsDashboardConnectorHealthRow {
                channel: "telegram".to_string(),
                mode: "polling".to_string(),
                liveness: "open".to_string(),
                events_ingested: 6,
                provider_failures: 2,
            }],
        },
        chat: TauOpsDashboardChatSnapshot::default(),
    });

    assert!(html.contains("id=\"tau-ops-connector-health-table\""));
    assert!(html.contains("id=\"tau-ops-connector-table-body\""));
    assert!(html.contains(
            "id=\"tau-ops-connector-row-0\" data-channel=\"telegram\" data-mode=\"polling\" data-liveness=\"open\" data-events-ingested=\"6\" data-provider-failures=\"2\""
        ));
}
