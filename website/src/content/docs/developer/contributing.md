---
title: "Contributing Guide"
description: "Guidelines for contributing bug fixes, feature improvements, and code reviews to BarePDF."
category: "developer"
order: 8
---

# Contributing Guide

Thank you for your interest in contributing to BarePDF! We welcome community contributions, bug reports, and documentation improvements.

## Contribution Workflow

1. **Fork the Repository**: Create your personal fork of [Woffluon/BarePDF](https://github.com/Woffluon/BarePDF).
2. **Create a Feature Branch**:
   ```bash
   git checkout -b feature/my-new-feature
   ```
3. **Make Surgical Changes**: Focus strictly on the requested feature or fix. Keep pull requests concise and well-tested.
4. **Run Verification Suite**:
   ```powershell
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
   cargo test --workspace --all-features --locked
   ```
5. **Commit & Push**: Follow standard commit conventions:
   ```bash
   git commit -m "feat(ui): add shortcut for sidebar toggle"
   git push origin feature/my-new-feature
   ```
6. **Open a Pull Request**: Submit your PR against the `main` branch.

## Project Guidelines

- **No AI / Telemetry Clutter**: We maintain a strict zero-telemetry, zero-cloud, and zero-ad philosophy.
- **Memory Boundaries**: Any new features must respect low idle memory overhead (<60MB).
- **Code Style**: Rust code must adhere to `rustfmt` formatting and Clippy warnings guidelines.
