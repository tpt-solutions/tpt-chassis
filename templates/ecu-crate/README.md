# ECU Crate Starter Template

A copy-paste starting point for building an ECU application on top of
[TPT Chassis](../README.md).

## What's inside

- `Cargo.toml` — depends on `tpt-chassis-core` and (for host testing)
  `tpt-chassis-sim`.
- `src/main.rs` — a minimal `main()` that brings up a CAN bus over the
  simulator, decodes a lane-offset signal, and emits a steering command.
- `signals.toml` — a declarative signal database describing the CAN signals
  this ECU speaks (consumed by the planned `tpt-cli signals` codegen, and
  mirrored by the hand-written `Signal` layout in `main.rs`).

## Quick start

```sh
# from this directory
cargo run
```

## Next steps

1. Point the `path =` deps in `Cargo.toml` at your TPT Chassis checkout (or use
   a `git`/`version` dependency).
2. Implement `tpt_chassis_core::bus::VehicleBus` for your MCU's CAN/LIN/
   Ethernet peripherals. The simulator backend in `tpt-chassis-sim` already
   implements the same transceiver traits, so your application code is unchanged
   between sim and hardware.
3. Read the integrator guide in
   [`../docs/phase-10-ecosystem.md`](../docs/phase-10-ecosystem.md) for the full
   bring-up, diagnostics (`uds::UdsServer`), and OTA (`ota::OtaEngine`) flow.
