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

//! # TPT Chassis — Automotive Grade Linux (AGL) bindings (Track B)
//!
//! AGL is a full Linux distribution, so this crate uses the `std` build path and
//! binds the [`tpt_chassis_core`] abstractions to Linux/AGL services:
//!
//! | Concern     | Core abstraction                | AGL / Linux binding                  |
//! | ----------- | ------------------------------- | ------------------------------------ |
//! | CAN         | [`can::CanTransceiver`]         | SOCKETCAN (`can0`, `vcan0`, ...)     |
//! | Persistence | [`ota::OtaStorage`]             | A/B image files under `/var/tpt`     |
//! | Actuation   | [`autonomy::AutonomyStack`]     | Linux task feeding actuator services |
//!
//! The `no_std` constraint is relaxed on this track; the same safe engine logic
//! from `tpt_chassis_core` runs unchanged.

pub mod can_socket;
pub mod ota_storage;

pub use tpt_chassis_core;
