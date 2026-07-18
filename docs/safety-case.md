# Phase 9 — ISO 26262 Certification Track

Pursuing automotive functional-safety certification (see `spec.txt` §3 Phase 4).
Rust's memory safety removes whole classes of defects (data races, buffer
overflows, use-after-free) and is expected to reduce audit scope and time.

This directory (and `docs/phase-09-safety.md`) captures the artifacts the
certification body will review. Items marked **(process)** are produced with
the auditor; items marked **(artifact)** are committed here.

## 9.1 Hazard Analysis and Risk Assessment (HARA) — *process + artifact*

The HARA identifies hazardous events from TPT Chassis functions and assigns an
ASIL based on **S**everity, **E**xposure, and **C**ontrollability.

| Function | Hazardous event | S | E | C | ASIL |
| --- | --- | --- | --- | --- | --- |
| OTA engine applies corrupt image | Vehicle immobilized / wrong firmware → loss of propulsion | S3 | E4 | C3 | **ASIL D** |
| Autonomy stack commands wrong steering | Unintended lateral movement | S3 | E4 | C2 | **ASIL D** |
| Bus abstraction drops safety frame | Missing brake/steer command | S3 | E3 | C2 | **ASIL C** |
| UDS server exposes write w/o security | Unauthorized calibration change | S2 | E3 | C2 | **ASIL B** |
| Telemetry loss | Unobservable malfunction | S1 | E4 | C3 | **ASIL A** |

Safety goals derived from the table:

- **SG-OTA**: No update shall make the vehicle unbootable (covered by the A/B
  `OtaEngine` state machine in `core/src/ota.rs`).
- **SG-AUTO**: Actuator authority shall be removable in bounded time (covered by
  `safety::Interlock` in `core/src/safety.rs`).
- **SG-BUS**: Safety-critical frames shall not be silently dropped (covered by
  `BusError::RxQueueEmpty`/`TxQueueFull` propagation in `core/src/bus.rs`).

## 9.2 Safety case — *artifact (template)*

The safety case argues, with evidence, that the safety goals hold. Structure:

1. **Claim** — each safety goal above.
2. **Argument** — how the code + tests satisfy it (see traceability table).
3. **Evidence** — unit/integration tests, static analysis (`#![forbid(unsafe_code)]`,
   `#![deny(missing_docs)]`), and CI gates.
4. **Assumptions** — RTOS provides preemption/MMU; crypto backend (Ed25519) is
   independently certified; the kill-switch is wired to a hard e-stop.

## 9.3 Requirements → implementation → test traceability — *artifact*

| Phase | Requirement | Implementation | Test |
| --- | --- | --- | --- |
| 1 | CAN abstraction | `core/src/can.rs`, `core/src/bus.rs` | `sim/tests/*`, `sim/src/can.rs` |
| 2 | Ethernet/LIN | `core/src/someip.rs`, `core/src/lin.rs` | `sim/tests/integration.rs` |
| 3 | AUTOSAR + UDS | `core/src/autosar.rs`, `core/src/uds.rs` | `sim/tests/uds_can.rs` |
| 4 | OTA atomic + rollback | `core/src/ota.rs` | `core/src/ota.rs` tests |
| 5 | Autonomy interface | `core/src/autonomy.rs` | `core/src/autonomy.rs` tests |
| 6 | RTOS integration | `zephyr/`, `agl/` | `zephyr/tests/`, `agl/tests/` |
| 8 | Kill-switch / interlock | `core/src/safety.rs` (`Interlock`) | `core/src/safety.rs` tests |
| 8 | Field telemetry | `core/src/safety.rs` (`TelemetryRing`) | `core/src/safety.rs` tests |

## 9.4 Certification body engagement — *process*

- Select an accredited auditor (TÜV / EXIDA / similar).
- Submit the safety case + HARA; iterate on findings.
- Outcome tracked in §9.5.

## 9.5 Audit findings and iteration — *process*

Log findings here as they arrive; each links to a code/test change and a re-run
of the relevant CI gate.

| Finding | Severity | Resolution | Status |
| --- | --- | --- | --- |
| _(none yet — pre-engagement)_ | — | — | — |
