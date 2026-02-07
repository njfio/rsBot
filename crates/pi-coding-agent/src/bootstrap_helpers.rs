use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;

use crate::CliCommandFileErrorMode;

pub(crate) fn command_file_error_mode_label(mode: CliCommandFileErrorMode) -> &'static str {
    match mode {
        CliCommandFileErrorMode::FailFast => "fail-fast",
        CliCommandFileErrorMode::ContinueOnError => "continue-on-error",
    }
}

pub(crate) fn init_tracing() {
    let env_filter = EnvFilter::builder()
        .with_default_directive(LevelFilter::WARN.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .compact()
        .init();
}
