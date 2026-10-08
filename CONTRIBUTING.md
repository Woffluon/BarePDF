# Contributing to BarePDF

Thank you for your interest in contributing to BarePDF. This guide documents our architecture, single-version contract, strict safety standards, release signing / key rotation procedures, and required verification checks.

Before making any changes, please also read [`AGENTS.md`](./AGENTS.md) and [`docs/RELEASING.md`](./docs/RELEASING.md).

---

## 1. Repository Architecture

BarePDF is organized as a multi-crate Rust workspace paired with an Astro static website and deterministic Windows packaging scripts:

| Path | Responsibility |
| --- | --- |
| [`apps/barepdf`](./apps/barepdf) | Main Windows executable, CLI argument handling, opt-in diagnostics (`--log` / `BAREPDF_LOG` writing to `%LOCALAPPDATA%\BarePDF\logs\barepdf.log`), preference persistence, update orchestration, and Slint event-loop wiring. |
| [`crates/barepdf-core`](./crates/barepdf-core) | Engine-independent domain types, page layout geometry, text selection state, password zeroization (`SecretPassword`), and user preferences. |
| [`crates/barepdf-pdf`](./crates/barepdf-pdf) | Document traits and single-actor PDFium adapter (`pdfium-render` over `pdfium.dll`). |
| [`crates/barepdf-render`](./crates/barepdf-render) | Priority-based asynchronous render scheduler, request deduplication, generation-token cancellation, and byte-budgeted LRU bitmap caches. |
| [`crates/barepdf-ui`](./crates/barepdf-ui) | Slint UI definitions (`.slint`), toolbar, document viewport, sidebar thumbnails/outline, and modal dialogs. |
| [`crates/barepdf-platform`](./crates/barepdf-platform) | OS-agnostic platform service contracts. |
| [`crates/barepdf-platform-windows`](./crates/barepdf-platform-windows) | Win32 integration (native dialogs, clipboard, drag-and-drop, Default Apps registration, PE version inspection). |
| [`crates/barepdf-i18n`](./crates/barepdf-i18n) | Localization tables and language resolution (English, Turkish, System). |
| [`crates/barepdf-thumbnail`](./crates/barepdf-thumbnail) | Windows Explorer Shell `IThumbnailProvider` COM DLL (`BarePDF.Thumbnail.dll`). |
| [`packaging/windows`](./packaging/windows) | Inno Setup definition (`BarePDF.iss`) and PowerShell scripts for fetching PDFium, staging, building, signing, and validating releases. |
| [`website`](./website) | Astro website, user/developer documentation, and build-time GitHub Release data integration. |

### Core Design Principles

1. **Simplicity First (YAGNI)**: Do not introduce speculative abstractions, single-implementation traits, or unnecessary third-party crates when the Rust standard library or native Windows APIs suffice.
2. **Offline and Private by Default**: Reading PDFs must never trigger network requests. Update checks and diagnostic file logging (`--log` or `BAREPDF_LOG`) are strictly opt-in.
3. **Surgical Changes**: Keep pull requests and commits focused on a single coherent change.

---

## 2. Production Code Safety Rules (Zero `unwrap` / `expect` / `panic`)

BarePDF enforces strict compile-time safety across its production codebase:

- **No Panics in Production**: `apps/barepdf` and workspace crates enforce `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]` alongside `#![forbid(unsafe_code)]` (except where Win32/COM FFI is explicitly isolated).
- **Graceful Error Handling**: Never use `.unwrap()`, `.expect()`, `panic!()`, `todo!()`, or `unimplemented!()` in non-test code. Propagate errors with `Result<T, E>` and `thiserror`, or degrade gracefully while recording a redacted warning via `diagnostics::warn_redacted`.
- **COM FFI Boundary Protection**: In `crates/barepdf-thumbnail`, every COM entry point is wrapped in `std::panic::catch_unwind` so that unexpected failures translate into safe `HRESULT` error codes without unwinding across the C ABI boundary.
- **Redacted Diagnostics**: Never log raw file paths, URLs with query tokens, or user document contents. Always route diagnostic warnings through `diagnostics::warn_redacted`.

---

## 3. Conventional Commits & Single-Source Version Contract

`[workspace.package].version` in the root [`Cargo.toml`](./Cargo.toml) is the **single source of truth** for the BarePDF product version.

- All workspace crates inherit `version.workspace = true`.
- Windows PE `VERSIONINFO` resources (`build.rs`), the Inno Setup installer (`BarePDF.iss`), portable archives, Git tags, signed update manifests (`latest.json`), and the website derive their version from `Cargo.toml`.
- **Never hardcode the product version** in Rust, Slint, Astro, Inno Setup, documentation, or GitHub Actions workflows. (`website/package.json` is an internal build package and not a product version.)

### Commit Message & SemVer Mapping

Every commit on `main` must use a valid [Conventional Commit](https://www.conventionalcommits.org/) header, which deterministically controls the SemVer bump:

| Commit Type / Indicator | Version Bump | Triggers Release? |
| --- | :---: | :---: |
| `<type>!:` or `BREAKING CHANGE:` in body | **Major** (`X+1.0.0`) | Yes |
| `feat:` / `feat(scope):` | **Minor** (`X.Y+1.0`) | Yes |
| `fix:`, `perf:`, `refactor:`, `build:`, `security:` | **Patch** (`X.Y.Z+1`) | Yes |
| `docs:`, `ci:`, `test:`, `chore:` | **None** | No |

### Preparing and Validating a Commit

Before creating any commit, run the idempotent version preparation and validation scripts using the **exact commit message** you intend to commit with:

```powershell
$CommitMessage = "fix(scope): concise description of the change"

# 1. Compute and apply the required version bump to Cargo.toml and Cargo.lock
powershell -File scripts/prepare-version.ps1 -Message $CommitMessage

# 2. Validate workspace version inheritance, installer metadata bindings, and git history
powershell -File packaging/windows/scripts/validate-version.ps1 -Message $CommitMessage
```

- If your commit is `docs:`, `ci:`, `test:`, or `chore:`, `prepare-version.ps1` verifies that `Cargo.toml` remains unchanged.
- Never amend or force-push an already-published release commit; always roll forward with a new commit.

---

## 4. Ed25519 Update Manifest Signing & Key Rotation (Bus Factor Continuity)

Stable Windows releases publish an update manifest (`latest.json`) and a detached 64-byte Ed25519 signature (`latest.json.sig`). BarePDF verifies `latest.json.sig` against the 32-byte hexadecimal Ed25519 public key pinned in [`assets/update-public-key.hex`](./assets/update-public-key.hex) before parsing update metadata or downloading any installer.

### Multi-Maintainer Signing & Key Custody

- **Secret Storage**: The active Ed25519 private key (PKCS#8 PEM encoded as Base64) is stored in the repository's encrypted GitHub Actions secret `UPDATE_MANIFEST_PRIVATE_KEY_BASE64`. It must **never** be committed to the repository, printed in logs, or uploaded as an artifact.
- **Fail-Closed Releases**: `.github/workflows/release.yml` runs `packaging/windows/scripts/update-manifest-signature.ps1 -Action CheckKey` during preflight. If the secret is missing or its derived public key does not match `assets/update-public-key.hex`, the release fails closed immediately.
- **Shared Custody (Bus Factor)**: To prevent single-maintainer lock-in, at least two trusted repository administrators should hold offline encrypted backups of the active Ed25519 private key PEM (e.g., in an organization vault or hardware-backed password manager) and have admin access to rotate GitHub Actions secrets.

### Key Generation & Planned Key Rotation Procedure

If the signing key must be rotated (due to maintainer handoff, scheduled rotation, or key compromise):

1. **Generate a New Ed25519 Keypair (OpenSSL 3)**:
   Run the following in an isolated, secure local directory outside the repository:
   ```powershell
   # Generate a new Ed25519 private key in PKCS#8 PEM format
   openssl genpkey -algorithm Ed25519 -out barepdf-update-signing.pem

   # Extract the 32-byte raw public key in hexadecimal (last 32 bytes of the 44-byte SPKI DER)
   openssl pkey -in barepdf-update-signing.pem -pubout -outform DER -out barepdf-update-public.der
   $DerBytes = [System.IO.File]::ReadAllBytes("barepdf-update-public.der")
   $RawPubBytes = $DerBytes[12..43]
   $PubHex = -join ($RawPubBytes | ForEach-Object { $_.ToString("x2") })
   Write-Host "New Public Key Hex: $PubHex"

   # Encode the private key PEM as single-line Base64 for GitHub Actions Secrets
   $PrivBase64 = [Convert]::ToBase64String([System.IO.File]::ReadAllBytes("barepdf-update-signing.pem"))
   ```
2. **Transition Release (Pinning the New Public Key)**:
   Because existing installed clients verify `latest.json.sig` using the public key compiled into their binary (`assets/update-public-key.hex`):
   - **Planned Rotation**: Update `assets/update-public-key.hex` with `$PubHex` in a `security(updater): rotate Ed25519 update verification key` commit. Before publishing that transitional release, update `UPDATE_MANIFEST_PRIVATE_KEY_BASE64` in GitHub Actions Secrets to `$PrivBase64` so the release workflow signs the new release with the matching key. (Note: Clients on older keys will require a manual update across a hard single-key cutover unless a dual-key bridge release is shipped first.)
   - **Verify Locally**:
     ```powershell
     powershell -File packaging/windows/scripts/update-manifest-signature.ps1 `
       -Action CheckKey `
       -PrivateKeyPath .\barepdf-update-signing.pem
     ```
3. **Clean Up**: Securely delete the temporary `.pem` and `.der` files from disk after storing the backup in the shared maintainer vault and updating `UPDATE_MANIFEST_PRIVATE_KEY_BASE64`.

---

## 5. Required Validation Before Submitting a PR

Run all checks proportional to your change (and the full suite before any release-affecting commit):

```powershell
# 1. Rust formatting, strict Clippy lints, and workspace unit/integration tests
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked

# 2. Website tests, TypeScript/Astro type checks, and static build
pnpm --dir website run test
pnpm --dir website exec astro check
pnpm --dir website run build

# 3. Product version contract validation
powershell -File packaging/windows/scripts/validate-version.ps1 -Message "<exact full commit message>"
```

For security, dependency, or Windows packaging changes, also run:

```powershell
cargo audit --deny warnings
powershell -File scripts/test-versioning.ps1
```
