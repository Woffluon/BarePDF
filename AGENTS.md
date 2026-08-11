# BarePDF Agent Instructions

This file applies to the entire repository. Every AI agent and delegated subagent MUST read this file completely before inspecting, editing, testing, committing, or pushing repository changes. A delegating agent MUST include that requirement in every subagent task.

## 1. Think Before Coding

- Do not assume silently. State material assumptions and surface tradeoffs.
- Resolve discoverable facts from the repository before asking the user.
- If requirements have multiple materially different interpretations, stop and clarify.
- Prefer the smallest design that fully meets the requested behavior.
- Define verifiable success criteria before editing.

## 2. Keep Changes Simple and Surgical

- Touch only files and lines required by the task.
- Do not refactor, reformat, rename, or delete adjacent code without a direct need.
- Reuse existing helpers, standard-library facilities, native Windows APIs, and installed dependencies before adding new abstractions or packages.
- Do not create interfaces, factories, configuration layers, or extension points for a single implementation.
- Remove imports, variables, and helpers made unused by your own change. Leave unrelated existing debt alone and report it separately.
- Preserve user-authored changes in a dirty worktree. Never use destructive Git recovery commands unless explicitly authorized.

## 3. Repository Orientation

- Rust workspace source lives under `apps/` and `crates/`.
- The Astro website lives under `website/`.
- Windows packaging lives under `packaging/windows/`.
- CI and deployment workflows live under `.github/workflows/`.
- If `.codegraph/` exists, use CodeGraph before text search for code-flow questions. Do not create an index unless asked.
- Use `rg` and `rg --files` for normal repository search.

## 4. Product Version Is a Contract

- `[workspace.package].version` in the root `Cargo.toml` is the single source of truth for the BarePDF product version.
- Workspace crates inherit that version. Runtime UI, Windows file metadata, installer metadata, artifact names, release tags, release manifests, and the public website MUST derive from it.
- Do not hardcode the product version in Rust, Slint, Astro, Inno Setup, documentation, or workflows.
- `website/package.json` describes a private build package and is not a product-version source.
- Before creating a commit, run:

  ```powershell
  powershell -File scripts/prepare-version.ps1 -Message "<exact commit message>"
  ```

- The helper is idempotent. If the working tree already contains the expected bump it must do nothing; a conflicting version must fail.
- Version policy:
  - `!` in the Conventional Commit header or `BREAKING CHANGE:` in the body: major.
  - `feat`: minor.
  - `fix`, `perf`, `refactor`, `build`, `security`: patch.
  - `docs`, `ci`, `test`, `chore`: no product-version change.
- Invalid Conventional Commit messages, missing bumps, extra bumps, version regressions, and reused release versions are errors.
- Do not amend or rewrite a pushed release commit. Correct it with a new commit.

## 5. Commit and Push Rules

- Use a valid Conventional Commit subject that describes one coherent change.
- Use the same exact commit message when preparing the version and creating the commit.
- Inspect `git diff`, `git diff --cached`, and `git status` before committing.
- Stage explicit paths. Do not stage unrelated user changes.
- Do not create empty commits, force-push, or bypass hooks/checks.
- A code-changing `main` commit is expected to become a release. Keep it buildable, testable, and independently releasable.
- Documentation/CI/test/chore commits do not create releases and must not change the product version.

## 6. Required Validation

Run checks proportional to the change, and run the full relevant set before a release-affecting commit:

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
pnpm --dir website run test
pnpm --dir website exec astro check
pnpm --dir website run build
powershell -File packaging/windows/scripts/validate-version.ps1 -Message "<exact full commit message>"
```

- Security or packaging changes also require RustSec audit and Windows staging/package validation.
- Never fix a failing check by weakening or deleting the check without explicit user approval.
- Add the smallest runnable regression test for non-trivial parsing, branching, security, or versioning logic.

## 7. Release and Supply-Chain Security

- Stable Windows releases MUST include an Ed25519-signed update manifest. Missing signing material must fail closed. Authenticode is optional and Windows may show an unknown-publisher warning.
- Never print, persist in the repository, commit, publish as an artifact, or echo passwords, tokens, signing keys, or secret values. Release signing keys belong only in the repository's encrypted Actions secrets.
- Third-party native binaries must come from pinned HTTPS sources and pass the repository checksum allowlist before staging.
- GitHub Actions must use least-privilege permissions and pin third-party actions by full commit SHA.
- Release publication must be idempotent: an existing tag is accepted only when it resolves to the same commit; conflicting tags/releases fail.
- Never overwrite assets on an older stable release. Every product version gets a new immutable tag and release.
- Publish exactly one versioned installer, one versioned portable archive, one versioned checksum manifest, `latest.json`, and `latest.json.sig` per release. Do not publish duplicate alias copies.
- Hash checks do not replace signature verification. Update installers require a valid manifest signature from the public key pinned in the application, exact SHA-256/size/version matching, and an approved immutable release URL.

## 8. Updater and Privacy Rules

- BarePDF remains offline by default until the user explicitly chooses whether to enable update checks.
- Rejecting update checks means no background update network traffic.
- Network and hashing work must not block the UI thread.
- Only HTTPS GitHub/BarePDF release endpoints may supply update metadata or installers.
- Never install silently. Download, verify, present the release, and require an explicit install action.
- Never permit downgrade, same-version reinstall, untrusted redirects, hash mismatch, invalid signature, unexpected signer, oversized payload, or partial-file execution.
- Portable builds notify and link to the release; they do not self-replace.
- Update failures are non-fatal and must leave the current installation usable.

## 9. Website and Documentation Data

- The website must show the latest stable GitHub Release and default-branch commits from current build-time GitHub data.
- Build-time API failure may use current build metadata, but never a stale committed list of releases or commits.
- Public documentation must link to the website download page. That page resolves the latest release's canonical versioned assets from current build-time GitHub data.
- A release workflow must explicitly dispatch Pages deployment after publishing; do not rely on token-generated release events to trigger another workflow.

## 10. Delegation and Completion

- Subagents must receive a bounded, non-overlapping task and must read this file first.
- Subagents must report changed files, checks run, failures, and assumptions.
- The primary agent owns integration, conflict resolution, full validation, commit creation, and push verification.
- A task is complete only when requested behavior is implemented, relevant checks pass, the final diff is reviewed, and any external prerequisite is clearly reported.
