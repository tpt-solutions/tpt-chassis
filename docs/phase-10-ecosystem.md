# Phase 10 — Ecosystem & Market Readiness

Enable adoption by EV startups, autonomous stacks, and fleets (see `spec.txt` §5).

## Deliverables

- [ ] Write integrator documentation for EV startups building on TPT Chassis
- [ ] Write example integration guides for third-party autonomous driving stacks
- [ ] Build fleet OTA rollout tooling/runbook for simultaneous multi-vehicle updates

## EV startup integrator guide (outline)

1. Add `tpt-chassis-core` to your `Cargo.toml`.
2. Implement `VehicleBus` for your MCU's CAN/LIN/Ethernet peripherals.
3. Use `tpt_chassis_core::autosar::DioDriver` for actuators and
   `tpt_chassis_core::uds::UdsServer` for diagnostics.
4. Ship updates via `tpt_chassis_core::ota::OtaEngine` with a real
   `SignatureScheme` (Ed25519) — never the test-only `DemoSigner`.

## Third-party autonomy stack integration

Implement `tpt_chassis_core::autonomy::AutonomyStack`. Feed sensor samples from
your perception pipeline; pull `ControlCommand`s and forward to your actuators.
The reference `LaneKeepingStack` shows the contract shape.

## Fleet OTA runbook (outline)

- Sign a package, stage to a canary cohort, `promote` + `commit` after health
  check; on failure `OtaEngine::rollback` restores the prior slot.
- Roll out canary → regional → fleet waves; `OtaEngine::recover` on every boot
  guarantees no bricked nodes.
