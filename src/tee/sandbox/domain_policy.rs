use crate::tee::sandbox::error::SandboxError;
use reqwest::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
struct AllowedDomainPattern {
    host: String,
    port: u16,
    wildcard_suffix: Option<String>,
}

impl AllowedDomainPattern {
    fn parse(raw: &str) -> Result<Self, SandboxError> {
        let candidate = raw.trim();
        if candidate.is_empty() {
            return Err(SandboxError::Other(
                "allowed_domains entry must not be empty".to_string(),
            ));
        }

        let (scheme, without_scheme) = candidate
            .split_once("://")
            .map(|(scheme, rest)| (Some(scheme.trim().to_ascii_lowercase()), rest))
            .unwrap_or((None, candidate));
        if without_scheme.contains('/') {
            return Err(SandboxError::Other(format!(
                "allowed_domains entry must not contain a path: {candidate}"
            )));
        }

        let (host, port) = match without_scheme.rsplit_once(':') {
            Some((host, port_text))
                if !host.is_empty() && port_text.chars().all(|ch| ch.is_ascii_digit()) =>
            {
                let port = port_text.parse::<u16>().map_err(|_| {
                    SandboxError::Other(format!(
                        "invalid port in allowed_domains entry: {candidate}"
                    ))
                })?;
                (host, port)
            }
            _ => (without_scheme, default_port_for_scheme(scheme.as_deref())),
        };

        let normalized_host = host.trim().to_ascii_lowercase();
        if normalized_host.is_empty() {
            return Err(SandboxError::Other(format!(
                "allowed_domains host must not be empty: {candidate}"
            )));
        }

        let wildcard_suffix = normalized_host
            .strip_prefix("*.")
            .map(|suffix| suffix.to_string());

        if let Some(suffix) = &wildcard_suffix {
            if suffix.is_empty() || suffix.contains('*') {
                return Err(SandboxError::Other(format!(
                    "invalid wildcard allowed_domains entry: {candidate}"
                )));
            }
        } else if normalized_host.contains('*') {
            return Err(SandboxError::Other(format!(
                "wildcards are only supported as a leading '*.' in allowed_domains: {candidate}"
            )));
        }

        Ok(Self {
            host: normalized_host,
            port,
            wildcard_suffix,
        })
    }

    fn matches(&self, host: &str, port: u16) -> bool {
        if self.port != port {
            return false;
        }

        let normalized_host = host.to_ascii_lowercase();
        if let Some(suffix) = &self.wildcard_suffix {
            normalized_host != *suffix && normalized_host.ends_with(&format!(".{suffix}"))
        } else {
            normalized_host == self.host
        }
    }
}

fn default_port_for_scheme(scheme: Option<&str>) -> u16 {
    match scheme {
        Some("http") | Some("ws") => 80,
        Some("https") | Some("wss") | None => 443,
        Some(_) => 443,
    }
}

pub fn validate_allowed_domains(entries: &[String]) -> Result<(), SandboxError> {
    for entry in entries {
        AllowedDomainPattern::parse(entry)?;
    }
    Ok(())
}

pub fn ensure_url_allowed(url: &Url, allowed_domains: &[String]) -> Result<(), SandboxError> {
    if allowed_domains.is_empty() {
        return Ok(());
    }

    let host = url
        .host_str()
        .ok_or_else(|| SandboxError::Other(format!("url is missing host: {url}")))?;
    let port = url
        .port_or_known_default()
        .ok_or_else(|| SandboxError::Other(format!("url is missing port: {url}")))?;

    let allowed = allowed_domains
        .iter()
        .map(|entry| AllowedDomainPattern::parse(entry))
        .collect::<Result<Vec<_>, _>>()?;

    if allowed.iter().any(|pattern| pattern.matches(host, port)) {
        return Ok(());
    }

    Err(SandboxError::Other(format!(
        "http_request blocked by allowed_domains policy: {host}:{port}"
    )))
}

#[cfg(test)]
mod tests {
    use super::{AllowedDomainPattern, ensure_url_allowed, validate_allowed_domains};
    use reqwest::Url;

    #[test]
    fn validates_allowed_domains_entries() {
        validate_allowed_domains(&[
            "*.okx.com:443".to_string(),
            "https://www.okx.com".to_string(),
            "wss://ws.okx.com".to_string(),
        ])
        .expect("entries should be valid");
    }

    #[test]
    fn wildcard_matching_requires_subdomain_boundary() {
        let pattern = AllowedDomainPattern::parse("*.okx.com:443").expect("pattern should parse");
        assert!(pattern.matches("www.okx.com", 443));
        assert!(!pattern.matches("okx.com", 443));
        assert!(!pattern.matches("evil-okx.com", 443));
        assert!(!pattern.matches("www.okx.com.evil.com", 443));
    }

    #[test]
    fn ensures_url_matches_allowed_domains() {
        let url = Url::parse("https://www.okx.com/api/v5/account/balance").unwrap();
        ensure_url_allowed(&url, &["*.okx.com:443".to_string()]).expect("url should be allowed");
    }

    #[test]
    fn defaults_http_and_ws_entries_to_port_80() {
        let http_pattern =
            AllowedDomainPattern::parse("http://localhost").expect("http pattern should parse");
        let ws_pattern =
            AllowedDomainPattern::parse("ws://feed.example.com").expect("ws pattern should parse");

        assert!(http_pattern.matches("localhost", 80));
        assert!(ws_pattern.matches("feed.example.com", 80));
        assert!(!http_pattern.matches("localhost", 443));
        assert!(!ws_pattern.matches("feed.example.com", 443));
    }

    #[test]
    fn allows_http_and_ws_urls_without_explicit_ports() {
        let http_url = Url::parse("http://localhost/api/health").unwrap();
        let ws_url = Url::parse("ws://feed.example.com/stream").unwrap();

        ensure_url_allowed(&http_url, &["http://localhost".to_string()])
            .expect("http url should be allowed");
        ensure_url_allowed(&ws_url, &["ws://feed.example.com".to_string()])
            .expect("ws url should be allowed");
    }

    #[test]
    fn rejects_disallowed_url() {
        let url = Url::parse("https://evil-okx.com/api/v5/account/balance").unwrap();
        let error = ensure_url_allowed(&url, &["*.okx.com:443".to_string()])
            .expect_err("url should be blocked");
        assert!(error.to_string().contains("allowed_domains policy"));
    }
}
