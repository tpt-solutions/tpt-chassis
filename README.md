# TPT Chassis

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

**An open-source, memory-safe vehicle operating system written in Rust — a
drop-in-replacement for the AUTOSAR stack.**

> Status: **Phase 0 — Project Setup & Licensing** (pre-Alpha). See
> [`todo.md`](todo.md) for the full phased roadmap and [`spec.txt`](spec.txt)
> for the design document.

## Why

Modern cars ship 100+ ECUs running bloated AUTOSAR C/C++ code. Updating that
software takes years and billions of dollars. TPT Chassis replaces the AUTOSAR
stack with safe Rust and provides a unified API for every vehicle subsystem
(powertrain, brakes, steering, infotainment) — enabling true software-defined
vehicles with secure, atomic over-the-air updates.

## Architecture

TPT Chassis is organized as a Cargo workspace:

| Component | Crate | Description |
| --- | --- | --- |
| Core | `tpt-chassis-core` | `no_std`, `forbid(unsafe_code)` building blocks for bare-metal targets. |
| Simulation | `tpt-chassis-sim` | `std`-enabled host tooling: simulated CAN/LIN/SOME/IP buses and other stand-ins so the core can be tested without hardware. |

### Core modules (`tpt-chassis-core`)

| Module | Purpose |
| --- | --- |
| [`bus`](core/src/bus.rs) | Unified [`VehicleBus`](core/src/bus.rs) trait + [`Frame`](core/src/bus.rs) contract shared by every network. |
| [`can`](core/src/can.rs) | CAN frame, ID (standard/extended), and [`CanBus`](core/src/can.rs) high-level interface. |
| [`lin`](core/src/lin.rs) | LIN frame, protected-ID parity, and [`LinBus`](core/src/lin.rs) interface. |
| [`someip`](core/src/someip.rs) | SOME/IP header + message + [`SomeIpBus`](core/src/someip.rs) for automotive Ethernet. |
| [`autosar`](core/src/autosar.rs) | Safe-Rust AUTOSAR equivalents: DIO driver, COM signals. |
| [`uds`](core/src/uds.rs) | UDS (ISO 14229) diagnostic server over CAN. |
| [`ota`](core/src/ota.rs) | Secure, atomic A/B update engine with rollback. |
| [`autonomy`](core/src/autonomy.rs) | Self-driving stack plugin contract + reference lane-keeper. |

The system is designed around these core components (per `spec.txt`):

- **AUTOSAR Replacement** — the same interfaces as AUTOSAR, implemented in safe Rust.
- **Vehicle Bus Abstraction** — one unified API over CAN, Ethernet (SOME/IP), and LIN.
- **OTA Update Engine** — secure, atomic updates that cannot brick the vehicle.
- **Autonomous Driving Interface** — plug-and-play integration with self-driving stacks (Apollo, Autoware).

## Roadmap

Work is tracked in ten phases in [`todo.md`](todo.md). Highlights:

- **Phase 0** — Workspace, licensing, CI, contributor docs (done).
- **Phase 1** — CAN bus abstraction layer + simulated CAN bus (done).
- **Phase 2** — Ethernet (SOME/IP) & LIN bus support (done).
- **Phase 3** — AUTOSAR-compatible interface layer + UDS diagnostics (done).
- **Phase 4** — OTA update engine with atomic apply/rollback (done).
- **Phase 5** — Autonomous driving interface (done).
- **Phase 6** — RTOS integration (Zephyr / AGL) — [plan](docs/phase-06-rtos.md).
- **Phase 7** — Hardware bring-up (NXP / Infineon) — [plan](docs/phase-07-hardware.md).
- **Phase 8** — Real vehicle integration — [plan](docs/phase-08-vehicle.md).
- **Phase 9** — ISO 26262 safety certification track — [plan](docs/phase-09-safety.md).
- **Phase 10** — Ecosystem & market readiness — [plan](docs/phase-10-ecosystem.md).

## Building

```sh
# Build the whole workspace (host target)
cargo build

# Build the bare-metal core for a no_std target
cargo build -p tpt-chassis-core --target thumbv7em-none-eabihf

# Run tests (host)
cargo test
```

## License

Licensed under either of

- [MIT License](LICENSE-MIT)
- [Apache License, Version 2.0](LICENSE-APACHE)

at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 License, shall be
dual licensed as above, without any additional terms or conditions.

Copyright © TPT Solutions. All rights reserved.
