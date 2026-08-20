use super::manifest::validate_installer_url;
use super::UpdateFailure;
use semver::Version;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use ureq::tls::{RootCerts, TlsConfig};
use ureq::Agent;

const MAX_REDIRECTS: usize = 5;
const NETWORK_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const NETWORK_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
const NETWORK_BODY_TIMEOUT: Duration = Duration::from_mins(2);
const USER_AGENT: &str = concat!("BarePDF/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy)]
pub(super) enum RequestTarget<'a> {
    Metadata,
    Installer(&'a str),
}

pub(super) fn new_agent() -> Agent {
    Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_connect(Some(NETWORK_CONNECT_TIMEOUT))
        .timeout_recv_response(Some(NETWORK_RESPONSE_TIMEOUT))
        .timeout_recv_body(Some(NETWORK_BODY_TIMEOUT))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .new_agent()
}

pub(super) fn get_with_redirects(
    agent: &Agent,
    initial_url: &str,
    target: RequestTarget<'_>,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<ureq::http::Response<ureq::Body>, UpdateFailure> {
    let mut url = initial_url.to_owned();
    let started = Instant::now();
    for redirect_count in 0..=MAX_REDIRECTS {
        check_cancelled(cancelled)?;
        let uri = validate_request_url(&url, target)?;
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or(UpdateFailure::Rejected("Update request timed out"))?;
        // `ureq::Agent::call` waits synchronously for response headers. Cancellation is
        // checked before this call and before each redirect, while existing timeouts bound
        // a header wait that cannot be force-interrupted by the atomic flag.
        let response = agent
            .get(uri)
            .header("User-Agent", USER_AGENT)
            .config()
            .timeout_global(Some(remaining))
            .build()
            .call()
            .map_err(|source| UpdateFailure::Transport {
                operation: "Update request failed",
                source,
            })?;
        check_cancelled(cancelled)?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let location = response
            .headers()
            .get(ureq::http::header::LOCATION)
            .map(|header| {
                header
                    .to_str()
                    .map_err(|_| UpdateFailure::Rejected("Update redirect Location is invalid"))
            })
            .transpose()?;
        let location =
            validate_redirect_target(response.status().as_u16(), redirect_count, location, target)?;
        url.clear();
        url.push_str(location);
    }
    Err(UpdateFailure::Rejected(
        "Update request exceeded the redirect limit",
    ))
}

fn validate_redirect_target<'a>(
    status: u16,
    redirect_count: usize,
    location: Option<&'a str>,
    target: RequestTarget<'_>,
) -> Result<&'a str, UpdateFailure> {
    if !matches!(status, 301 | 302 | 303 | 307 | 308) {
        return Err(UpdateFailure::Rejected(
            "Update request returned an unsupported redirect",
        ));
    }
    if redirect_count == MAX_REDIRECTS {
        return Err(UpdateFailure::Rejected(
            "Update request exceeded the redirect limit",
        ));
    }
    let location = location.ok_or(UpdateFailure::Rejected(
        "Update redirect is missing Location",
    ))?;
    validate_request_url(location, target)?;
    Ok(location)
}

pub(super) fn check_cancelled(cancelled: &AtomicBool) -> Result<(), UpdateFailure> {
    if cancelled.load(Ordering::Acquire) {
        Err(UpdateFailure::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn validate_request_url(
    url: &str,
    target: RequestTarget<'_>,
) -> Result<ureq::http::Uri, UpdateFailure> {
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|_| UpdateFailure::Rejected("Update URL is invalid"))?;
    if uri.scheme_str() != Some("https") || uri.port_u16().is_some() {
        return Err(UpdateFailure::Rejected(
            "Update request requires an approved HTTPS URL",
        ));
    }
    let approved = match uri.host() {
        Some("github.com") if uri.query().is_none() => match target {
            RequestTarget::Metadata => metadata_path_is_approved(uri.path()),
            RequestTarget::Installer(version) => validate_installer_url(url, version).is_ok(),
        },
        Some("release-assets.githubusercontent.com") => {
            uri.path().starts_with("/github-production-release-asset/")
        }
        _ => false,
    };
    if approved {
        Ok(uri)
    } else {
        Err(UpdateFailure::Rejected(
            "Update request URL is not approved",
        ))
    }
}

fn metadata_path_is_approved(path: &str) -> bool {
    if matches!(
        path,
        "/Woffluon/BarePDF/releases/latest/download/latest.json"
            | "/Woffluon/BarePDF/releases/latest/download/latest.json.sig"
    ) {
        return true;
    }
    let Some(remainder) = path.strip_prefix("/Woffluon/BarePDF/releases/download/v") else {
        return false;
    };
    let Some((version, filename)) = remainder.split_once('/') else {
        return false;
    };
    matches!(filename, "latest.json" | "latest.json.sig")
        && Version::parse(version).is_ok_and(|version| version.pre.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infrastructure::update::manifest::{METADATA_SIGNATURE_URL, METADATA_URL};

    #[test]
    fn requests_accept_only_approved_https_hosts_and_paths() {
        assert!(validate_request_url(METADATA_URL, RequestTarget::Metadata).is_ok());
        assert!(validate_request_url(METADATA_SIGNATURE_URL, RequestTarget::Metadata).is_ok());
        assert!(validate_request_url(
            "https://github.com/Woffluon/BarePDF/releases/download/v1.2.0/latest.json",
            RequestTarget::Metadata
        )
        .is_ok());
        assert!(validate_request_url(
            "https://release-assets.githubusercontent.com/github-production-release-asset/123/abc?token=signed",
            RequestTarget::Metadata
        )
        .is_ok());
        for url in [
            "http://github.com/Woffluon/BarePDF/releases/latest/download/latest.json",
            "https://example.com/Woffluon/BarePDF/releases/latest/download/latest.json",
            "https://github.com/Woffluon/Other/releases/latest/download/latest.json",
            "https://release-assets.githubusercontent.com/unapproved/file",
        ] {
            assert!(validate_request_url(url, RequestTarget::Metadata).is_err());
        }
    }

    #[test]
    fn every_redirect_hop_enforces_status_limit_location_and_allowlist() {
        let valid = "https://release-assets.githubusercontent.com/github-production-release-asset/123/file?token=signed";
        assert!(validate_redirect_target(302, 0, Some(valid), RequestTarget::Metadata).is_ok());
        assert!(validate_redirect_target(300, 0, Some(valid), RequestTarget::Metadata).is_err());
        assert!(
            validate_redirect_target(301, MAX_REDIRECTS, Some(valid), RequestTarget::Metadata)
                .is_err()
        );
        assert!(validate_redirect_target(301, 0, None, RequestTarget::Metadata).is_err());
        assert!(validate_redirect_target(
            301,
            0,
            Some("https://example.com/github-production-release-asset/123/file"),
            RequestTarget::Metadata,
        )
        .is_err());
        assert!(validate_redirect_target(
            301,
            0,
            Some("https://github.com/Woffluon/BarePDF/releases/latest/download/other.json"),
            RequestTarget::Metadata,
        )
        .is_err());
    }
}
