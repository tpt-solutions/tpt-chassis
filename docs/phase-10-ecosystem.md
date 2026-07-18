# Phase 10 — Ecosystem & Market Readiness

Enable adoption by EV startups, autonomous stacks, and fleets (see `spec.txt` §5).

## Deliverables

- [x] Write integrator documentation for EV startups building on TPT Chassis
- [x] Write example integration guides for third-party autonomous driving stacks
- [x] Build fleet OTA rollout tooling/runbook for simultaneous multi-vehicle updates

## Quickstart

Everything below runs on a host with **no hardware** — the `tpt-chassis-sim`
crate provides in-memory CAN/LIN/SOME/IP networks. Run any example with:

```sh
cargo run -p tpt-chassis-core --example <name>
```

| Example | What it shows |
| --- | --- |
| [`hello_ecu`](../core/examples/hello_ecu.rs) | Bring up a `CanBus` over the simulated network, send + receive. |
| [`ota_update`](../core/examples/ota_update.rs) | OTA lifecycle (stage → promote → commit) + power-loss / bad-signature failure injection. |
| [`uds_diagnostics`](../core/examples/uds_diagnostics.rs) | A `UdsServer` answering session-control, read/write DID, security access, tester-present over sim CAN. |
| [`autonomy_lane_keep`](../core/examples/autonomy_lane_keep.rs) | A lane-keeping stack wrapped in an `Interlock` kill-switch; safe-stop asserted when disarmed. |
| [`custom_stack`](../core/examples/custom_stack.rs) | A from-scratch `impl AutonomyStack` skeleton to copy for your own stack. |

A copy-paste starter ECU crate (Cargo.toml + `src/main.rs` + `signals.toml`)
lives in [`../templates/ecu-crate`](../templates/ecu-crate); it builds and runs
against the simulator out of the box.

## EV startup integrator guide

1. Copy [`templates/ecu-crate`](../templates/ecu-crate) and point its
   `path =` deps at your TPT Chassis checkout (or use a `git`/`version` dep).
2. Implement `tpt_chassis_core::bus::VehicleBus` for your MCU's CAN/LIN/
   Ethernet peripherals. The simulator already implements the same
   `CanTransceiver` / `LinTransceiver` / `SomeIpTransceiver` traits, so your
   application code is identical between sim and hardware.
3. Use `tpt_chassis_core::autosar::DioDriver` for actuators and
   `tpt_chassis_core::uds::UdsServer` for diagnostics.
4. Ship updates via `tpt_chassis_core::ota::OtaEngine` with a real
   `SignatureScheme` (Ed25519) — never the test-only `DemoSigner`. See
   `core/examples/ota_update.rs`.

## Third-party autonomy stack integration

Implement `tpt_chassis_core::autonomy::AutonomyStack`. Feed sensor samples from
your perception pipeline via `ingest`; pull `ControlCommand`s with
`next_command` and forward to your actuators. Always wrap your stack in an
`Interlock` so a disarmed kill-switch forces a safe stop. The reference
`LaneKeepingStack` and `core/examples/custom_stack.rs` show the contract shape.

## Fleet OTA runbook

- Sign a package, stage to a canary cohort, `promote` + `commit` after a health
  check; on failure `OtaEngine::rollback` restores the prior slot.
- Roll out canary → regional → fleet waves; `OtaEngine::recover` on every boot
  guarantees no bricked nodes (interrupted updates promote or roll back
  deterministically).
- See `core/examples/ota_update.rs` for a runnable demonstration of the full
  lifecycle plus power-loss and corrupted-payload injection.
