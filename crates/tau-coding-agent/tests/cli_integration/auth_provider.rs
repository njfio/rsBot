use super::*;

#[test]
fn integration_models_list_command_filters_catalog_entries() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    write_model_catalog(
        &catalog_path,
        json!([
            {
                "provider": "openai",
                "model": "gpt-4o-mini",
                "context_window_tokens": 128000,
                "supports_tools": true,
                "supports_multimodal": true,
                "supports_reasoning": true,
                "input_cost_per_million": 0.15,
                "output_cost_per_million": 0.6
            },
            {
                "provider": "openai",
                "model": "legacy-no-tools",
                "context_window_tokens": 8192,
                "supports_tools": false,
                "supports_multimodal": false,
                "supports_reasoning": false,
                "input_cost_per_million": null,
                "output_cost_per_million": null
            }
        ]),
    );

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--model-catalog-offline",
        "--no-session",
    ])
    .write_stdin("/models-list gpt --provider openai --tools true --limit 5\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("models list: source=cache:"))
        .stdout(predicate::str::contains("model: openai/gpt-4o-mini"))
        .stdout(predicate::str::contains("legacy-no-tools").not());
}

#[test]
fn regression_model_show_command_reports_not_found_and_continues() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    write_model_catalog(
        &catalog_path,
        json!([{
            "provider": "openai",
            "model": "gpt-4o-mini",
            "context_window_tokens": 128000,
            "supports_tools": true,
            "supports_multimodal": true,
            "supports_reasoning": true,
            "input_cost_per_million": 0.15,
            "output_cost_per_million": 0.6
        }]),
    );

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--model-catalog-offline",
        "--no-session",
    ])
    .write_stdin("/model-show openai/missing-model\n/help model-show\n/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("model show: not found"))
        .stdout(predicate::str::contains("command: /model-show"));
}

#[test]
fn integration_startup_model_catalog_remote_refresh_is_reported() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    let server = MockServer::start();
    let refresh = server.mock(|when, then| {
        when.method(GET).path("/models.json");
        then.status(200).json_body(json!({
            "schema_version": 1,
            "entries": [{
                "provider": "openai",
                "model": "gpt-4o-mini",
                "context_window_tokens": 128000,
                "supports_tools": true,
                "supports_multimodal": true,
                "supports_reasoning": true,
                "input_cost_per_million": 0.15,
                "output_cost_per_million": 0.6
            }]
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--model-catalog-url",
        &format!("{}/models.json", server.base_url()),
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--no-session",
    ])
    .write_stdin("/quit\n");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "model catalog: source=remote url=",
        ))
        .stdout(predicate::str::contains("entries=1"));
    refresh.assert_calls(1);
}

#[test]
fn regression_startup_rejects_tool_incompatible_model_from_catalog() {
    let temp = tempdir().expect("tempdir");
    let catalog_path = temp.path().join("models.json");
    write_model_catalog(
        &catalog_path,
        json!([{
            "provider": "openai",
            "model": "no-tools-model",
            "context_window_tokens": 8192,
            "supports_tools": false,
            "supports_multimodal": false,
            "supports_reasoning": false,
            "input_cost_per_million": null,
            "output_cost_per_million": null
        }]),
    );

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/no-tools-model",
        "--model-catalog-cache",
        catalog_path.to_str().expect("utf8 path"),
        "--model-catalog-offline",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("tool-incompatible"));
}

#[test]
fn openai_prompt_persists_session_and_supports_branch_from() {
    let server = MockServer::start();
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration openai response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 3, "total_tokens": 13}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let session = temp.path().join("session.jsonl");

    let mut first = binary_command();
    first.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "first prompt",
        "--session",
        session.to_str().expect("utf8 session path"),
    ]);

    first
        .assert()
        .success()
        .stdout(predicate::str::contains("integration openai response"));

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].message.role, "system");
    assert_eq!(entries[1].message.role, "user");
    assert_eq!(entries[2].message.role, "assistant");

    let mut second = binary_command();
    second.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "forked prompt",
        "--session",
        session.to_str().expect("utf8 session path"),
        "--branch-from",
        "2",
    ]);

    second.assert().success();

    let entries = parse_session_entries(&session);
    assert_eq!(entries.len(), 5);
    assert_eq!(entries[3].parent_id, Some(2));
    assert_eq!(entries[4].parent_id, Some(entries[3].id));

    openai.assert_calls(2);
}

#[test]
fn fallback_model_flag_routes_to_secondary_model_on_retryable_failure() {
    let server = MockServer::start();
    let primary = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("\"model\":\"gpt-primary\"")
            .header("x-tau-retry-attempt", "0");
        then.status(503).body("primary unavailable");
    });
    let fallback = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .body_includes("\"model\":\"gpt-fallback\"")
            .header("x-tau-retry-attempt", "0");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "fallback route response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 6, "completion_tokens": 2, "total_tokens": 8}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-primary",
        "--fallback-model",
        "openai/gpt-fallback",
        "--provider-max-retries",
        "0",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("fallback route response"));

    primary.assert_calls(1);
    fallback.assert_calls(1);
}

#[test]
fn integration_openrouter_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let openrouter = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration openrouter response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openrouter/openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("OPENROUTER_API_KEY", "test-openrouter-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration openrouter response"));

    openrouter.assert_calls(1);
}

#[test]
fn integration_groq_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let groq = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration groq response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "groq/llama-3.3-70b",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("GROQ_API_KEY", "test-groq-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration groq response"));

    groq.assert_calls(1);
}

#[test]
fn integration_xai_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let xai = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration xai response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "xai/grok-4",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("XAI_API_KEY", "test-xai-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration xai response"));

    xai.assert_calls(1);
}

#[test]
fn integration_mistral_alias_uses_openai_compatible_runtime_with_env_key() {
    let server = MockServer::start();
    let mistral = server.mock(|_, then| {
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration mistral response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "mistral/mistral-large-latest",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("MISTRAL_API_KEY", "test-mistral-key");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration mistral response"));

    mistral.assert_calls(1);
}

#[test]
fn integration_azure_alias_uses_openai_client_with_api_key_header_and_api_version() {
    let server = MockServer::start();
    let azure = server.mock(|when, then| {
        when.method(POST)
            .path("/openai/deployments/test-deployment/chat/completions")
            .query_param("api-version", "2024-10-21")
            .header_exists("api-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "integration azure response"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 3, "total_tokens": 11}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "azure/gpt-4o-mini",
        "--api-base",
        &format!("{}/openai/deployments/test-deployment", server.base_url()),
        "--azure-openai-api-version",
        "2024-10-21",
        "--prompt",
        "hello",
        "--no-session",
    ])
    .env("AZURE_OPENAI_API_KEY", "test-azure-key")
    .env_remove("OPENAI_API_KEY")
    .env_remove("TAU_API_KEY");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration azure response"));

    azure.assert_calls(1);
}

#[test]
fn anthropic_prompt_works_end_to_end() {
    let server = MockServer::start();
    let anthropic = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/messages")
            .header("x-api-key", "test-anthropic-key")
            .header("anthropic-version", "2023-06-01");
        then.status(200).json_body(json!({
            "content": [{"type": "text", "text": "integration anthropic response"}],
            "stop_reason": "end_turn",
            "usage": {"input_tokens": 8, "output_tokens": 3}
        }));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "anthropic/claude-sonnet-4-20250514",
        "--anthropic-api-base",
        &format!("{}/v1", server.base_url()),
        "--anthropic-api-key",
        "test-anthropic-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration anthropic response"));

    anthropic.assert_calls(1);
}

#[test]
fn google_prompt_works_end_to_end() {
    let server = MockServer::start();
    let google = server.mock(|when, then| {
        when.method(POST)
            .path("/models/gemini-2.5-pro:streamGenerateContent")
            .query_param("key", "test-google-key")
            .query_param("alt", "sse");
        then.status(200)
            .header("content-type", "text/event-stream")
            .body(concat!(
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"integration \"}]}}]}\n\n",
                "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"google response\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":8,\"candidatesTokenCount\":3,\"totalTokenCount\":11}}\n\n"
            ));
    });

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "google/gemini-2.5-pro",
        "--google-api-base",
        &server.base_url(),
        "--google-api-key",
        "test-google-key",
        "--prompt",
        "hello",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("integration google response"));

    google.assert_calls(1);
}
