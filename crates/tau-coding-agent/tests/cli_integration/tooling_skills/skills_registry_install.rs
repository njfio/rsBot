//! Integration coverage for remote/registry skill installs, offline replay, and signature enforcement.

use super::*;

#[test]
fn install_skill_url_with_sha256_verification_works_end_to_end() {
    let server = MockServer::start();
    let remote_body = "Remote checksum skill";
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));

    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/remote.md");
        then.status(200).body(remote_body);
    });

    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: remote\nRemote checksum skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok remote"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 1, "total_tokens": 8}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--install-skill-url",
        &format!("{}/skills/remote.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "remote",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "remote skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok remote"));
    assert!(skills_dir.join("remote.md").exists());
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn integration_install_skill_url_offline_replay_uses_cache_without_network() {
    let server = MockServer::start();
    let remote_body = "Remote cached skill";
    let checksum = format!("{:x}", Sha256::digest(remote_body.as_bytes()));

    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/cached.md");
        then.status(200).body(remote_body);
    });

    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok remote cache"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 7, "completion_tokens": 1, "total_tokens": 8}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let cache_dir = temp.path().join("skills-cache");

    let mut warm = binary_command();
    warm.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--install-skill-url",
        &format!("{}/skills/cached.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "cached",
        "--no-session",
    ]);
    warm.assert().success();

    let mut replay = binary_command();
    replay.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--install-skill-url",
        &format!("{}/skills/cached.md", server.base_url()),
        "--install-skill-sha256",
        &checksum,
        "--skill",
        "cached",
        "--no-session",
    ]);
    replay
        .assert()
        .success()
        .stdout(predicate::str::contains("remote skills install:"));

    remote.assert_calls(1);
    openai.assert_calls(2);
}

#[test]
fn regression_skills_offline_mode_without_warm_remote_cache_fails() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--install-skill-url",
        "https://example.com/skills/missing.md",
        "--install-skill-sha256",
        "deadbeef",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("offline cache miss for skill URL"));
}

#[test]
fn install_skill_from_registry_works_end_to_end() {
    let server = MockServer::start();
    let skill_body = "Registry-driven skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "reg",
            "url": format!("{}/skills/reg.md", server.base_url()),
            "sha256": skill_sha
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key")
            .json_body_includes(
                json!({
                    "messages": [{
                        "role": "system",
                        "content": "base\n\n# Skill: reg\nRegistry-driven skill"
                    }]
                })
                .to_string(),
            );
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok registry"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg",
        "--skill",
        "reg",
        "--no-session",
    ]);

    cmd.assert()
        .success()
        .stdout(predicate::str::contains(
            "registry skills install: installed=1",
        ))
        .stdout(predicate::str::contains("ok registry"));
    assert!(skills_dir.join("reg.md").exists());
    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(1);
}

#[test]
fn integration_install_skill_from_registry_offline_replay_uses_cache_without_network() {
    let server = MockServer::start();
    let skill_body = "Registry cached skill";
    let skill_sha = format!("{:x}", Sha256::digest(skill_body.as_bytes()));
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "reg-cache",
            "url": format!("{}/skills/reg-cache.md", server.base_url()),
            "sha256": skill_sha
        }]
    })
    .to_string();
    let registry_sha = format!("{:x}", Sha256::digest(registry_body.as_bytes()));

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry-cache.json");
        then.status(200).body(registry_body);
    });
    let remote = server.mock(|when, then| {
        when.method(GET).path("/skills/reg-cache.md");
        then.status(200).body(skill_body);
    });
    let openai = server.mock(|when, then| {
        when.method(POST)
            .path("/v1/chat/completions")
            .header("authorization", "Bearer test-openai-key");
        then.status(200).json_body(json!({
            "choices": [{
                "message": {"content": "ok registry cache"},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 8, "completion_tokens": 1, "total_tokens": 9}
        }));
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");
    let cache_dir = temp.path().join("skills-cache");

    let mut warm = binary_command();
    warm.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry-cache.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg-cache",
        "--skill",
        "reg-cache",
        "--no-session",
    ]);
    warm.assert().success();

    let mut replay = binary_command();
    replay.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--system-prompt",
        "base",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-cache-dir",
        cache_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--skill-registry-url",
        &format!("{}/registry-cache.json", server.base_url()),
        "--skill-registry-sha256",
        &registry_sha,
        "--install-skill-from-registry",
        "reg-cache",
        "--skill",
        "reg-cache",
        "--no-session",
    ]);
    replay
        .assert()
        .success()
        .stdout(predicate::str::contains("registry skills install:"));

    registry.assert_calls(1);
    remote.assert_calls(1);
    openai.assert_calls(2);
}

#[test]
fn regression_skills_offline_mode_without_warm_registry_cache_fails() {
    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skills-offline",
        "--skill-registry-url",
        "https://example.com/registry.json",
        "--install-skill-from-registry",
        "review",
        "--no-session",
    ]);

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("offline cache miss for registry"));
}

#[test]
fn require_signed_skills_rejects_unsigned_registry_entries() {
    let server = MockServer::start();
    let registry_body = json!({
        "version": 1,
        "skills": [{
            "name": "unsigned",
            "url": format!("{}/skills/unsigned.md", server.base_url())
        }]
    })
    .to_string();

    let registry = server.mock(|when, then| {
        when.method(GET).path("/registry.json");
        then.status(200).body(registry_body);
    });

    let temp = tempdir().expect("tempdir");
    let skills_dir = temp.path().join("skills");

    let mut cmd = binary_command();
    cmd.args([
        "--model",
        "openai/gpt-4o-mini",
        "--api-base",
        &format!("{}/v1", server.base_url()),
        "--openai-api-key",
        "test-openai-key",
        "--prompt",
        "hello",
        "--skills-dir",
        skills_dir.to_str().expect("utf8 path"),
        "--skill-registry-url",
        &format!("{}/registry.json", server.base_url()),
        "--require-signed-skills",
        "--install-skill-from-registry",
        "unsigned",
        "--no-session",
    ]);

    cmd.assert().failure().stderr(predicate::str::contains(
        "unsigned but signatures are required",
    ));
    registry.assert_calls(1);
}
