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

//! Starter ECU crate built on TPT Chassis.
//!
//! Copy this directory, adjust the `path =` deps in `Cargo.toml`, and fill in
//! `main()` with your application logic. On a host you can run it against the
//! in-memory simulator; for bare metal, implement `VehicleBus` for your MCU's
//! peripherals and drop the `tpt-chassis-sim` dev-dependency.

use tpt_chassis_core::autosar::Signal;
use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_sim::can::SimCanNetwork;

fn main() {
    let net = SimCanNetwork::new();
    let mut bus = CanBus::new(net.node());

    // Decode a 16-bit signed lane-offset signal (start bit 0, 16 bits).
    let lane_offset = Signal::new(0, 16).expect("valid signal layout");
    let id = CanId::standard(0x200).expect("valid id");
    let payload: u64 = 0x0014; // +20 cm
    let offset_cm = lane_offset.unpack(payload) as i16;
    assert_eq!(offset_cm, 20);

    let frame = CanFrame::new(id, &payload.to_le_bytes()[..2]).expect("valid frame");
    bus.transmit(frame).expect("transmit");
    println!("ECU received lane offset {} cm on 0x{:X}", offset_cm, id.raw());

    // Encode a steering command back out (start bit 0, 16 bits, signed).
    let steer = Signal::new(0, 16).expect("valid signal layout");
    let cmd_word = steer.pack(0, (offset_cm as i64 * 10) as u64).expect("fits");
    let out_id = CanId::standard(0x300).expect("valid id");
    let out = CanFrame::new(out_id, &cmd_word.to_le_bytes()[..2]).expect("valid frame");
    bus.transmit(out).expect("transmit command");
    println!("ECU issued steering command {} deci-deg", offset_cm * 10);

    println!("ecu-crate template: signal decode/encode + CAN loop OK");
}
