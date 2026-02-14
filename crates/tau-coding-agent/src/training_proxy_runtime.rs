//! Training attribution proxy runtime wiring.

use anyhow::{bail, Result};
use tau_cli::Cli;
use tau_training_proxy::{run_training_proxy, TrainingProxyConfig};

pub(crate) async fn run_training_proxy_mode_if_requested(cli: &Cli) -> Result<bool> {
    if !cli.training_proxy_server {
        return Ok(false);
    }

    if cli.train_config.is_some() {
        bail!("--training-proxy-server cannot be combined with --train-config");
    }

    let upstream_base_url = cli
        .training_proxy_upstream_url
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "--training-proxy-upstream-url is required when --training-proxy-server is enabled"
            )
        })?;

    run_training_proxy(TrainingProxyConfig {
        bind: cli.training_proxy_bind.clone(),
        upstream_base_url: upstream_base_url.to_string(),
        state_dir: cli.training_proxy_state_dir.clone(),
        request_timeout_ms: cli.training_proxy_timeout_ms,
    })
    .await?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::run_training_proxy_mode_if_requested;
    use clap::Parser;
    use std::path::PathBuf;
    use tau_cli::Cli;

    fn parse_cli_with_stack(args: &[&str]) -> Cli {
        let owned_args: Vec<String> = args.iter().map(|value| (*value).to_string()).collect();
        std::thread::Builder::new()
            .name("tau-cli-parse".to_string())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || Cli::parse_from(owned_args))
            .expect("spawn cli parse thread")
            .join()
            .expect("join cli parse thread")
    }

    #[tokio::test]
    async fn unit_training_proxy_mode_returns_false_when_disabled() {
        let cli = parse_cli_with_stack(&["tau-rs"]);
        let handled = run_training_proxy_mode_if_requested(&cli)
            .await
            .expect("proxy mode should be skipped");
        assert!(!handled);
    }

    #[tokio::test]
    async fn regression_training_proxy_mode_requires_upstream_url() {
        let mut cli = parse_cli_with_stack(&["tau-rs"]);
        cli.training_proxy_server = true;
        cli.training_proxy_upstream_url = None;
        cli.training_proxy_bind = "127.0.0.1:8790".to_string();
        cli.training_proxy_state_dir = PathBuf::from(".tau");

        let error = run_training_proxy_mode_if_requested(&cli)
            .await
            .expect_err("missing upstream URL must fail closed");
        assert!(error
            .to_string()
            .contains("--training-proxy-upstream-url is required"));
    }
}
