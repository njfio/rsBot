use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use tokio::net::lookup_host;
use url::Url;

/// Configures outbound SSRF guardrails for HTTP clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SsrfProtectionConfig {
    /// Enables or disables all SSRF checks.
    pub enabled: bool,
    /// Allows plain HTTP URLs when true; HTTPS is always allowed.
    pub allow_http: bool,
    /// Allows loopback/private/link-local targets when true.
    pub allow_private_network: bool,
}

impl Default for SsrfProtectionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            allow_http: false,
            allow_private_network: false,
        }
    }
}

/// Structured SSRF validation error with stable reason codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SsrfViolation {
    /// Stable machine-readable reason code.
    pub reason_code: String,
    /// Human-readable detail for logs and diagnostics.
    pub detail: String,
}

impl std::fmt::Display for SsrfViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "reason_code={} detail={}", self.reason_code, self.detail)
    }
}

impl std::error::Error for SsrfViolation {}

/// Reusable outbound URL guard that blocks SSRF-prone destinations.
#[derive(Debug, Clone)]
pub struct SsrfGuard {
    config: SsrfProtectionConfig,
}

impl SsrfGuard {
    /// Builds a new SSRF guard with the provided policy.
    pub fn new(config: SsrfProtectionConfig) -> Self {
        Self { config }
    }

    /// Parses and validates a URL under the configured SSRF policy.
    pub async fn parse_and_validate_url(&self, raw_url: &str) -> Result<Url, SsrfViolation> {
        let url = Url::parse(raw_url).map_err(|error| {
            violation(
                "delivery_ssrf_invalid_url",
                format!("invalid outbound URL '{raw_url}': {error}"),
            )
        })?;
        self.validate_url(&url).await?;
        Ok(url)
    }

    /// Validates a parsed URL under the configured SSRF policy.
    pub async fn validate_url(&self, url: &Url) -> Result<(), SsrfViolation> {
        if !self.config.enabled {
            return Ok(());
        }
        validate_scheme(url, self.config.allow_http)?;
        let host = normalized_host(url)?;
        if is_metadata_hostname(&host) {
            return Err(violation(
                "delivery_ssrf_blocked_metadata_endpoint",
                format!("blocked outbound metadata hostname '{}'", host),
            ));
        }
        if is_localhost_hostname(&host) && !self.config.allow_private_network {
            return Err(violation(
                "delivery_ssrf_blocked_private_network",
                format!("blocked outbound localhost hostname '{}'", host),
            ));
        }
        if let Ok(ip_addr) = host.parse::<IpAddr>() {
            validate_ip(ip_addr, self.config.allow_private_network)?;
            return Ok(());
        }

        let port = url.port_or_known_default().ok_or_else(|| {
            violation(
                "delivery_ssrf_invalid_url",
                format!("URL '{}' does not include a known default port", url),
            )
        })?;
        let lookup_target = format!("{host}:{port}");
        let addresses = lookup_host(lookup_target.as_str()).await.map_err(|error| {
            violation(
                "delivery_ssrf_dns_resolution_failed",
                format!(
                    "failed DNS resolution for host '{host}' while validating URL '{}': {error}",
                    url
                ),
            )
        })?;

        let mut resolved_any = false;
        for socket_addr in addresses {
            resolved_any = true;
            validate_ip(socket_addr.ip(), self.config.allow_private_network)?;
        }
        if !resolved_any {
            return Err(violation(
                "delivery_ssrf_dns_resolution_failed",
                format!(
                    "host '{host}' resolved no addresses while validating URL '{}'",
                    url
                ),
            ));
        }
        Ok(())
    }
}

fn violation(reason_code: &str, detail: String) -> SsrfViolation {
    SsrfViolation {
        reason_code: reason_code.to_string(),
        detail,
    }
}

fn validate_scheme(url: &Url, allow_http: bool) -> Result<(), SsrfViolation> {
    match url.scheme() {
        "https" => Ok(()),
        "http" if allow_http => Ok(()),
        "http" => Err(violation(
            "delivery_ssrf_blocked_scheme",
            format!("blocked non-HTTPS outbound URL '{}'", url),
        )),
        scheme => Err(violation(
            "delivery_ssrf_blocked_scheme",
            format!(
                "blocked unsupported outbound scheme '{scheme}' for URL '{}'",
                url
            ),
        )),
    }
}

fn normalized_host(url: &Url) -> Result<String, SsrfViolation> {
    let host = url.host_str().ok_or_else(|| {
        violation(
            "delivery_ssrf_invalid_url",
            format!("URL '{}' is missing a host", url),
        )
    })?;
    let host = host.trim().trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() {
        return Err(violation(
            "delivery_ssrf_invalid_url",
            format!("URL '{}' resolved to an empty host", url),
        ));
    }
    Ok(host)
}

fn is_localhost_hostname(host: &str) -> bool {
    host == "localhost" || host.ends_with(".localhost")
}

fn is_metadata_hostname(host: &str) -> bool {
    matches!(
        host,
        "metadata"
            | "metadata.google.internal"
            | "instance-data"
            | "instance-data.ec2.internal"
            | "metadata.azure.internal"
    )
}

fn validate_ip(ip_addr: IpAddr, allow_private_network: bool) -> Result<(), SsrfViolation> {
    if is_metadata_ip(ip_addr) {
        return Err(violation(
            "delivery_ssrf_blocked_metadata_endpoint",
            format!("blocked outbound metadata IP '{}'", ip_addr),
        ));
    }
    if ip_addr.is_unspecified() {
        return Err(violation(
            "delivery_ssrf_blocked_unspecified_ip",
            format!("blocked outbound unspecified IP '{}'", ip_addr),
        ));
    }
    if ip_addr.is_multicast() {
        return Err(violation(
            "delivery_ssrf_blocked_multicast",
            format!("blocked outbound multicast IP '{}'", ip_addr),
        ));
    }
    if !allow_private_network && is_private_network_ip(ip_addr) {
        return Err(violation(
            "delivery_ssrf_blocked_private_network",
            format!("blocked outbound private or loopback IP '{}'", ip_addr),
        ));
    }
    Ok(())
}

fn is_metadata_ip(ip_addr: IpAddr) -> bool {
    matches!(ip_addr, IpAddr::V4(ipv4) if ipv4 == Ipv4Addr::new(169, 254, 169, 254))
}

fn is_private_network_ip(ip_addr: IpAddr) -> bool {
    match ip_addr {
        IpAddr::V4(ipv4) => {
            ipv4.is_private()
                || ipv4.is_loopback()
                || ipv4.is_link_local()
                || ipv4.is_broadcast()
                || is_ipv4_carrier_grade_nat(ipv4)
        }
        IpAddr::V6(ipv6) => {
            ipv6.is_loopback()
                || ipv6.is_unique_local()
                || is_ipv6_link_local(ipv6)
                || is_ipv6_documentation(ipv6)
        }
    }
}

fn is_ipv4_carrier_grade_nat(ipv4: Ipv4Addr) -> bool {
    let octets = ipv4.octets();
    octets[0] == 100 && (64..=127).contains(&octets[1])
}

fn is_ipv6_link_local(ipv6: Ipv6Addr) -> bool {
    (ipv6.segments()[0] & 0xffc0) == 0xfe80
}

fn is_ipv6_documentation(ipv6: Ipv6Addr) -> bool {
    ipv6.segments()[0] == 0x2001 && ipv6.segments()[1] == 0x0db8
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use super::{SsrfGuard, SsrfProtectionConfig};

    #[tokio::test]
    async fn unit_ssrf_guard_blocks_http_by_default() {
        let guard = SsrfGuard::new(SsrfProtectionConfig::default());
        let error = guard
            .parse_and_validate_url("http://93.184.216.34/v1/messages")
            .await
            .expect_err("http should fail");
        assert_eq!(error.reason_code, "delivery_ssrf_blocked_scheme");
    }

    #[tokio::test]
    async fn functional_ssrf_guard_allows_http_when_configured() {
        let guard = SsrfGuard::new(SsrfProtectionConfig {
            allow_http: true,
            ..SsrfProtectionConfig::default()
        });
        guard
            .parse_and_validate_url("http://93.184.216.34/v1/messages")
            .await
            .expect("http should pass when enabled");
    }

    #[tokio::test]
    async fn regression_ssrf_guard_blocks_private_ip_by_default() {
        let guard = SsrfGuard::new(SsrfProtectionConfig {
            allow_http: true,
            ..SsrfProtectionConfig::default()
        });
        let error = guard
            .parse_and_validate_url("http://10.0.0.10/path")
            .await
            .expect_err("private ip should fail");
        assert_eq!(error.reason_code, "delivery_ssrf_blocked_private_network");
    }

    #[tokio::test]
    async fn unit_ssrf_guard_allows_private_network_when_configured() {
        let guard = SsrfGuard::new(SsrfProtectionConfig {
            allow_http: true,
            allow_private_network: true,
            ..SsrfProtectionConfig::default()
        });
        guard
            .parse_and_validate_url("http://127.0.0.1:8787/health")
            .await
            .expect("loopback should pass when private network is allowed");
    }

    #[tokio::test]
    async fn regression_ssrf_guard_blocks_metadata_endpoint_even_when_private_allowed() {
        let guard = SsrfGuard::new(SsrfProtectionConfig {
            allow_http: true,
            allow_private_network: true,
            ..SsrfProtectionConfig::default()
        });
        let error = guard
            .parse_and_validate_url("http://169.254.169.254/latest/meta-data")
            .await
            .expect_err("metadata endpoint should always fail");
        assert_eq!(error.reason_code, "delivery_ssrf_blocked_metadata_endpoint");
    }

    #[tokio::test]
    async fn functional_ssrf_guard_blocks_localhost_hostname_without_private_override() {
        let guard = SsrfGuard::new(SsrfProtectionConfig {
            allow_http: true,
            ..SsrfProtectionConfig::default()
        });
        let error = guard
            .parse_and_validate_url("http://localhost:8787/health")
            .await
            .expect_err("localhost should be blocked");
        assert_eq!(error.reason_code, "delivery_ssrf_blocked_private_network");
    }

    #[test]
    fn unit_ssrf_guard_marks_metadata_ipv4() {
        let ip = IpAddr::V4(Ipv4Addr::new(169, 254, 169, 254));
        assert!(super::is_metadata_ip(ip));
    }
}
