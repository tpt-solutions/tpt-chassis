# Phase 6 — RTOS Integration

TPT Chassis targets two RTOS tracks in parallel (see `spec.txt` §2). The core
crate is `no_std` and `#![forbid(unsafe_code)]`, so each port only supplies the
backend traits already defined in the workspace.

| Concern     | Core abstraction                | RTOS binding                              |
| ----------- | ------------------------------- | ----------------------------------------- |
| Vehicle bus | `tpt_chassis_core::bus::VehicleBus` | CAN: FlexCAN/SOCKETCAN; Eth: SOME/IP; LIN: UART+LIN |
| Persistence | `tpt_chassis_core::ota::OtaStorage` | Flash partition (Zephyr) / A/B files (AGL) |
| Update auth | `tpt_chassis_core::ota::SignatureScheme` | Ed25519 (production), `DemoSigner` (tests) |
| Actuation   | `tpt_chassis_core::autonomy::AutonomyStack` | RTOS task feeding actuators            |

## Track A — Zephyr RTOS — DONE

- Crate: `zephyr/` (`tpt-chassis-zephyr`), builds for `thumbv7em-none-eabihf`.
- `zephyr/src/ota_storage.rs` — `OtaStorage` over flash (`flash-ota` feature)
  with a RAM fallback for CI/QEMU validation.
- `zephyr/src/ffi.rs` — the only `unsafe` surface; minimal `extern "C"` to the
  Zephyr flash API, wrapped by safe functions.
- Validation: `cargo build -p tpt-chassis-zephyr --target thumbv7em-none-eabihf`
  (CI `no_std` job) plus `cargo test -p tpt-chassis-zephyr --features std`
  (bus + OTA engine end-to-end through the Zephyr backend).

## Track B — Automotive-grade Linux (AGL) — DONE

- Crate: `agl/` (`tpt-chassis-agl`), `std` build path.
- `agl/src/ota_storage.rs` — `OtaStorage` over A/B image files under
  `/var/lib/tpt-chassis`.
- `agl/src/can_socket.rs` — `CanTransceiver` over SOCKETCAN (`socketcan`
  feature) with a loopback fallback for host/CI.
- Validation: `cargo test -p tpt-chassis-agl` exercises the OTA engine (stage →
  promote → commit, and reboot/recover) and the CAN loopback end-to-end.

Both tracks reuse the *identical* `no_std` engine logic from `tpt-chassis-core`;
only the backend traits differ. CI gates are added in `.github/workflows/ci.yml`
(`rtos-integration` job).

See `docs/phase-07-hardware.md`, `docs/phase-08-vehicle.md`,
`docs/safety-case.md`, `docs/phase-10-ecosystem.md`.
