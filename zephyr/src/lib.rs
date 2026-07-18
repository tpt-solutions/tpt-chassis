// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) TPT Solutions. All rights reserved.
//
// Licensed under the MIT License and the Apache License, Version 2.0
// (the "Licenses"). You may obtain a copy of each License at:
//
//   - MIT:   https://opensource.org/licenses/MIT
//   - Apache: https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the Licenses is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the Licenses for the specific language governing permissions and
// limitations under each License.

//! # TPT Chassis — Zephyr RTOS bindings (Track A)
//!
//! This crate ports the [`tpt_chassis_core`] abstractions onto the Zephyr RTOS.
//! The core crate is `no_std` and `#![forbid(unsafe_code)]`, so the port only
//! has to supply the backend traits that Zephyr implements:
//!
//! | Concern     | Core abstraction                | Zephyr binding                       |
//! | ----------- | ------------------------------- | ------------------------------------ |
//! | Persistence | [`ota::OtaStorage`]             | Flash partition (A/B slots)          |
//! | CAN         | [`can::CanTransceiver`]         | MCU FlexCAN via a safe Tx/Rx ring    |
//! | Actuation   | [`autonomy::AutonomyStack`]     | Zephyr task feeding actuator threads |
//!
//! The crate builds for `thumbv7em-none-eabihf` (verified in CI) and links into a
//! Zephyr application as a static library. The hardware-facing FFI shims are
//! isolated in [`ffi`] and kept minimal; safety-critical logic stays in safe
//! Rust.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod ffi;
pub mod ota_storage;

pub use tpt_chassis_core;
