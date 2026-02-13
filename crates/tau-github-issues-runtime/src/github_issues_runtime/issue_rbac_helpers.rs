use super::{EventAction, TauIssueAuthCommandKind, TauIssueCommand};

pub(super) fn rbac_action_for_event(action: &EventAction) -> String {
    match action {
        EventAction::RunPrompt { .. } => "command:/tau-run".to_string(),
        EventAction::Command(command) => match command {
            TauIssueCommand::Run { .. } => "command:/tau-run".to_string(),
            TauIssueCommand::Stop => "command:/tau-stop".to_string(),
            TauIssueCommand::Status => "command:/tau-status".to_string(),
            TauIssueCommand::Health => "command:/tau-health".to_string(),
            TauIssueCommand::Compact => "command:/tau-compact".to_string(),
            TauIssueCommand::Help => "command:/tau-help".to_string(),
            TauIssueCommand::ChatStart => "command:/tau-chat-start".to_string(),
            TauIssueCommand::ChatResume => "command:/tau-chat-resume".to_string(),
            TauIssueCommand::ChatReset => "command:/tau-chat-reset".to_string(),
            TauIssueCommand::ChatExport => "command:/tau-chat-export".to_string(),
            TauIssueCommand::ChatStatus => "command:/tau-chat-status".to_string(),
            TauIssueCommand::ChatSummary => "command:/tau-chat-summary".to_string(),
            TauIssueCommand::ChatReplay => "command:/tau-chat-replay".to_string(),
            TauIssueCommand::ChatShow { .. } => "command:/tau-chat-show".to_string(),
            TauIssueCommand::ChatSearch { .. } => "command:/tau-chat-search".to_string(),
            TauIssueCommand::Artifacts { .. } => "command:/tau-artifacts".to_string(),
            TauIssueCommand::ArtifactShow { .. } => "command:/tau-artifacts-show".to_string(),
            TauIssueCommand::DemoIndexList => "command:/tau-demo-index".to_string(),
            TauIssueCommand::DemoIndexRun { .. } => "command:/tau-demo-index".to_string(),
            TauIssueCommand::DemoIndexReport => "command:/tau-demo-index".to_string(),
            TauIssueCommand::Auth { command } => match command.kind {
                TauIssueAuthCommandKind::Status => "command:/tau-auth-status".to_string(),
                TauIssueAuthCommandKind::Matrix => "command:/tau-auth-matrix".to_string(),
            },
            TauIssueCommand::Doctor { .. } => "command:/tau-doctor".to_string(),
            TauIssueCommand::Canvas { .. } => "command:/tau-canvas".to_string(),
            TauIssueCommand::Summarize { .. } => "command:/tau-summarize".to_string(),
            TauIssueCommand::Invalid { .. } => "command:/tau-invalid".to_string(),
        },
    }
}
