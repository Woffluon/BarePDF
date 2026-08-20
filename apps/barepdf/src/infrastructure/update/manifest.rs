use super::transport::{get_with_redirects, RequestTarget};
use super::{UpdateFailure, CURRENT_VERSION};
use ed25519_dalek::{Signature, VerifyingKey};
use semver::Version;
use serde::Deserialize;
use std::io::Read;
use std::sync::atomic::AtomicBool;
use std::time::Duration;
use ureq::Agent;

pub(crate) const METADATA_URL: &str =
    "https://github.com/Woffluon/BarePDF/releases/latest/download/latest.json";
pub(crate) const METADATA_SIGNATURE_URL: &str =
    "https://github.com/Woffluon/BarePDF/releases/latest/download/latest.json.sig";

const MAX_METADATA_BYTES: u64 = 64 * 1024;
// Read one extra byte, then enforce the inclusive application limit explicitly.
const METADATA_READ_LIMIT: u64 = MAX_METADATA_BYTES + 1;
// `ureq` probes for EOF at its configured limit, so its guard needs one additional byte.
const METADATA_UREQ_LIMIT: u64 = METADATA_READ_LIMIT + 1;
const MAX_SIGNATURE_BYTES: u64 = 64;
const SIGNATURE_READ_LIMIT: u64 = MAX_SIGNATURE_BYTES + 1;
const SIGNATURE_UREQ_LIMIT: u64 = SIGNATURE_READ_LIMIT + 1;
const BODY_READ_CHUNK_BYTES: usize = 8 * 1024;
const MAX_INSTALLER_BYTES: u64 = 256 * 1024 * 1024;
const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
const UPDATE_PUBLIC_KEY_HEX: &str = include_str!("../../../../../assets/update-public-key.hex");

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct VerifiedUpdate {
    version: String,
    release_url: String,
    release_notes: String,
    pub(super) installer_url: String,
    pub(super) installer_sha256: String,
    pub(super) installer_size: u64,
}

impl VerifiedUpdate {
    fn from_manifest(manifest: UpdateManifest, installer_sha256: String) -> Self {
        Self {
            version: manifest.version,
            release_url: manifest.release_url,
            release_notes: manifest.release_notes,
            installer_url: manifest.installer.url,
            installer_sha256,
            installer_size: manifest.installer.size,
        }
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn release_url(&self) -> &str {
        &self.release_url
    }

    pub(crate) fn release_notes(&self) -> &str {
        &self.release_notes
    }

    #[cfg(test)]
    pub(super) fn for_test(version: &str, installer_sha256: String, installer_size: u64) -> Self {
        Self {
            version: version.to_owned(),
            release_url: format!("https://github.com/Woffluon/BarePDF/releases/tag/v{version}"),
            release_notes: String::new(),
            installer_url: format!(
                "https://github.com/Woffluon/BarePDF/releases/download/v{version}/BarePDF-Setup-x64-v{version}.exe"
            ),
            installer_sha256,
            installer_size,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    schema_version: u32,
    version: String,
    release_url: String,
    #[serde(default)]
    release_notes: String,
    installer: InstallerManifest,
}

#[derive(Deserialize)]
struct InstallerManifest {
    url: String,
    sha256: String,
    size: u64,
}

pub(super) fn check_for_update(
    agent: &Agent,
    cancelled: &AtomicBool,
) -> Result<Option<VerifiedUpdate>, UpdateFailure> {
    let mut response = get_with_redirects(
        agent,
        METADATA_URL,
        RequestTarget::Metadata,
        METADATA_TIMEOUT,
        cancelled,
    )?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(METADATA_UREQ_LIMIT)
        .reader();
    let manifest = read_capped_body(
        &mut reader,
        METADATA_READ_LIMIT,
        cancelled,
        "Update metadata body could not be read",
    )?;
    validate_metadata_size(&manifest)?;
    super::transport::check_cancelled(cancelled)?;
    let mut signature_response = get_with_redirects(
        agent,
        METADATA_SIGNATURE_URL,
        RequestTarget::Metadata,
        METADATA_TIMEOUT,
        cancelled,
    )?;
    let mut reader = signature_response
        .body_mut()
        .with_config()
        .limit(SIGNATURE_UREQ_LIMIT)
        .reader();
    let signature = read_capped_body(
        &mut reader,
        SIGNATURE_READ_LIMIT,
        cancelled,
        "Update signature body could not be read",
    )?;
    super::transport::check_cancelled(cancelled)?;
    verify_then_parse_manifest(&manifest, &signature, CURRENT_VERSION)
}

fn read_capped_body<R: Read>(
    reader: &mut R,
    limit: u64,
    cancelled: &AtomicBool,
    operation: &'static str,
) -> Result<Vec<u8>, UpdateFailure> {
    let mut body = Vec::new();
    let mut buffer = [0_u8; BODY_READ_CHUNK_BYTES];
    loop {
        super::transport::check_cancelled(cancelled)?;
        let read_so_far = u64::try_from(body.len())
            .map_err(|_| UpdateFailure::Rejected("Update response body is too large"))?;
        if read_so_far >= limit {
            return Ok(body);
        }
        let remaining = limit - read_so_far;
        let chunk_limit = u64::try_from(BODY_READ_CHUNK_BYTES)
            .map_err(|_| UpdateFailure::Rejected("Update response body is too large"))?;
        let chunk_len = usize::try_from(remaining.min(chunk_limit))
            .map_err(|_| UpdateFailure::Rejected("Update response body is too large"))?;
        let read = reader
            .read(&mut buffer[..chunk_len])
            .map_err(|source| UpdateFailure::Io { operation, source })?;
        if read == 0 {
            return Ok(body);
        }
        body.extend_from_slice(&buffer[..read]);
    }
}

fn validate_metadata_size(manifest: &[u8]) -> Result<(), UpdateFailure> {
    let exceeds_limit = match u64::try_from(manifest.len()) {
        Ok(length) => length > MAX_METADATA_BYTES,
        Err(_) => true,
    };
    if exceeds_limit {
        return Err(UpdateFailure::Rejected(
            "Update metadata exceeds the allowed size",
        ));
    }
    Ok(())
}

fn verify_then_parse_manifest(
    manifest: &[u8],
    signature: &[u8],
    current_version: &str,
) -> Result<Option<VerifiedUpdate>, UpdateFailure> {
    verify_then_parse_manifest_with_key(manifest, signature, current_version, UPDATE_PUBLIC_KEY_HEX)
}

fn verify_then_parse_manifest_with_key(
    manifest: &[u8],
    signature: &[u8],
    current_version: &str,
    public_key: &str,
) -> Result<Option<VerifiedUpdate>, UpdateFailure> {
    verify_manifest_signature_with_key(manifest, signature, public_key)?;
    let json = std::str::from_utf8(manifest)
        .map_err(|_| UpdateFailure::Rejected("Update metadata is not valid UTF-8"))?;
    parse_manifest(json, current_version)
}

fn verify_manifest_signature_with_key(
    manifest: &[u8],
    signature: &[u8],
    public_key: &str,
) -> Result<(), UpdateFailure> {
    let public_key = decode_public_key(public_key)?;
    let verifying_key = VerifyingKey::from_bytes(&public_key)
        .map_err(|_| UpdateFailure::Rejected("Update verification key is invalid"))?;
    let signature = Signature::from_slice(signature)
        .map_err(|_| UpdateFailure::Rejected("Update metadata signature is invalid"))?;
    verifying_key
        .verify_strict(manifest, &signature)
        .map_err(|_| UpdateFailure::Rejected("Update metadata signature verification failed"))
}

fn decode_public_key(value: &str) -> Result<[u8; 32], UpdateFailure> {
    let value = value.trim().as_bytes();
    if value.len() != 64 {
        return Err(UpdateFailure::Rejected(
            "Update verification key is invalid",
        ));
    }
    let mut key = [0_u8; 32];
    for (index, byte) in key.iter_mut().enumerate() {
        *byte = (decode_hex(value[index * 2])? << 4) | decode_hex(value[index * 2 + 1])?;
    }
    Ok(key)
}

fn decode_hex(value: u8) -> Result<u8, UpdateFailure> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(UpdateFailure::Rejected(
            "Update verification key is invalid",
        )),
    }
}

pub(super) fn parse_manifest(
    json: &str,
    current_version: &str,
) -> Result<Option<VerifiedUpdate>, UpdateFailure> {
    let manifest: UpdateManifest =
        serde_json::from_str(json).map_err(|source| UpdateFailure::Manifest {
            operation: "Invalid update metadata",
            source,
        })?;
    if manifest.schema_version != 1 {
        return Err(UpdateFailure::Rejected(
            "Unsupported update metadata version",
        ));
    }
    let current = Version::parse(current_version).map_err(|source| UpdateFailure::Version {
        operation: "Invalid application version",
        source,
    })?;
    let available = Version::parse(&manifest.version).map_err(|source| UpdateFailure::Version {
        operation: "Invalid release version",
        source,
    })?;
    if !available.pre.is_empty() || !available.build.is_empty() {
        return Err(UpdateFailure::Rejected(
            "Preview releases are not accepted on the stable channel",
        ));
    }
    expected_file_version(&available)?;
    if available <= current {
        return Ok(None);
    }
    validate_release_url(&manifest.release_url, &manifest.version)?;
    validate_installer_url(&manifest.installer.url, &manifest.version)?;
    let hash = manifest.installer.sha256.to_ascii_lowercase();
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(UpdateFailure::Rejected("Invalid installer SHA-256"));
    }
    if manifest.installer.size == 0 || manifest.installer.size > MAX_INSTALLER_BYTES {
        return Err(UpdateFailure::Rejected(
            "Installer size is outside the allowed range",
        ));
    }
    Ok(Some(VerifiedUpdate::from_manifest(manifest, hash)))
}

pub(super) fn validate_installer_url(url: &str, version: &str) -> Result<(), UpdateFailure> {
    let expected =
        format!("/Woffluon/BarePDF/releases/download/v{version}/BarePDF-Setup-x64-v{version}.exe");
    validate_github_url(url, &expected)
}

fn validate_release_url(url: &str, version: &str) -> Result<(), UpdateFailure> {
    validate_github_url(url, &format!("/Woffluon/BarePDF/releases/tag/v{version}"))
}

fn validate_github_url(url: &str, expected_path: &str) -> Result<(), UpdateFailure> {
    let uri: ureq::http::Uri = url
        .parse()
        .map_err(|_| UpdateFailure::Rejected("Invalid HTTPS URL"))?;
    if uri.scheme_str() != Some("https")
        || uri.host() != Some("github.com")
        || uri.port_u16().is_some()
        || uri.query().is_some()
        || uri.path() != expected_path
    {
        return Err(UpdateFailure::Rejected(
            "Update URL is not an approved BarePDF release URL",
        ));
    }
    Ok(())
}

pub(super) fn expected_file_version(version: &Version) -> Result<[u16; 4], UpdateFailure> {
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(UpdateFailure::Rejected(
            "Windows installer version must be a stable semantic version",
        ));
    }
    Ok([
        u16::try_from(version.major)
            .map_err(|_| UpdateFailure::Rejected("Release major version exceeds Windows limits"))?,
        u16::try_from(version.minor)
            .map_err(|_| UpdateFailure::Rejected("Release minor version exceeds Windows limits"))?,
        u16::try_from(version.patch)
            .map_err(|_| UpdateFailure::Rejected("Release patch version exceeds Windows limits"))?,
        0,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::fmt::Write as _;
    use std::io;
    use std::sync::atomic::Ordering;

    struct CancelAfterFirstChunk<'a> {
        cancelled: &'a AtomicBool,
        reads: u8,
    }

    impl Read for CancelAfterFirstChunk<'_> {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.reads == 0 {
                buffer[0] = b'x';
                self.reads = 1;
                self.cancelled.store(true, Ordering::Release);
                return Ok(1);
            }
            panic!("a cancelled body read must not request another chunk");
        }
    }

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("simulated body read failure"))
        }
    }

    fn manifest(version: &str, url_version: &str, hash: &str, size: u64) -> String {
        format!(
            r#"{{"schemaVersion":1,"version":"{version}","releaseUrl":"https://github.com/Woffluon/BarePDF/releases/tag/v{url_version}","installer":{{"url":"https://github.com/Woffluon/BarePDF/releases/download/v{url_version}/BarePDF-Setup-x64-v{url_version}.exe","sha256":"{hash}","size":{size}}}}}"#
        )
    }

    #[test]
    fn accepts_only_newer_matching_stable_release() {
        let hash = "a".repeat(64);
        let update = parse_manifest(&manifest("1.2.0", "1.2.0", &hash, 1024), "1.1.0");
        assert!(matches!(update, Ok(Some(ref update)) if update.version() == "1.2.0"));
        assert!(matches!(
            parse_manifest(&manifest("1.1.0", "1.1.0", &hash, 1024), "1.1.0"),
            Ok(None)
        ));
        assert!(parse_manifest(
            &manifest("1.2.0-beta.1", "1.2.0-beta.1", &hash, 1024),
            "1.1.0"
        )
        .is_err());
    }

    #[test]
    fn metadata_size_limit_is_inclusive() {
        let limit = usize::try_from(MAX_METADATA_BYTES).expect("metadata limit fits usize");
        for length in [limit - 1, limit] {
            assert!(validate_metadata_size(&vec![0_u8; length]).is_ok());
        }
        assert!(matches!(
            validate_metadata_size(&vec![0_u8; limit + 1]),
            Err(UpdateFailure::Rejected(
                "Update metadata exceeds the allowed size"
            ))
        ));
    }

    #[test]
    fn capped_reader_retains_one_byte_for_metadata_overflow_detection() {
        let limit = usize::try_from(METADATA_READ_LIMIT).expect("metadata read limit fits usize");
        let cancelled = AtomicBool::new(false);
        let mut reader = io::Cursor::new(vec![0_u8; limit + 1]);
        let body = read_capped_body(
            &mut reader,
            METADATA_READ_LIMIT,
            &cancelled,
            "test body read",
        )
        .expect("capped read");

        assert_eq!(body.len(), limit);
        assert!(validate_metadata_size(&body).is_err());
    }

    #[test]
    fn cancelling_stops_before_the_next_metadata_body_chunk() {
        let cancelled = AtomicBool::new(false);
        let mut reader = CancelAfterFirstChunk {
            cancelled: &cancelled,
            reads: 0,
        };

        assert!(matches!(
            read_capped_body(&mut reader, 2, &cancelled, "test body read"),
            Err(UpdateFailure::Cancelled)
        ));
    }

    #[test]
    fn body_read_errors_keep_the_io_source() {
        let cancelled = AtomicBool::new(false);
        let error = read_capped_body(&mut FailingReader, 1, &cancelled, "test body read")
            .expect_err("failing reader must surface an error");

        assert!(matches!(&error, UpdateFailure::Io { .. }));
        assert!(std::error::Error::source(&error).is_some());
    }

    #[test]
    fn signature_is_verified_before_manifest_parsing() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let mut public_key = String::with_capacity(64);
        for byte in signing_key.verifying_key().to_bytes() {
            assert!(write!(public_key, "{byte:02x}").is_ok());
        }
        let manifest = b"signed update metadata";
        let signature = signing_key.sign(manifest).to_bytes();
        assert!(verify_manifest_signature_with_key(manifest, &signature, &public_key).is_ok());
        assert!(verify_manifest_signature_with_key(b"tampered", &signature, &public_key).is_err());
        assert!(
            verify_manifest_signature_with_key(manifest, &signature[..63], &public_key).is_err()
        );
        let oversized_signature = [0_u8; 65];
        assert!(
            verify_manifest_signature_with_key(manifest, &oversized_signature, &public_key)
                .is_err()
        );
    }

    #[test]
    fn invalid_signature_rejects_malformed_json_before_parsing() {
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let mut public_key = String::with_capacity(64);
        for byte in signing_key.verifying_key().to_bytes() {
            assert!(write!(public_key, "{byte:02x}").is_ok());
        }
        let malformed_json = b"{not-json";
        let unrelated_signature = signing_key.sign(b"different metadata").to_bytes();

        assert!(matches!(
            verify_then_parse_manifest_with_key(
                malformed_json,
                &unrelated_signature,
                "1.0.0",
                &public_key,
            ),
            Err(UpdateFailure::Rejected(
                "Update metadata signature verification failed"
            ))
        ));
    }

    #[test]
    fn rejects_mismatched_urls_hashes_and_sizes() {
        let hash = "a".repeat(64);
        assert!(parse_manifest(&manifest("1.2.0", "1.3.0", &hash, 1024), "1.1.0").is_err());
        assert!(parse_manifest(&manifest("1.2.0", "1.2.0", "bad", 1024), "1.1.0").is_err());
        assert!(parse_manifest(&manifest("1.2.0", "1.2.0", &hash, 0), "1.1.0").is_err());
        assert!(parse_manifest(
            &manifest("1.2.0", "1.2.0", &hash, MAX_INSTALLER_BYTES + 1),
            "1.1.0"
        )
        .is_err());
    }
}
