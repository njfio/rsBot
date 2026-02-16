use std::path::Path;
use std::sync::Arc;

use anyhow::Result;
use tau_access::approvals::{
    approval_paths_for_state_dir, execute_approvals_command_with_paths_and_actor,
};
use tau_access::pairing::{evaluate_pairing_access, pairing_policy_for_state_dir, PairingDecision};
use tau_diagnostics::{
    execute_doctor_command, execute_doctor_command_with_options, DoctorCheckOptions,
    DoctorCommandConfig, DoctorCommandOutputFormat,
};
use tau_multi_channel::{
    MultiChannelApprovalsCommandExecutor, MultiChannelAuthCommandExecutor,
    MultiChannelCommandHandlers, MultiChannelDoctorCommandExecutor, MultiChannelPairingDecision,
    MultiChannelPairingEvaluator,
};
use tau_provider::{execute_auth_command, AuthCommandConfig};

#[derive(Clone)]
struct MultiChannelAuthCommandHandler {
    config: AuthCommandConfig,
}

impl MultiChannelAuthCommandExecutor for MultiChannelAuthCommandHandler {
    fn execute_auth_status(&self, provider: Option<&str>) -> String {
        let mut args = String::from("status");
        if let Some(provider) = provider {
            args.push(' ');
            args.push_str(provider);
        }
        execute_auth_command(&self.config, &args)
    }
}

#[derive(Clone)]
struct MultiChannelDoctorCommandHandler {
    config: DoctorCommandConfig,
}

impl MultiChannelDoctorCommandExecutor for MultiChannelDoctorCommandHandler {
    fn execute_doctor(&self, online: bool) -> String {
        if online {
            execute_doctor_command_with_options(
                &self.config,
                DoctorCommandOutputFormat::Text,
                DoctorCheckOptions { online: true },
            )
        } else {
            execute_doctor_command(&self.config, DoctorCommandOutputFormat::Text)
        }
    }
}

#[derive(Clone, Default)]
struct MultiChannelApprovalsCommandHandler;

impl MultiChannelApprovalsCommandExecutor for MultiChannelApprovalsCommandHandler {
    fn execute_approvals(
        &self,
        state_dir: &Path,
        args: &str,
        decision_actor: Option<&str>,
    ) -> String {
        let (policy_path, store_path) = approval_paths_for_state_dir(state_dir);
        execute_approvals_command_with_paths_and_actor(
            args,
            &policy_path,
            &store_path,
            decision_actor,
        )
    }
}

#[derive(Clone, Default)]
struct MultiChannelPairingEvaluatorAdapter;

impl MultiChannelPairingEvaluator for MultiChannelPairingEvaluatorAdapter {
    fn evaluate_pairing(
        &self,
        state_dir: &Path,
        policy_channel: &str,
        actor_id: &str,
        now_unix_ms: u64,
    ) -> Result<MultiChannelPairingDecision> {
        let policy = pairing_policy_for_state_dir(state_dir);
        let decision = evaluate_pairing_access(&policy, policy_channel, actor_id, now_unix_ms)?;
        Ok(match decision {
            PairingDecision::Allow { reason_code } => {
                MultiChannelPairingDecision::Allow { reason_code }
            }
            PairingDecision::Deny { reason_code } => {
                MultiChannelPairingDecision::Deny { reason_code }
            }
        })
    }
}

/// Build multi-channel command handlers backed by startup auth/doctor configs.
pub fn build_multi_channel_command_handlers(
    auth_config: AuthCommandConfig,
    doctor_config: DoctorCommandConfig,
) -> MultiChannelCommandHandlers {
    MultiChannelCommandHandlers {
        auth: Some(Arc::new(MultiChannelAuthCommandHandler {
            config: auth_config,
        })),
        doctor: Some(Arc::new(MultiChannelDoctorCommandHandler {
            config: doctor_config,
        })),
        approvals: Some(Arc::new(MultiChannelApprovalsCommandHandler)),
    }
}

/// Build pairing evaluator adapter for multi-channel policy checks.
pub fn build_multi_channel_pairing_evaluator() -> Arc<dyn MultiChannelPairingEvaluator> {
    Arc::new(MultiChannelPairingEvaluatorAdapter)
}
