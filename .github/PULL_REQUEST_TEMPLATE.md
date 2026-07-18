name: Pull request
description: Template for submitting a pull request to TPT Chassis
title: "[PR] "
labels: []
body:
  - type: textarea
    id: summary
    attributes:
      label: Summary
      description: What does this PR change and why?
    validations:
      required: true
  - type: checkboxes
    id: checks
    attributes:
      label: Pre-submission checklist
      options:
        - label: "cargo fmt --all -- --check passes"
        - label: "cargo clippy --all-targets --all-features -- -D warnings passes"
        - label: "cargo test --all-features passes"
        - label: "New source files carry the SPDX MIT OR Apache-2.0 header"
        - label: "Public items are documented"
  - type: textarea
    id: phase
    attributes:
      label: Related phase / issue
      description: Link the roadmap phase (todo.md) or issue this PR addresses.
    validations:
      required: false
