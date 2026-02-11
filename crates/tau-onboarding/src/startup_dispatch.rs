use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::Result;
use serde_json::Value;
use tau_cli::Cli;
use tau_skills::default_skills_lock_path;
use tau_tools::tools::ToolPolicy;

use crate::startup_policy::{resolve_startup_policy, StartupPolicyBundle};
use crate::startup_prompt_composition::compose_startup_system_prompt;

pub struct StartupRuntimeDispatchContext {
    pub effective_skills_dir: PathBuf,
    pub skills_lock_path: PathBuf,
    pub system_prompt: String,
    pub startup_policy: StartupPolicyBundle,
}

pub struct StartupRuntimeResolution<TModelRef, TFallbackModelRefs, TModelCatalog, TClient> {
    pub model_ref: TModelRef,
    pub fallback_model_refs: TFallbackModelRefs,
    pub model_catalog: TModelCatalog,
    pub client: TClient,
    pub runtime_dispatch_context: StartupRuntimeDispatchContext,
}

pub async fn execute_startup_runtime_modes<
    TModelRef,
    TFallbackModelRefs,
    TModelCatalog,
    TClient,
    TRenderOptions,
    FRunTransportModeIfRequested,
    FRunLocalRuntime,
>(
    cli: &Cli,
    runtime: StartupRuntimeResolution<TModelRef, TFallbackModelRefs, TModelCatalog, TClient>,
    render_options: TRenderOptions,
    run_transport_mode_if_requested: FRunTransportModeIfRequested,
    run_local_runtime: FRunLocalRuntime,
) -> Result<()>
where
    TRenderOptions: Clone,
    FRunTransportModeIfRequested:
        for<'a> FnOnce(
            &'a Cli,
            &'a TClient,
            &'a TModelRef,
            &'a str,
            &'a ToolPolicy,
            TRenderOptions,
        ) -> Pin<Box<dyn Future<Output = Result<bool>> + 'a>>,
    FRunLocalRuntime: for<'a> FnOnce(
        &'a Cli,
        TClient,
        TModelRef,
        TFallbackModelRefs,
        TModelCatalog,
        String,
        ToolPolicy,
        Value,
        TRenderOptions,
        PathBuf,
        PathBuf,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + 'a>>,
{
    let StartupRuntimeResolution {
        model_ref,
        fallback_model_refs,
        model_catalog,
        client,
        runtime_dispatch_context:
            StartupRuntimeDispatchContext {
                effective_skills_dir,
                skills_lock_path,
                system_prompt,
                startup_policy,
            },
    } = runtime;
    let StartupPolicyBundle {
        tool_policy,
        tool_policy_json,
    } = startup_policy;
    if run_transport_mode_if_requested(
        cli,
        &client,
        &model_ref,
        &system_prompt,
        &tool_policy,
        render_options.clone(),
    )
    .await?
    {
        return Ok(());
    }
    run_local_runtime(
        cli,
        client,
        model_ref,
        fallback_model_refs,
        model_catalog,
        system_prompt,
        tool_policy,
        tool_policy_json,
        render_options,
        effective_skills_dir,
        skills_lock_path,
    )
    .await
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartupModelRuntimeResolution<TModelRef, TFallbackModelRefs, TModelCatalog, TClient> {
    pub model_ref: TModelRef,
    pub fallback_model_refs: TFallbackModelRefs,
    pub model_catalog: TModelCatalog,
    pub client: TClient,
}

pub async fn resolve_startup_model_runtime_from_cli<
    TModelRef,
    TFallbackModelRefs,
    TModelCatalog,
    TClient,
    FResolveModels,
    FResolveModelCatalog,
    FValidateModelCatalog,
    FBuildClientWithFallbacks,
>(
    cli: &Cli,
    resolve_models: FResolveModels,
    resolve_model_catalog: FResolveModelCatalog,
    validate_model_catalog: FValidateModelCatalog,
    build_client_with_fallbacks: FBuildClientWithFallbacks,
) -> Result<StartupModelRuntimeResolution<TModelRef, TFallbackModelRefs, TModelCatalog, TClient>>
where
    FResolveModels: FnOnce(&Cli) -> Result<(TModelRef, TFallbackModelRefs)>,
    FResolveModelCatalog:
        for<'a> FnOnce(&'a Cli) -> Pin<Box<dyn Future<Output = Result<TModelCatalog>> + 'a>>,
    FValidateModelCatalog: FnOnce(&TModelCatalog, &TModelRef, &TFallbackModelRefs) -> Result<()>,
    FBuildClientWithFallbacks: FnOnce(&Cli, &TModelRef, &TFallbackModelRefs) -> Result<TClient>,
{
    let (model_ref, fallback_model_refs) = resolve_models(cli)?;
    let model_catalog = resolve_model_catalog(cli).await?;
    validate_model_catalog(&model_catalog, &model_ref, &fallback_model_refs)?;
    let client = build_client_with_fallbacks(cli, &model_ref, &fallback_model_refs)?;
    Ok(StartupModelRuntimeResolution {
        model_ref,
        fallback_model_refs,
        model_catalog,
        client,
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn resolve_startup_runtime_from_cli<
    TModelRef,
    TFallbackModelRefs,
    TModelCatalog,
    TClient,
    TSkillsBootstrap,
    TPackageActivation,
    FResolveModels,
    FResolveModelCatalog,
    FValidateModelCatalog,
    FBuildClientWithFallbacks,
    FRunSkillsBootstrap,
    FExecutePackageActivateOnStartup,
    FResolveBootstrapLockPath,
>(
    cli: &Cli,
    resolve_models: FResolveModels,
    resolve_model_catalog: FResolveModelCatalog,
    validate_model_catalog: FValidateModelCatalog,
    build_client_with_fallbacks: FBuildClientWithFallbacks,
    run_skills_bootstrap: FRunSkillsBootstrap,
    execute_package_activate_on_startup: FExecutePackageActivateOnStartup,
    resolve_bootstrap_lock_path: FResolveBootstrapLockPath,
) -> Result<StartupRuntimeResolution<TModelRef, TFallbackModelRefs, TModelCatalog, TClient>>
where
    FResolveModels: FnOnce(&Cli) -> Result<(TModelRef, TFallbackModelRefs)>,
    FResolveModelCatalog:
        for<'a> FnOnce(&'a Cli) -> Pin<Box<dyn Future<Output = Result<TModelCatalog>> + 'a>>,
    FValidateModelCatalog: FnOnce(&TModelCatalog, &TModelRef, &TFallbackModelRefs) -> Result<()>,
    FBuildClientWithFallbacks: FnOnce(&Cli, &TModelRef, &TFallbackModelRefs) -> Result<TClient>,
    FRunSkillsBootstrap:
        for<'a> FnOnce(&'a Cli) -> Pin<Box<dyn Future<Output = Result<TSkillsBootstrap>> + 'a>>,
    FExecutePackageActivateOnStartup: FnOnce(&Cli) -> Result<Option<TPackageActivation>>,
    FResolveBootstrapLockPath: FnOnce(&TSkillsBootstrap) -> PathBuf,
{
    let StartupModelRuntimeResolution {
        model_ref,
        fallback_model_refs,
        model_catalog,
        client,
    } = resolve_startup_model_runtime_from_cli(
        cli,
        resolve_models,
        resolve_model_catalog,
        validate_model_catalog,
        build_client_with_fallbacks,
    )
    .await?;
    let runtime_dispatch_context = resolve_startup_runtime_dispatch_context_from_cli(
        cli,
        run_skills_bootstrap,
        execute_package_activate_on_startup,
        resolve_bootstrap_lock_path,
    )
    .await?;
    Ok(StartupRuntimeResolution {
        model_ref,
        fallback_model_refs,
        model_catalog,
        client,
        runtime_dispatch_context,
    })
}

pub async fn resolve_startup_runtime_dispatch_context_from_cli<
    TSkillsBootstrap,
    TPackageActivation,
    FRunSkillsBootstrap,
    FExecutePackageActivateOnStartup,
    FResolveBootstrapLockPath,
>(
    cli: &Cli,
    run_skills_bootstrap: FRunSkillsBootstrap,
    execute_package_activate_on_startup: FExecutePackageActivateOnStartup,
    resolve_bootstrap_lock_path: FResolveBootstrapLockPath,
) -> Result<StartupRuntimeDispatchContext>
where
    FRunSkillsBootstrap:
        for<'a> FnOnce(&'a Cli) -> Pin<Box<dyn Future<Output = Result<TSkillsBootstrap>> + 'a>>,
    FExecutePackageActivateOnStartup: FnOnce(&Cli) -> Result<Option<TPackageActivation>>,
    FResolveBootstrapLockPath: FnOnce(&TSkillsBootstrap) -> PathBuf,
{
    let skills_bootstrap = run_skills_bootstrap(cli).await?;
    let activation_applied = execute_package_activate_on_startup(cli)?.is_some();
    let bootstrap_lock_path = resolve_bootstrap_lock_path(&skills_bootstrap);
    build_startup_runtime_dispatch_context(cli, &bootstrap_lock_path, activation_applied)
}

pub fn build_startup_runtime_dispatch_context(
    cli: &Cli,
    bootstrap_lock_path: &Path,
    activation_applied: bool,
) -> Result<StartupRuntimeDispatchContext> {
    let effective_skills_dir = resolve_runtime_skills_dir(cli, activation_applied);
    let skills_lock_path =
        resolve_runtime_skills_lock_path(cli, bootstrap_lock_path, &effective_skills_dir);
    let system_prompt = compose_startup_system_prompt(cli, &effective_skills_dir)?;
    let startup_policy = resolve_startup_policy(cli)?;
    Ok(StartupRuntimeDispatchContext {
        effective_skills_dir,
        skills_lock_path,
        system_prompt,
        startup_policy,
    })
}

pub fn resolve_runtime_skills_dir(cli: &Cli, activation_applied: bool) -> PathBuf {
    if !activation_applied {
        return cli.skills_dir.clone();
    }
    let activated_skills_dir = cli.package_activate_destination.join("skills");
    if activated_skills_dir.is_dir() {
        return activated_skills_dir;
    }
    cli.skills_dir.clone()
}

pub fn resolve_runtime_skills_lock_path(
    cli: &Cli,
    bootstrap_lock_path: &Path,
    effective_skills_dir: &Path,
) -> PathBuf {
    if effective_skills_dir == cli.skills_dir {
        bootstrap_lock_path.to_path_buf()
    } else {
        default_skills_lock_path(effective_skills_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_startup_runtime_dispatch_context, execute_startup_runtime_modes,
        resolve_runtime_skills_dir, resolve_runtime_skills_lock_path,
        resolve_startup_model_runtime_from_cli, resolve_startup_runtime_dispatch_context_from_cli,
        resolve_startup_runtime_from_cli, StartupModelRuntimeResolution, StartupRuntimeResolution,
    };
    use anyhow::{anyhow, Result};
    use clap::Parser;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tau_cli::Cli;
    use tau_skills::default_skills_lock_path;
    use tempfile::tempdir;

    fn parse_cli_with_stack() -> Cli {
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(|| Cli::parse_from(["tau-rs"]))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    #[derive(Debug, Clone)]
    struct MockSkillsBootstrap {
        skills_lock_path: PathBuf,
    }

    #[test]
    fn unit_resolve_runtime_skills_lock_path_prefers_bootstrap_lock_for_default_skills_dir() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        let skills_dir = workspace.path().join(".tau/skills");
        cli.skills_dir = skills_dir.clone();

        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let resolved = resolve_runtime_skills_lock_path(&cli, &bootstrap_lock_path, &skills_dir);
        assert_eq!(resolved, bootstrap_lock_path);
    }

    #[test]
    fn functional_resolve_runtime_skills_dir_prefers_activated_directory_when_present() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.skills_dir = workspace.path().join(".tau/skills");
        cli.package_activate_destination = workspace.path().join("packages-active");

        let activated_skills_dir = cli.package_activate_destination.join("skills");
        std::fs::create_dir_all(&activated_skills_dir).expect("create activated skills dir");

        let resolved = resolve_runtime_skills_dir(&cli, true);
        assert_eq!(resolved, activated_skills_dir);
    }

    #[test]
    fn regression_resolve_runtime_skills_dir_falls_back_when_activation_output_missing() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        let base_skills_dir = workspace.path().join(".tau/skills");
        cli.skills_dir = base_skills_dir.clone();
        cli.package_activate_destination = workspace.path().join("packages-active");

        let resolved = resolve_runtime_skills_dir(&cli, true);
        assert_eq!(resolved, base_skills_dir);
    }

    #[test]
    fn regression_resolve_runtime_skills_lock_path_uses_effective_directory_when_switched() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.skills_dir = workspace.path().join(".tau/skills");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");

        let activated_skills_dir = workspace.path().join("packages-active/skills");
        let resolved =
            resolve_runtime_skills_lock_path(&cli, &bootstrap_lock_path, &activated_skills_dir);

        assert_eq!(resolved, default_skills_lock_path(&activated_skills_dir));
        assert_ne!(resolved, bootstrap_lock_path);
    }

    #[test]
    fn functional_build_startup_runtime_dispatch_context_prefers_activated_runtime_paths() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "You are Tau.".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        cli.package_activate_destination = workspace.path().join("packages-active");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let activated_skills_dir = cli.package_activate_destination.join("skills");
        std::fs::create_dir_all(&activated_skills_dir).expect("create activated skills dir");

        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let context =
            build_startup_runtime_dispatch_context(&cli, &bootstrap_lock_path, true).expect("ok");

        assert_eq!(context.effective_skills_dir, activated_skills_dir);
        assert_eq!(
            context.skills_lock_path,
            default_skills_lock_path(&context.effective_skills_dir)
        );
        assert!(context.system_prompt.contains("You are Tau."));
        assert!(context.startup_policy.tool_policy_json.is_object());
    }

    #[test]
    fn integration_build_startup_runtime_dispatch_context_honors_system_prompt_file() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.skills_dir = workspace.path().join(".tau/skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let prompt_path = workspace.path().join("system_prompt.txt");
        std::fs::write(&prompt_path, "System prompt from file.").expect("write system prompt");
        cli.system_prompt_file = Some(prompt_path);
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");

        let context =
            build_startup_runtime_dispatch_context(&cli, &bootstrap_lock_path, false).expect("ok");

        assert!(context.system_prompt.contains("System prompt from file."));
        assert_eq!(context.skills_lock_path, bootstrap_lock_path);
    }

    #[test]
    fn regression_build_startup_runtime_dispatch_context_uses_bootstrap_lock_without_switch() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");

        let context =
            build_startup_runtime_dispatch_context(&cli, &bootstrap_lock_path, false).expect("ok");

        assert_eq!(context.effective_skills_dir, cli.skills_dir);
        assert_eq!(context.skills_lock_path, bootstrap_lock_path);
        assert!(context.system_prompt.contains("Tau system prompt"));
    }

    #[tokio::test]
    async fn unit_resolve_startup_runtime_dispatch_context_from_cli_uses_bootstrap_lock_without_activation(
    ) {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let bootstrap_lock_path_for_bootstrap = bootstrap_lock_path.clone();

        let context = resolve_startup_runtime_dispatch_context_from_cli(
            &cli,
            |_cli| {
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: bootstrap_lock_path_for_bootstrap.clone(),
                    })
                })
            },
            |_cli| Ok(None::<()>),
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await
        .expect("context");

        assert_eq!(context.effective_skills_dir, cli.skills_dir);
        assert_eq!(context.skills_lock_path, bootstrap_lock_path);
    }

    #[tokio::test]
    async fn functional_resolve_startup_runtime_dispatch_context_from_cli_uses_activated_runtime_paths(
    ) {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        cli.package_activate_destination = workspace.path().join("packages-active");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let activated_skills_dir = cli.package_activate_destination.join("skills");
        std::fs::create_dir_all(&activated_skills_dir).expect("create activated skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let bootstrap_lock_path_for_bootstrap = bootstrap_lock_path.clone();

        let context = resolve_startup_runtime_dispatch_context_from_cli(
            &cli,
            |_cli| {
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: bootstrap_lock_path_for_bootstrap.clone(),
                    })
                })
            },
            |_cli| Ok(Some("activated".to_string())),
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await
        .expect("context");

        assert_eq!(context.effective_skills_dir, activated_skills_dir);
        assert_eq!(
            context.skills_lock_path,
            default_skills_lock_path(&context.effective_skills_dir)
        );
        assert_ne!(context.skills_lock_path, bootstrap_lock_path);
    }

    #[tokio::test]
    async fn integration_resolve_startup_runtime_dispatch_context_from_cli_propagates_skills_bootstrap_errors(
    ) {
        let cli = parse_cli_with_stack();
        let result = resolve_startup_runtime_dispatch_context_from_cli(
            &cli,
            |_cli| Box::pin(async move { Err(anyhow!("skills bootstrap failed")) }),
            |_cli| -> Result<Option<()>> {
                panic!("activation callback should not run when skills bootstrap fails");
            },
            |_bootstrap: &MockSkillsBootstrap| PathBuf::from("unused"),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("skills bootstrap errors should propagate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("skills bootstrap failed"));
    }

    #[tokio::test]
    async fn regression_resolve_startup_runtime_dispatch_context_from_cli_propagates_package_activation_errors(
    ) {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let bootstrap_lock_path_for_bootstrap = bootstrap_lock_path.clone();

        let result = resolve_startup_runtime_dispatch_context_from_cli(
            &cli,
            |_cli| {
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: bootstrap_lock_path_for_bootstrap.clone(),
                    })
                })
            },
            |_cli| -> Result<Option<()>> { Err(anyhow!("package activation failed")) },
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("package activation errors should propagate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("package activation failed"));
    }

    #[tokio::test]
    async fn unit_resolve_startup_model_runtime_from_cli_returns_composed_outputs() {
        let cli = parse_cli_with_stack();
        let StartupModelRuntimeResolution {
            model_ref,
            fallback_model_refs,
            model_catalog,
            client,
        } = resolve_startup_model_runtime_from_cli(
            &cli,
            |_cli| {
                Ok((
                    "primary-model".to_string(),
                    vec!["fallback-a".to_string(), "fallback-b".to_string()],
                ))
            },
            |_cli| Box::pin(async { Ok("catalog-v1".to_string()) }),
            |catalog, model, fallback| {
                assert_eq!(catalog, "catalog-v1");
                assert_eq!(model, "primary-model");
                assert_eq!(fallback.len(), 2);
                Ok(())
            },
            |_cli, model, _fallback| Ok(format!("client:{model}")),
        )
        .await
        .expect("runtime");

        assert_eq!(model_ref, "primary-model");
        assert_eq!(
            fallback_model_refs,
            vec!["fallback-a".to_string(), "fallback-b".to_string()]
        );
        assert_eq!(model_catalog, "catalog-v1");
        assert_eq!(client, "client:primary-model");
    }

    #[tokio::test]
    async fn functional_resolve_startup_model_runtime_from_cli_builds_client_from_resolved_models()
    {
        let cli = parse_cli_with_stack();
        let build_calls = AtomicUsize::new(0);
        let runtime = resolve_startup_model_runtime_from_cli(
            &cli,
            |_cli| {
                Ok((
                    "primary".to_string(),
                    vec!["f1".to_string(), "f2".to_string()],
                ))
            },
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| Ok(()),
            |_cli, model, fallback| {
                build_calls.fetch_add(1, Ordering::Relaxed);
                Ok(format!("client:{model}+{}", fallback.len()))
            },
        )
        .await
        .expect("runtime");

        assert_eq!(build_calls.load(Ordering::Relaxed), 1);
        assert_eq!(runtime.client, "client:primary+2");
    }

    #[tokio::test]
    async fn integration_resolve_startup_model_runtime_from_cli_validates_before_client_build() {
        let cli = parse_cli_with_stack();
        let stage = AtomicUsize::new(0);
        let _runtime = resolve_startup_model_runtime_from_cli(
            &cli,
            |_cli| Ok(("primary".to_string(), vec!["fallback".to_string()])),
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| {
                stage.store(1, Ordering::Relaxed);
                Ok(())
            },
            |_cli, _model, _fallback| {
                assert_eq!(stage.load(Ordering::Relaxed), 1);
                stage.store(2, Ordering::Relaxed);
                Ok("client".to_string())
            },
        )
        .await
        .expect("runtime");

        assert_eq!(stage.load(Ordering::Relaxed), 2);
    }

    #[tokio::test]
    async fn regression_resolve_startup_model_runtime_from_cli_propagates_validation_errors() {
        let cli = parse_cli_with_stack();
        let error = resolve_startup_model_runtime_from_cli(
            &cli,
            |_cli| Ok(("primary".to_string(), vec!["fallback".to_string()])),
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| Err(anyhow!("catalog validation failed")),
            |_cli, _model, _fallback| -> Result<String> {
                panic!("client builder should not run after validation error");
            },
        )
        .await
        .expect_err("validation errors should propagate");

        assert!(error.to_string().contains("catalog validation failed"));
    }

    #[tokio::test]
    async fn unit_resolve_startup_runtime_from_cli_returns_composed_outputs() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let bootstrap_lock_path_for_bootstrap = bootstrap_lock_path.clone();

        let StartupRuntimeResolution {
            model_ref,
            fallback_model_refs,
            model_catalog,
            client,
            runtime_dispatch_context,
        } = resolve_startup_runtime_from_cli(
            &cli,
            |_cli| Ok(("primary".to_string(), vec!["fallback".to_string()])),
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| Ok(()),
            |_cli, model, _fallback| Ok(format!("client:{model}")),
            |_cli| {
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: bootstrap_lock_path_for_bootstrap.clone(),
                    })
                })
            },
            |_cli| Ok(None::<()>),
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await
        .expect("runtime");

        assert_eq!(model_ref, "primary");
        assert_eq!(fallback_model_refs, vec!["fallback".to_string()]);
        assert_eq!(model_catalog, "catalog");
        assert_eq!(client, "client:primary");
        assert_eq!(
            runtime_dispatch_context.effective_skills_dir,
            cli.skills_dir
        );
        assert_eq!(
            runtime_dispatch_context.skills_lock_path,
            bootstrap_lock_path
        );
    }

    #[tokio::test]
    async fn functional_resolve_startup_runtime_from_cli_uses_activated_runtime_paths() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        cli.package_activate_destination = workspace.path().join("packages-active");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let activated_skills_dir = cli.package_activate_destination.join("skills");
        std::fs::create_dir_all(&activated_skills_dir).expect("create activated skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let bootstrap_lock_path_for_bootstrap = bootstrap_lock_path.clone();

        let resolution = resolve_startup_runtime_from_cli(
            &cli,
            |_cli| Ok(("primary".to_string(), vec!["fallback".to_string()])),
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| Ok(()),
            |_cli, model, fallback| Ok(format!("client:{model}+{}", fallback.len())),
            |_cli| {
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: bootstrap_lock_path_for_bootstrap.clone(),
                    })
                })
            },
            |_cli| Ok(Some("activated".to_string())),
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await
        .expect("runtime");

        assert_eq!(resolution.client, "client:primary+1");
        assert_eq!(
            resolution.runtime_dispatch_context.effective_skills_dir,
            activated_skills_dir
        );
        assert_eq!(
            resolution.runtime_dispatch_context.skills_lock_path,
            default_skills_lock_path(&resolution.runtime_dispatch_context.effective_skills_dir)
        );
    }

    #[tokio::test]
    async fn integration_resolve_startup_runtime_from_cli_short_circuits_when_model_resolution_fails(
    ) {
        let cli = parse_cli_with_stack();
        let model_catalog_calls = AtomicUsize::new(0);
        let model_validation_calls = AtomicUsize::new(0);
        let client_builder_calls = AtomicUsize::new(0);
        let dispatch_calls = AtomicUsize::new(0);

        let result = resolve_startup_runtime_from_cli(
            &cli,
            |_cli| -> Result<(String, Vec<String>)> { Err(anyhow!("model resolution failed")) },
            |_cli| {
                model_catalog_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Ok("catalog".to_string()) })
            },
            |_catalog: &String, _model: &String, _fallback: &Vec<String>| {
                model_validation_calls.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
            |_cli, _model: &String, _fallback: &Vec<String>| {
                client_builder_calls.fetch_add(1, Ordering::Relaxed);
                Ok("client".to_string())
            },
            |_cli| {
                dispatch_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: PathBuf::from("unused"),
                    })
                })
            },
            |_cli| Ok(None::<()>),
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("model resolution errors should propagate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("model resolution failed"));
        assert_eq!(model_catalog_calls.load(Ordering::Relaxed), 0);
        assert_eq!(model_validation_calls.load(Ordering::Relaxed), 0);
        assert_eq!(client_builder_calls.load(Ordering::Relaxed), 0);
        assert_eq!(dispatch_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn regression_resolve_startup_runtime_from_cli_propagates_dispatch_errors() {
        let cli = parse_cli_with_stack();
        let result = resolve_startup_runtime_from_cli(
            &cli,
            |_cli| Ok(("primary".to_string(), vec!["fallback".to_string()])),
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| Ok(()),
            |_cli, _model, _fallback| Ok("client".to_string()),
            |_cli| Box::pin(async move { Err(anyhow!("skills bootstrap failed")) }),
            |_cli| -> Result<Option<()>> {
                panic!("activation callback should not run when skills bootstrap fails");
            },
            |_bootstrap: &MockSkillsBootstrap| PathBuf::from("unused"),
        )
        .await;
        let error = match result {
            Ok(_) => panic!("dispatch errors should propagate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("skills bootstrap failed"));
    }

    #[tokio::test]
    async fn unit_execute_startup_runtime_modes_runs_local_when_transport_not_requested() {
        let mut cli = parse_cli_with_stack();
        let workspace = tempdir().expect("tempdir");
        cli.system_prompt = "Tau system prompt".to_string();
        cli.skills_dir = workspace.path().join(".tau/skills");
        std::fs::create_dir_all(&cli.skills_dir).expect("create skills dir");
        let bootstrap_lock_path = workspace.path().join(".tau/skills.lock.json");
        let bootstrap_lock_path_for_bootstrap = bootstrap_lock_path.clone();
        let runtime = resolve_startup_runtime_from_cli(
            &cli,
            |_cli| Ok(("primary".to_string(), vec!["fallback".to_string()])),
            |_cli| Box::pin(async { Ok("catalog".to_string()) }),
            |_catalog, _model, _fallback| Ok(()),
            |_cli, model, _fallback| Ok(format!("client:{model}")),
            |_cli| {
                Box::pin(async move {
                    Ok(MockSkillsBootstrap {
                        skills_lock_path: bootstrap_lock_path_for_bootstrap.clone(),
                    })
                })
            },
            |_cli| Ok(None::<()>),
            |bootstrap| bootstrap.skills_lock_path.clone(),
        )
        .await
        .expect("runtime");
        let expected_skills_dir = cli.skills_dir.clone();
        let transport_calls = AtomicUsize::new(0);
        let local_calls = AtomicUsize::new(0);
        execute_startup_runtime_modes(
            &cli,
            runtime,
            "render-v1".to_string(),
            |_cli, _client, _model_ref, _system_prompt, _tool_policy, render_options| {
                transport_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move {
                    assert_eq!(render_options, "render-v1");
                    Ok(false)
                })
            },
            |_cli,
             client,
             model_ref,
             fallback_model_refs,
             model_catalog,
             system_prompt,
             _tool_policy,
             tool_policy_json,
             render_options,
             effective_skills_dir,
             skills_lock_path| {
                local_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move {
                    assert_eq!(client, "client:primary");
                    assert_eq!(model_ref, "primary");
                    assert_eq!(fallback_model_refs, vec!["fallback".to_string()]);
                    assert_eq!(model_catalog, "catalog");
                    assert!(system_prompt.contains("Tau system prompt"));
                    assert_eq!(render_options, "render-v1");
                    assert!(tool_policy_json.is_object());
                    assert_eq!(effective_skills_dir, expected_skills_dir);
                    assert_eq!(skills_lock_path, bootstrap_lock_path);
                    Ok(())
                })
            },
        )
        .await
        .expect("dispatch");

        assert_eq!(transport_calls.load(Ordering::Relaxed), 1);
        assert_eq!(local_calls.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn functional_execute_startup_runtime_modes_short_circuits_local_when_transport_handles()
    {
        let cli = parse_cli_with_stack();
        let runtime = StartupRuntimeResolution {
            model_ref: "primary".to_string(),
            fallback_model_refs: vec!["fallback".to_string()],
            model_catalog: "catalog".to_string(),
            client: "client".to_string(),
            runtime_dispatch_context: build_startup_runtime_dispatch_context(
                &cli,
                Path::new(".tau/skills.lock.json"),
                false,
            )
            .expect("runtime context"),
        };
        let local_calls = AtomicUsize::new(0);
        execute_startup_runtime_modes(
            &cli,
            runtime,
            "render-v1".to_string(),
            |_cli, _client, _model_ref, _system_prompt, _tool_policy, _render_options| {
                Box::pin(async { Ok(true) })
            },
            |_cli,
             _client,
             _model_ref,
             _fallback_model_refs,
             _model_catalog,
             _system_prompt,
             _tool_policy,
             _tool_policy_json,
             _render_options,
             _effective_skills_dir,
             _skills_lock_path| {
                local_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move { Ok(()) })
            },
        )
        .await
        .expect("dispatch");

        assert_eq!(local_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn integration_execute_startup_runtime_modes_propagates_transport_errors() {
        let cli = parse_cli_with_stack();
        let runtime = StartupRuntimeResolution {
            model_ref: "primary".to_string(),
            fallback_model_refs: vec!["fallback".to_string()],
            model_catalog: "catalog".to_string(),
            client: "client".to_string(),
            runtime_dispatch_context: build_startup_runtime_dispatch_context(
                &cli,
                Path::new(".tau/skills.lock.json"),
                false,
            )
            .expect("runtime context"),
        };
        let local_calls = AtomicUsize::new(0);
        let result = execute_startup_runtime_modes(
            &cli,
            runtime,
            "render-v1".to_string(),
            |_cli, _client, _model_ref, _system_prompt, _tool_policy, _render_options| {
                Box::pin(async { Err(anyhow!("transport failed")) })
            },
            |_cli,
             _client,
             _model_ref,
             _fallback_model_refs,
             _model_catalog,
             _system_prompt,
             _tool_policy,
             _tool_policy_json,
             _render_options,
             _effective_skills_dir,
             _skills_lock_path| {
                local_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move { Ok(()) })
            },
        )
        .await;
        let error = match result {
            Ok(_) => panic!("transport errors should propagate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("transport failed"));
        assert_eq!(local_calls.load(Ordering::Relaxed), 0);
    }

    #[tokio::test]
    async fn regression_execute_startup_runtime_modes_propagates_local_runtime_errors() {
        let cli = parse_cli_with_stack();
        let runtime = StartupRuntimeResolution {
            model_ref: "primary".to_string(),
            fallback_model_refs: vec!["fallback".to_string()],
            model_catalog: "catalog".to_string(),
            client: "client".to_string(),
            runtime_dispatch_context: build_startup_runtime_dispatch_context(
                &cli,
                Path::new(".tau/skills.lock.json"),
                false,
            )
            .expect("runtime context"),
        };
        let transport_calls = AtomicUsize::new(0);
        let local_calls = AtomicUsize::new(0);
        let result = execute_startup_runtime_modes(
            &cli,
            runtime,
            "render-v1".to_string(),
            |_cli, _client, _model_ref, _system_prompt, _tool_policy, _render_options| {
                transport_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move { Ok(false) })
            },
            |_cli,
             _client,
             _model_ref,
             _fallback_model_refs,
             _model_catalog,
             _system_prompt,
             _tool_policy,
             _tool_policy_json,
             _render_options,
             _effective_skills_dir,
             _skills_lock_path| {
                local_calls.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move { Err(anyhow!("local runtime failed")) })
            },
        )
        .await;
        let error = match result {
            Ok(_) => panic!("local runtime errors should propagate"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("local runtime failed"));
        assert_eq!(transport_calls.load(Ordering::Relaxed), 1);
        assert_eq!(local_calls.load(Ordering::Relaxed), 1);
    }
}
