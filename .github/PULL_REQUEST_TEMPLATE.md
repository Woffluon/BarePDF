## Description

Please describe the changes in this pull request and the rationale behind them.
If this resolves an open issue, link it below (e.g. `Fixes #123`).

## Type of Change

Select all that apply:
- [ ] `feat`: A new user-facing feature or enhancement (triggers minor version bump)
- [ ] `fix`: A bug fix (triggers patch version bump)
- [ ] `perf`: A performance improvement (triggers patch version bump)
- [ ] `refactor`: Code refactoring without behavior change (triggers patch version bump)
- [ ] `docs`: Documentation updates only (no version bump)
- [ ] `test`: Adding or correcting tests (no version bump)
- [ ] `ci`: CI workflows or automation updates (no version bump)
- [ ] `chore`: Maintenance tasks or dependency updates (no version bump)
- [ ] `BREAKING CHANGE`: Breaking change marked with `!` in commit title or footer (triggers major version bump)

## Conventional Commit Check

- [ ] The PR title and commit messages adhere to Conventional Commits: `<type>(<scope>): <summary>`.
- [ ] The commit subject line is lowercase, imperative, and under 72 characters.
- [ ] The commit describes one cohesive change without mixing unrelated modifications.

## AGENTS.md & Repository Checklist

- [ ] Read and followed rules in `AGENTS.md`.
- [ ] Did not manually bump versions in code or workflows; versioning derives strictly from `[workspace.package].version` in root `Cargo.toml`.
- [ ] Zero telemetry preserved: no outbound analytics, phone-home metrics, or unsolicited network calls.
- [ ] Memory bounds respected: no unbounded cache growth or continuous bitmap retention.
- [ ] Thread safety respected: PDFium document calls remain isolated on the background actor thread.
- [ ] Tested locally with:
  ```powershell
  cargo fmt --all --check
  cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
  cargo test --workspace --all-features --locked
  ```
- [ ] If changing website code, tested locally with:
  ```powershell
  pnpm --dir website run test
  pnpm --dir website exec astro check
  pnpm --dir website run build
  ```

## Verification & Manual Testing

Detail the manual and automated steps taken to test these changes on Windows:
1. ...
2. ...
