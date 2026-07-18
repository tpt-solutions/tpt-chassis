# Contributing to TPT Chassis

Thanks for your interest in contributing! TPT Chassis is an open-source,
memory-safe vehicle operating system, and we welcome contributions of all
kinds: code, documentation, hardware bring-up, and design feedback.

## License

By contributing, you agree that your contributions will be dual licensed under
the [MIT](LICENSE-MIT) and [Apache-2.0](LICENSE-APACHE) licenses, at the
option of the project, without any additional terms or conditions.

## Getting started

1. Fork and clone the repository.
2. Install a recent Rust toolchain (see `rust-toolchain`/`Cargo.toml`
   `rust-version`).
3. Build and test the workspace:

   ```sh
   cargo build
   cargo test
   cargo clippy --all-targets -- -D warnings
   ```

## Code conventions

- **Memory safety first.** The `tpt-chassis-core` crate is `#![forbid(unsafe_code)]`
  and `#![no_std]`. Unsafe code is only acceptable with explicit, documented
  justification and a reviewed `unsafe` block — open an issue to discuss before
  introducing any.
- **License headers.** Every new source file must begin with the SPDX header
  used throughout the repo:

  ```rust
  // SPDX-License-Identifier: MIT OR Apache-2.0
  //
  // Copyright (c) TPT Solutions. All rights reserved.
  ```

  `Cargo.toml` files carry `license = "MIT OR Apache-2.0" # SPDX-License-Identifier: MIT OR Apache-2.0`.
- **Documentation.** Public items should have `///` docs. The core crate
  denies `missing_docs`.
- **Formatting.** Run `cargo fmt --all` before submitting.
- **Commits.** Keep commits focused and write clear, imperative commit
  messages. Reference the relevant phase from `todo.md` where applicable.

## Running CI locally

The GitHub Actions CI (`.github/workflows/ci.yml`) runs formatting, clippy with
deny-warnings, and tests across the workspace. Please ensure all three pass
before opening a pull request.

## Reporting issues

Use the issue templates under `.github/ISSUE_TEMPLATE/`. For safety-critical
or security-sensitive reports, please follow the security policy and avoid
public disclosure until triaged.

## Code of conduct

Be respectful and constructive. We are building safety-critical software where
clarity and correctness matter.
