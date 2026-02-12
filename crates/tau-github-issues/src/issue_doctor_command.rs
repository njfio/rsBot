#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Public struct `IssueDoctorCommand` used across Tau components.
pub struct IssueDoctorCommand {
    pub online: bool,
}

pub fn parse_issue_doctor_command(
    remainder: &str,
    usage_message: &str,
) -> std::result::Result<IssueDoctorCommand, String> {
    if remainder.is_empty() {
        return Ok(IssueDoctorCommand { online: false });
    }
    let tokens = remainder
        .split_whitespace()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    if tokens.len() == 1 && tokens[0] == "--online" {
        Ok(IssueDoctorCommand { online: true })
    } else {
        Err(usage_message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_issue_doctor_command, IssueDoctorCommand};

    #[test]
    fn unit_parse_issue_doctor_command_defaults_to_offline() {
        let parsed = parse_issue_doctor_command("", "Usage: /tau doctor [--online]")
            .expect("parse default doctor command");
        assert_eq!(parsed, IssueDoctorCommand { online: false });
    }

    #[test]
    fn functional_parse_issue_doctor_command_supports_online_flag() {
        let parsed = parse_issue_doctor_command("--online", "Usage: /tau doctor [--online]")
            .expect("parse online doctor command");
        assert_eq!(parsed, IssueDoctorCommand { online: true });
    }

    #[test]
    fn integration_parse_issue_doctor_command_trims_whitespace_before_tokenization() {
        let parsed = parse_issue_doctor_command("   --online   ", "Usage: /tau doctor [--online]")
            .expect("parse online doctor command with whitespace");
        assert_eq!(parsed, IssueDoctorCommand { online: true });
    }

    #[test]
    fn regression_parse_issue_doctor_command_rejects_extra_flags() {
        let err = parse_issue_doctor_command("--online --verbose", "Usage: /tau doctor [--online]")
            .expect_err("invalid doctor command should fail");
        assert_eq!(err, "Usage: /tau doctor [--online]");
    }
}
