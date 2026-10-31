# BarePDF Agent Briefing

<project_context>
BarePDF is a lean, offline-first desktop PDF reader and editor written in Rust with Slint UI, targeting Windows and Linux (Debian/Pardus). A companion Astro website lives under website/. You are pair programming with the user to inspect, modify, test, and package this codebase.
</project_context>

<engineering_principles>
1. Think before implementing: Formulate verifiable assumptions and inspect repository evidence first. Clarify requirements with the user only when repository facts leave material ambiguity, keeping interruptions minimal.
2. Surgical and minimal changes: Modify only the files and lines necessary for the assigned task. Preserve untouched code, existing formatting, and uncommitted user edits to keep review diffs clean and prevent merge friction.
3. Reuse before inventing: Leverage the Rust standard library, platform APIs (Windows and Linux), and existing workspace crates before introducing new abstractions, dependencies, or configuration layers.
4. Clean your own artifacts: Remove unused imports, variables, and temporary helpers introduced by your changes. Document pre-existing technical debt separately rather than refactoring it without request.
</engineering_principles>

<repository_layout>
- apps/ and crates/: Rust workspace source (including barepdf-platform-windows and barepdf-platform-linux).
- website/: Astro website and documentation.
- packaging/windows/: Inno Setup scripts and Windows staging helpers.
- packaging/linux/: Desktop entry, icons, and cargo-deb configuration for Debian/Pardus.
- .github/workflows/: CI pipelines and automated release workflows.
- Search tools: Use rg and rg --files for text queries. When .codegraph/ is present, use CodeGraph for call-path and symbol analysis before broad text scans.
</repository_layout>

<versioning_and_git>
1. Single version authority: [workspace.package].version in root Cargo.toml governs all crate versions, UI strings, Windows metadata, installers, and website release references to keep artifacts in sync without drift.
2. Synchronize before committing: Run the version preparation script with your planned commit message before creating a commit:
   powershell -File scripts/prepare-version.ps1 -Message "<commit message>"
   This script increments the version based on Conventional Commit semantics:
   - Major: Header containing '!' or body with 'BREAKING CHANGE:'
   - Minor: 'feat'
   - Patch: 'fix', 'perf', 'refactor', 'build', 'security'
   - No bump: 'docs', 'ci', 'test', 'chore'
3. Atomic commits: Use the identical Conventional Commit subject in both prepare-version.ps1 and git commit. Stage only files relevant to the change, leaving uncommitted user work untouched. Every commit on main is an independently releasable build candidate.
</versioning_and_git>

<verification>
Run checks proportional to the change and inspect actual command output before declaring completion:

Rust workspace:
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked

Website:
pnpm --dir website run test
pnpm --dir website exec astro check
pnpm --dir website run build

Release readiness:
powershell -File packaging/windows/scripts/validate-version.ps1 -Message "<exact full commit message>"

Report check statuses as PASS, FAIL, or NOT EXECUTED based on actual execution. Resolve failures by correcting code behavior rather than weakening assertions or linters.
</verification>

<security_and_updater>
1. Offline-first privacy: Update checks remain disabled until the user explicitly opts in. Never trigger background network calls without explicit consent.
2. Verified updates: On Windows, updater payloads require valid Ed25519 manifest signatures pinned in the application, along with matching SHA-256 hashes and immutable release URLs, to guard against supply-chain tampering.
3. Non-destructive Linux updates: Linux packages must rely on system package managers (apt, Pardus Paket Kurucu) or user-space AppImage updates. Never invoke sudo/pkexec or write directly into /usr/bin or /usr/lib.
4. Secret isolation: Signing keys and credentials belong exclusively in encrypted GitHub Actions secrets. Never print, stage, or commit secret values.
</security_and_updater>

<delegation_and_completion>
1. Bounded subagents: Assign subagents focused, non-overlapping tasks. Require subagents to report changed files, executed checks, and any unresolved assumptions.
2. Completion criteria: Mark a task complete only after the requested behavior is verified, relevant tests pass, and the final git diff has been reviewed for cleanliness.
</delegation_and_completion>
