# Phase 7 — Hardware Bring-Up

Automotive-grade ARM processors: NXP S32K / i.MX RT and Infineon AURIX
TRAVEO™ T2G (see `spec.txt` §2, §3 Phase 3).

## 7.1 Target dev boards — SELECTED

- **NXP** — **S32K144EVB** (Cortex-M4F, FlexCAN, LINFlexD). Primary bench target;
  widely available, well-supported by `probe-rs`, and representative of the
  S32K3X family used in production ECUs.
- **Infineon** — **TRAVEO™ T2G CYT2B** evaluation kit (Cortex-M4/M7, CAN FD, LIN).
  Secondary target covering the Infineon path.

Selection rationale: both expose FlexCAN/CAN-FD + LIN and a debug probe
(`probe-rs`-compatible), so the `no_std` core crate + Zephyr bindings
(`zephyr/`, built for `thumbv7em-none-eabihf`) can be exercised on real silicon.

## 7.2 Toolchain

- `arm-none-eabi` toolchain for flashing/debugging.
- `probe-rs` for flash + RTT telemetry offload (preferred), with `OpenOCD` as
  fallback.
- Zephyr board definitions for `s32k144` and `cyt2b`; the `tpt-chassis-zephyr`
  static lib is linked into the Zephyr app and called from `main.c`.

`.cargo/config.toml` (runner for `probe-rs`):

```toml
[target.thumbv7em-none-eabihf]
runner = "probe-rs run --chip S32K144"
```

## 7.3 Bring-up steps (run on hardware)

- [ ] Board support: bootloader, clock tree, pin mux for CAN/LIN/Ethernet
- [ ] Wire CAN transceiver (TJA1050 / TLE9251) and set bit-timing (500 kbit/s)
- [ ] Flash `tpt-chassis-zephyr` + Zephyr app via `probe-rs`

## 7.4 Validation (run on hardware)

- [ ] Validate against real CAN transceiver hardware (loopback + off-board peer)
- [ ] Bench test full stack (bus abstraction, AUTOSAR layer, OTA) on target silicon

The `no_std` core crate already builds for `thumbv7em-none-eabihf` (CI `no_std`
job) and the Zephyr bindings add the flash/`OtaStorage` backend. Hardware bring-up
requires physical boards and is performed against these steps when hardware is
available; it is tracked here rather than in CI.
