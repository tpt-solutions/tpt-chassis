# Phase 8 — Real Vehicle Integration

Run on an ECU in a test vehicle (see `spec.txt` §3 Phase 3). The code-level
enablers (`safety::Interlock`, `safety::TelemetryRing`, `OtaEngine::recover`)
ship in `core/src/safety.rs` and `core/src/ota.rs`; the procedures below are the
operational runbook.

## 8.1 Safety interlocks (implemented)

- **Hardware kill-switch**: a physical e-stop wired to cut actuator authority at
  the actuator driver, independent of software. Software MUST also call
  [`Interlock::disarm`](crate::safety::Interlock::disarm) on any fault.
- **Bench-to-vehicle gate**: actuator authority is granted **only** when (a) the
  running image is signed and (b) the interlock is explicitly armed by a
  supervisor. [`Interlock::has_control`] is the single source of truth.
- `core/src/safety.rs` `Interlock` is fail-safe: it starts `Disarmed` and any
  disarm suppresses *all* control output, even if the inner stack reports
  control.

## 8.2 In-vehicle test plan

1. **Environment**: closed, geofenced test area; vehicle speed-limited to 20 km/h
   by a calibration DID enforced through UDS (`core/src/uds.rs`).
2. **Crew**: driver with mechanical override, safety supervisor at the e-stop,
   and a data logger reading the `TelemetryRing` offload.
3. **Script**:
   - Arm interlock → confirm `has_control == true`.
   - Run `LaneKeepingStack` (or the target stack) at low speed; log frames.
   - Mid-run, trip the e-stop → confirm `has_control == false` within one tick
     and vehicle coasts to safe stop.
   - Re-arm and continue; exercise one OTA update + `recover` on reboot.
4. **Pass criteria**: no uncontrolled actuator output; telemetry reconstructs the
   e-stop event; OTA never leaves the ECU unbootable.

## 8.3 Telemetry / field logging (implemented)

[`TelemetryRing`](crate::safety::TelemetryRing) is a fixed-capacity, allocation-
free ring buffer. Field code pushes `(timestamp, code, value)` records (e.g.
speed, active slot, error code); the buffer is periodically offloaded over the
diagnostic link. Oldest records are overwritten once full, so it runs
indefinitely.

## 8.4 Incident response & rollback (procedure)

1. On any anomaly, supervisor trips the e-stop → `Interlock::disarm`.
2. For a suspected-bad image, reboot: `OtaEngine::recover` on boot detects a
   `Pending`/`Testing` state and either promotes or rolls back to the last known
   good slot (`core/src/ota.rs`), guaranteeing a bootable ECU.
3. Open a finding per `docs/safety-case.md` §9.5; attach the offloaded telemetry.

## 8.5 Supervised vehicle test

Requires physical hardware (Phase 7). Tracked as a checklist item; not
executable in CI.
