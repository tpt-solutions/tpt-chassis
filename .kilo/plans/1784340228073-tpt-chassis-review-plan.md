# TPT Chassis — Review Findings & Improvement Plan

Review of `C:\Programming\tpt-chassis` (Rust `no_std` vehicle OS, AUTOSAR replacement).
Scope requested: bugs, todos, missing features, innovations, usability/automation, frontend
completeness, and faster adoption via examples/templates.

**Important expectation note on "frontend":** This is an embedded, `no_std`, `#![forbid(unsafe_code)]`
Rust OS for ECUs. There is **no web/GUI/HMI frontend** in this repo, and none is implied by
`spec.txt` or `todo.md`. The "frontend" of a vehicle OS is the *driver/integrator API surface*
(`VehicleBus`, `AutonomyStack`, `OtaEngine`, `UdsServer`). This plan treats that API surface as the
"frontend" to finish and polish. If a real web dashboard / HMI was intended, that is a separate
product and worth scoping separately.

---

## 1. Confirmed Bugs (must fix)

### 1.1 `Interlock` trait impl is infinitely recursive — CRITICAL
`core/src/safety.rs:108-123`, the `impl<S: AutonomyStack> AutonomyStack for Interlock<S>` block:
- `ingest` calls `self.ingest(sample)` (recursion, no forward to `self.stack`).
- `next_command` calls `self.next_command(state)` (recursion).
- `has_control` calls `self.has_control()` (recursion).

The *inherent* methods (`safety.rs:89,97,84`) correctly forward to `self.stack`, but the **trait**
methods do not. Any generic code that takes `impl AutonomyStack` (the whole point of the design,
e.g. `LaneKeepingStack` usage pattern) and is given an `Interlock<S>` will stack-overflow.
**Fix:** the trait impl methods must call the inner `self.stack` methods (or simply delegate to the
inherent inherent methods). Add a test that drives an `Interlock<LaneKeepingStack>` *through the
trait* (assign to `dyn`/generic `impl AutonomyStack`) to lock in the fix.

### 1.2 CAN extended-ID maximum is off by one
`core/src/can.rs:46`: `EXTENDED_MAX: u32 = 0x1FFF_FFFF` but a 29-bit CAN ID maxes at `0x1FFFFFFF`.
`CanId::extended(0x1FFFFFFF)` is wrongly rejected. Fix: `0x1FFF_FFFF` → `0x1FFFFFFF`.
(Tests at `can.rs:231` only check up to current max, so they don't catch it.)

### 1.3 Validation functions are dead code (silent malformed frames)
- `can::validate_frame` (`can.rs:205`) — never called in `CanBus::transmit` (`can.rs:184`).
- `someip::validate_message` (`someip.rs:366`) — never called in `SomeIpBus::transmit` (`someip.rs:348`).
- `lin::validate_frame` (`lin.rs:191`) — never called in `LinBus::transmit`.
- `BusError::InvalidFrame` (`bus.rs:59`) — defined but never constructed anywhere.

Result: an oversized/bad frame is handed straight to the transceiver unchecked. **Fix:** call the
relevant `validate_*` inside each `transmit()` and map failures to `BusError::InvalidFrame`. This
also makes `BusError::InvalidFrame` reachable and testable.

---

## 2. Todos / Status Inconsistencies (docs hygiene)

- `README.md:8` still says **"Phase 0 — Project Setup & Licensing (pre-Alpha)"** while `todo.md`
  marks Phases 0–6 and 10 as **done**. Update README status line.
- `docs/phase-10-ecosystem.md:7-9` deliverables are still unchecked `- [ ]`, yet `todo.md:98-100`
  marks Phase 10 done. Either finish the docs or un-check the todo items.
- `core/src/lib.rs:22-25` and `sim/src/lib.rs:24-25` module docs say CAN/Ethernet/LIN are
  "later phases" even though those modules already exist — stale comments.
- `core/src/lib.rs` module list (`lib.rs:27-33`) omits `autonomy`, `autosar`, `can`, `lin`,
  `someip`, `uds`, `ota` (only lists `bus` and `safety`) — incomplete docs.

---

## 3. Missing Features & Gaps

1. **Real crypto signature backend.** `ota::DemoSigner` (FNV-1a) is explicitly not for production.
   No Ed25519/`SignatureScheme` impl exists. Add an optional `ed25519` backend (feature-gated,
   `std`/host or a `no_std` impl like `ed25519-dalek`/`ed25519-compact`) and document swapping it in.
2. **`examples/` directory is absent everywhere.** No runnable adopter examples. This is the single
   biggest adoption blocker (see §5).
3. **No CAN FD / flexible data-rate.** Only classical 8-byte CAN. FD (64-byte) is common in modern
   vehicles. Consider a `CanFdFrame`/transport abstraction.
4. **No ISO-TP (ISO 15765-2) segmentation.** `uds.rs` only handles single-frame requests
   (`UDS_MAX_PAYLOAD=7`). Real UDS needs multi-frame transport. Add an ISO-TP layer.
5. **No timeouts / watchdog in OTA `recover`.** `tester_present_pending` is tracked in UDS but never
   decremented or enforced. Real safety needs a liveness timeout that auto-disarms.
6. **`Interlock` has no `arm`/`disarm` audit trail into `TelemetryRing`.** A kill-switch event should
   be logged (code + timestamp) for field reconstruction.
7. **RTOS ports are stubs.** `zephyr/src/{ffi,ota_storage}.rs` and `agl/src/{can_socket,ota_storage}.rs`
   exist but CI only builds host tests; no real peripheral driver is implemented. These are
   "planned" not "validated" — `todo.md` overstates "validated".

---

## 4. Innovation Suggestions (differentiators)

1. **`tpt-cli` developer tool** (host binary): `new` (scaffold an ECU crate from a template),
   `sim` (run the simulated bus + a stack), `ota sign`/`ota stage` (package & sign updates with the
   real signer), `diag` (a UDS tester-present/session client over the sim or a socketcan backend).
   This directly solves the "automation + faster adoption" ask.
2. **Declarative DBC / signal database → safe Rust** codegen: compile a `.dbc` or `signals.toml`
   into typed `Signal` pack/unpack accessors. Removes the #1 manual-error source for integrators.
3. **Hardware-in-the-loop (HIL) GitHub Action**: run the sim suite against a SocketCAN vcan interface
   so Linux CI exercises a *real* CAN stack, not just the in-memory sim.
4. **Tracing/telemetry offload tool**: dump `TelemetryRing` over UDS or SOME/IP for fleet debugging.
5. **`tp t-flight-control` bridge stub**: `spec.txt` §4 mentions ecosystem comms (TPT Beam / 5G) —
   a small typed telemetry message + 5G/transport shim would showcase the full vision.

---

## 5. Faster Adoption: Examples & Templates (explicit ask)

Create these, all runnable via `cargo run --example` or `cargo test`:
- `core/examples/hello_ecu.rs` — bring up a `CanBus` + `SimCanNetwork`, send/receive, print.
- `core/examples/ota_update.rs` — stage → promote → commit → recover lifecycle with `MemStorage`
  and the real (or demo) signer, including a power-loss injection.
- `core/examples/uds_diagnostics.rs` — a `UdsServer` + `MockData` answering `22`/`2E`/`27` over sim CAN.
- `core/examples/autonomy_lane_keep.rs` — `LaneKeepingStack` wrapped in `Interlock`, sensors-in /
  commands-out loop, asserted safe-stop when disarmed.
- `core/examples/custom_stack.rs` — a *from-scratch* `impl AutonomyStack` (template skeleton) so
  third parties have a starting point.
- `templates/ecu-crate/` — a copy-paste starter workspace (Cargo.toml + `src/main.rs` skeleton +
  `signals.toml`) referenced from the Phase-10 integrator guide.
- Expand `docs/phase-10-ecosystem.md` from outline → copy-paste quickstart with the examples above.

---

## 6. Implementation Order (suggested)

1. Fix §1.1 (Interlock recursion) + add trait-path test. **Highest risk.**
2. Fix §1.2 (extended ID) + add boundary test for `0x1FFFFFFF`.
3. Wire §1.3 `validate_*` into each `transmit()`; add tests asserting `InvalidFrame`.
4. Add `examples/` (§5) — doubles as adoption material and regression coverage.
5. Real `ed25519` `SignatureScheme` backend (feature-gated) + `tpt-cli` `ota sign`/`stage`.
6. ISO-TP layer for UDS; CAN FD frame type.
7. Docs hygiene (§2): README status, module docs, phase-10 checklist.
8. DBC/signal codegen + `templates/` + expanded integrator guide.
9. HIL CI (vcan) and `TelemetryRing` audit logging for interlock.

## 7. Validation

- `cargo test` must stay green; add the new tests above.
- `cargo build -p tpt-chassis-core --target thumbv7em-none-eabihf` (already in CI) must stay green
  after changes — keep `#![forbid(unsafe_code)]` and `no_std` intact.
- `cargo clippy --all-targets` clean (add to CI if not present).
- Manual: run each new `examples/` and confirm output.

## 8. Open Questions for User

- Is a **web/HMI dashboard frontend** actually wanted (separate from the driver API)? If yes, scope a
  new crate (e.g. `tpt-chassis-hmi` over a host transport) — currently out of scope of this repo.
- Preferred crypto lib for the OTA signer: `ed25519-dalek` (std/host) vs `ed25519-compact`
  (`no_std` friendly)? This affects the feature gate design.
- Should `tpt-cli` be a separate workspace crate or a binary in `sim`? Recommend a new `cli/` crate.
