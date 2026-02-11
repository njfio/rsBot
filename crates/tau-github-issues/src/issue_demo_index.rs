use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoIndexRunCommandSpec {
    pub scenarios: Vec<String>,
    pub timeout_seconds: u64,
}

pub fn parse_demo_index_run_command(
    raw: &str,
    allowed_scenarios: &[&str],
    default_timeout_seconds: u64,
    max_timeout_seconds: u64,
    usage_message: &str,
) -> Result<DemoIndexRunCommandSpec, String> {
    let usage = usage_message.to_string();
    let mut timeout_seconds = default_timeout_seconds;
    let mut scenarios = Vec::new();

    let tokens = raw
        .split_whitespace()
        .filter(|token| !token.trim().is_empty())
        .collect::<Vec<_>>();
    let mut cursor = 0;
    if let Some(first) = tokens.first() {
        if !first.starts_with("--") {
            cursor = 1;
            let mut seen = HashSet::new();
            for raw_scenario in first.split(',') {
                let normalized =
                    normalize_demo_index_scenario(raw_scenario).ok_or_else(|| usage.clone())?;
                if seen.insert(normalized) {
                    scenarios.push(normalized.to_string());
                }
            }
            if scenarios.is_empty() {
                return Err(usage);
            }
        }
    }

    while cursor < tokens.len() {
        let token = tokens[cursor];
        match token {
            "--timeout-seconds" => {
                cursor += 1;
                let Some(raw_timeout) = tokens.get(cursor) else {
                    return Err(usage);
                };
                let parsed = raw_timeout.parse::<u64>().map_err(|_| usage.clone())?;
                if parsed == 0 || parsed > max_timeout_seconds {
                    return Err(usage);
                }
                timeout_seconds = parsed;
            }
            _ => return Err(usage),
        }
        cursor += 1;
    }

    if scenarios.is_empty() {
        scenarios = allowed_scenarios
            .iter()
            .map(|scenario| scenario.to_string())
            .collect();
    }

    Ok(DemoIndexRunCommandSpec {
        scenarios,
        timeout_seconds,
    })
}

pub fn normalize_demo_index_scenario(raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "onboarding" | "local" | "onboarding.sh" | "local.sh" => Some("onboarding"),
        "gateway-auth" | "gatewayauth" | "gateway-auth.sh" | "gatewayauth.sh" => {
            Some("gateway-auth")
        }
        "multi-channel-live"
        | "multichannel-live"
        | "multi-channel"
        | "multi-channel-live.sh"
        | "multi-channel.sh" => Some("multi-channel-live"),
        "deployment-wasm" | "deploymentwasm" | "deployment" | "deployment.sh" => {
            Some("deployment-wasm")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_demo_index_scenario, parse_demo_index_run_command};

    const ALLOWED_SCENARIOS: [&str; 4] = [
        "onboarding",
        "gateway-auth",
        "multi-channel-live",
        "deployment-wasm",
    ];

    fn usage() -> &'static str {
        "Usage: /tau demo-index <list|run [scenario[,scenario...]] [--timeout-seconds <n>]|report>"
    }

    #[test]
    fn unit_normalize_demo_index_scenario_maps_known_aliases() {
        assert_eq!(
            normalize_demo_index_scenario("local.sh"),
            Some("onboarding")
        );
        assert_eq!(
            normalize_demo_index_scenario("gatewayauth"),
            Some("gateway-auth")
        );
        assert_eq!(
            normalize_demo_index_scenario("multi-channel.sh"),
            Some("multi-channel-live")
        );
        assert_eq!(
            normalize_demo_index_scenario("deployment"),
            Some("deployment-wasm")
        );
    }

    #[test]
    fn functional_parse_demo_index_run_command_defaults_to_all_scenarios() {
        let parsed =
            parse_demo_index_run_command("", &ALLOWED_SCENARIOS, 180, 900, usage()).unwrap();
        assert_eq!(
            parsed.scenarios,
            vec![
                "onboarding".to_string(),
                "gateway-auth".to_string(),
                "multi-channel-live".to_string(),
                "deployment-wasm".to_string(),
            ]
        );
        assert_eq!(parsed.timeout_seconds, 180);
    }

    #[test]
    fn integration_parse_demo_index_run_command_accepts_scenarios_and_timeout() {
        let parsed = parse_demo_index_run_command(
            "onboarding,gateway-auth --timeout-seconds 120",
            &ALLOWED_SCENARIOS,
            180,
            900,
            usage(),
        )
        .unwrap();
        assert_eq!(
            parsed.scenarios,
            vec!["onboarding".to_string(), "gateway-auth".to_string()]
        );
        assert_eq!(parsed.timeout_seconds, 120);
    }

    #[test]
    fn regression_parse_demo_index_run_command_rejects_invalid_tokens_and_bounds() {
        assert_eq!(
            parse_demo_index_run_command(
                "unknown-scenario",
                &ALLOWED_SCENARIOS,
                180,
                900,
                usage(),
            )
            .unwrap_err(),
            usage().to_string()
        );
        assert_eq!(
            parse_demo_index_run_command(
                "onboarding --timeout-seconds 0",
                &ALLOWED_SCENARIOS,
                180,
                900,
                usage(),
            )
            .unwrap_err(),
            usage().to_string()
        );
        assert_eq!(
            parse_demo_index_run_command(
                "onboarding --timeout-seconds 9999",
                &ALLOWED_SCENARIOS,
                180,
                900,
                usage(),
            )
            .unwrap_err(),
            usage().to_string()
        );
        assert_eq!(
            parse_demo_index_run_command(
                "onboarding --unknown-flag 12",
                &ALLOWED_SCENARIOS,
                180,
                900,
                usage(),
            )
            .unwrap_err(),
            usage().to_string()
        );
    }
}
