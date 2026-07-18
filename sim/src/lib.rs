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
//! It is part of the Phase 0 workspace scaffolding; the actual simulated
//! transporters are implemented in later phases.

pub mod can;
pub mod lin;
pub mod someip;

pub use tpt_chassis_core;
