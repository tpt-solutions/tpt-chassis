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

//! # TPT Chassis Sim
//!
//! Host-side (`std`-enabled) simulation and test tooling for the TPT Chassis
//! vehicle operating system. This crate provides in-software stand-ins for
//! vehicle hardware (for example a simulated CAN bus) so that the `no_std`
//! core crate can be exercised without target silicon.
//!
//! It provides in-memory network backends for every supported bus:
//! - [`can`] — an in-memory [`can::SimCanNetwork`] that broadcasts frames
//!   between nodes, mirroring real CAN behavior.
//! - [`lin`] — an in-memory LIN network (`SimLinNetwork`).
//! - [`someip`] — an in-memory SOME/IP network (`SimSomeIpNetwork`).
//!
//! These backends implement the `CanTransceiver` / `LinTransceiver` /
//! `SomeIpTransceiver` traits and are wrapped by `CanBus` / `LinBus` /
//! `SomeIpBus` in `tpt-chassis-core`, so application code runs unmodified
//! against either the simulator or real hardware.

pub mod can;
pub mod lin;
pub mod someip;

pub use tpt_chassis_core;
