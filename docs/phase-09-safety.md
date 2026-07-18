# Phase 9 — ISO 26262 Certification Track

Pursue automotive safety certification (see `spec.txt` §3 Phase 4). Rust's memory
safety removes whole classes of defects and is expected to cut audit time.

The full HARA, safety-case template, and traceability matrix are in
[`docs/safety-case.md`](safety-case.md). Summary below.

## Work items

- [x] Produce hazard analysis and risk assessment (HARA) — `docs/safety-case.md` §9.1
- [x] Build safety case documentation — `docs/safety-case.md` §9.2
- [x] Establish requirements → implementation → test traceability — `docs/safety-case.md` §9.3
- [ ] Engage certification body / third-party auditor — §9.4 (process, pre-engagement)
- [ ] Address audit findings and iterate — §9.5 (process)

## Traceability (short form)

| Phase | Requirement | Implementation |
| --- | --- | --- |
| 1 | CAN abstraction | `core/src/can.rs` + `bus.rs` (`VehicleBus`) |
| 2 | Ethernet/LIN | `core/src/someip.rs`, `core/src/lin.rs` |
| 3 | AUTOSAR + UDS | `core/src/autosar.rs`, `core/src/uds.rs` |
| 4 | OTA | `core/src/ota.rs` |
| 5 | Autonomy | `core/src/autonomy.rs` |
| 6 | RTOS | `zephyr/`, `agl/` |
| 8 | Kill-switch / telemetry | `core/src/safety.rs` |

`#![forbid(unsafe_code)]` and `#![deny(missing_docs)]` in `core/src/lib.rs`
provide the baseline for the safety argument; MISRA/ASPICE-aligned review follows
once a certification body is engaged.
