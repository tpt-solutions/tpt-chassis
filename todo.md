# TPT Chassis — Project Checklist

Open-source, memory-safe vehicle OS in Rust (AUTOSAR replacement). Dual-licensed
MIT OR Apache-2.0, © TPT Solutions. Tracked in phases per `spec.txt`.

## Phase 0 — Project Setup & Licensing
Foundational repo work before any technical spec work begins.

- [x] Initialize Cargo workspace (core `no_std` crate + std-enabled tooling/test crates)
- [x] Add `LICENSE-MIT` and `LICENSE-APACHE` (copyright TPT Solutions)
- [x] Add SPDX `MIT OR Apache-2.0` headers to `Cargo.toml` and source file template
- [x] Write `README.md` (project summary, architecture overview, license)
- [x] Write `CONTRIBUTING.md`
- [x] Set up CI pipeline (build, clippy/lint, test on push/PR)
- [x] Add issue and PR templates

## Phase 1 — CAN Bus Abstraction Layer (Rust)
Core Vehicle Bus Abstraction, starting with CAN (spec §2, §3 Phase 1).

- [x] Design unified bus trait/API (to support CAN now, Ethernet/LIN later)
- [x] Implement CAN bus driver/abstraction in safe Rust
- [x] Build a simulated CAN bus for testing without hardware
- [x] Unit tests for CAN abstraction against the simulator
- [x] Validate `no_std` / bare-metal compatibility

## Phase 2 — Ethernet (SOME/IP) & LIN Bus Support
Extend the Vehicle Bus Abstraction to the remaining two in-car networks (spec §2).

- [x] Implement SOME/IP protocol support for Ethernet
- [x] Implement LIN bus driver/abstraction
- [x] Extend unified bus API to cover all three bus types
- [x] Integration tests exercising CAN + Ethernet + LIN through the unified API

## Phase 3 — AUTOSAR-Compatible Interface Layer
Safe-Rust interfaces compatible with existing AUTOSAR sensors/actuators (spec §2, §3 Phase 2 — hand-written, no code-gen tooling in scope).

- [x] Identify target AUTOSAR interfaces to replicate
- [x] Hand-implement AUTOSAR-equivalent interfaces in safe Rust
- [x] Implement UDS (diagnostics) protocol support
- [x] Conformance tests against reference AUTOSAR interface behavior

## Phase 4 — OTA (Over-the-Air) Update Engine
Secure, atomic updates that can't brick the vehicle (spec §2).

- [x] Design update package format and signing/verification scheme
- [x] Implement atomic update apply + rollback mechanism
- [x] Design staged/canary rollout strategy
- [x] Failure-injection tests: power loss mid-update, corrupted payload, forced rollback

## Phase 5 — Autonomous Driving Interface
Plug-and-play integration surface for self-driving stacks (spec §2).

- [x] Define plugin/integration API contract for autonomous driving stacks
- [x] Document expected data flow (sensors in, control commands out)
- [x] Build a reference/mock self-driving stack to validate the interface
- [x] Integration tests against the mock stack

## Phase 6 — RTOS Integration (parallel tracks)
Target tech stack lists both options; track each independently (spec §2).

### Track A: Zephyr RTOS
- [x] Port/integrate core crate onto Zephyr
- [x] Validate bus abstraction + OTA engine under Zephyr

### Track B: Automotive-grade Linux (AGL)
- [x] Port/integrate core crate onto AGL
- [x] Validate bus abstraction + OTA engine under AGL

## Phase 7 — Hardware Bring-Up
Automotive-grade ARM processors: NXP, Infineon (spec §2, §3 Phase 3).

- [ ] Select target dev boards (NXP / Infineon automotive-grade ARM)
- [ ] Bring up board support (boot, drivers, toolchain)
- [ ] Validate against real CAN transceiver hardware
- [ ] Bench test full stack (bus abstraction, AUTOSAR layer, OTA) on target silicon

## Phase 8 — Real Vehicle Integration
Run on an ECU in a test vehicle (spec §3 Phase 3).

- [x] Define safety interlocks / kill-switch for bench-to-vehicle transition
- [x] Write in-vehicle test plan
- [x] Implement telemetry/logging for field testing
- [x] Define incident response and rollback procedure
- [ ] Run supervised test on a real test vehicle ECU

## Phase 9 — ISO 26262 Certification Track
Pursue automotive safety certification (spec §3 Phase 4).

- [x] Produce hazard analysis and risk assessment (HARA)
- [x] Build safety case documentation
- [x] Establish requirements → implementation → test traceability
- [ ] Engage certification body / third-party auditor
- [ ] Address audit findings and iterate

## Phase 10 — Ecosystem & Market Readiness
Enable adoption by EV startups, autonomous stacks, and fleets (spec §5).

- [x] Write integrator documentation for EV startups building on TPT Chassis
- [x] Write example integration guides for third-party autonomous driving stacks
- [x] Build fleet OTA rollout tooling/runbook for simultaneous multi-vehicle updates

## Phase 11 — Review Findings & Hardening
Action items from the platform review (bugs, gaps, adoption). See `.kilo/plans/`.

### Bugs (must fix)
- [ ] Fix `Interlock` `AutonomyStack` trait impl infinite recursion in `core/src/safety.rs:108-123` (delegate to inner `self.stack`, not `self`); add a test driving `Interlock` through the trait
- [ ] Fix CAN extended-ID max: `EXTENDED_MAX` `0x1FFF_FFFF` -> `0x1FFFFFFF` in `core/src/can.rs:46`; add boundary test for `0x1FFFFFFF`
- [ ] Wire `validate_frame`/`validate_message` into each `transmit()` (CAN/SOME/IP/LIN) and map failures to `BusError::InvalidFrame`; add tests asserting `InvalidFrame`

### Docs & status hygiene
- [ ] Update `README.md` status line (still says "Phase 0") to reflect completed phases
- [ ] Reconcile `docs/phase-10-ecosystem.md` deliverables (still `- [ ]`) with todo's "done" marking
- [ ] Refresh stale module docs in `core/src/lib.rs` and `sim/src/lib.rs` ("later phases" comments)
- [ ] Complete `core/src/lib.rs` module list (add autonomy, autosar, can, lin, someip, uds, ota)

### Missing features
- [ ] Add a real Ed25519 `SignatureScheme` backend (feature-gated, `no_std`-friendly) to replace `DemoSigner`
- [ ] Add ISO-TP (ISO 15765-2) segmentation layer so `UdsServer` handles multi-frame requests
- [ ] Add CAN FD (64-byte) frame type / transport abstraction
- [ ] Implement (not just stub) `zephyr` and `agl` peripheral backends; mark todo Phase 6 as "planned" until validated
- [ ] Enforce UDS `tester_present` liveness timeout that auto-disarms
- [ ] Log `Interlock` arm/disarm events into `TelemetryRing` for field reconstruction

### Adoption: examples & templates
- [ ] Add `core/examples/hello_ecu.rs` (bring up CAN + sim, send/receive)
- [ ] Add `core/examples/ota_update.rs` (stage/promote/commit/recover + power-loss injection)
- [ ] Add `core/examples/uds_diagnostics.rs` (`UdsServer` over sim CAN)
- [ ] Add `core/examples/autonomy_lane_keep.rs` (stack wrapped in `Interlock`, safe-stop asserted)
- [ ] Add `core/examples/custom_stack.rs` (from-scratch `impl AutonomyStack` skeleton)
- [ ] Add `templates/ecu-crate/` starter workspace referenced from the integrator guide
- [ ] Expand `docs/phase-10-ecosystem.md` outline into a copy-paste quickstart using the examples

### Innovation / automation
- [ ] Add `tpt-cli` developer tool: `new` (scaffold), `sim`, `ota sign`/`ota stage`, `diag` (UDS client)
- [ ] Add DBC / `signals.toml` -> safe-Rust `Signal` codegen
- [ ] Add HIL GitHub Action exercising the sim suite over a SocketCAN `vcan` interface
- [ ] Add `tpt-flight-control` bridge stub (typed telemetry + TPT Beam/5G shim per spec §4)
- [ ] Add `cargo clippy --all-targets` gate to CI if not present
