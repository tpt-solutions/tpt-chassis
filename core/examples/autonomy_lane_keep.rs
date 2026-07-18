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

//! `autonomy_lane_keep` — wrap a `LaneKeepingStack` in an `Interlock` safety
//! kill-switch, run a sensors-in / commands-out loop over the simulated CAN
//! bus, and assert that a disarmed interlock suppresses all actuator output
//! (safe stop). Run with:
//!
//! ```sh
//! cargo run -p tpt-chassis-core --example autonomy_lane_keep
//! ```

use tpt_chassis_core::autonomy::{
    ControlCommand, LaneKeepingStack, SensorKind, SensorSample, VehicleState,
};
use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_core::safety::{Interlock, InterlockState};
use tpt_chassis_sim::can::SimCanNetwork;

fn main() {
    let net = SimCanNetwork::new();
    let sensor = CanBus::new(net.node());
    let mut ecu = CanBus::new(net.node());

    let mut interlock = Interlock::new(LaneKeepingStack::new(10, 1000));
    // Start DISARMED: the kill-switch must suppress commands until a trusted
    // supervisor arms it (e.g. after bench checks + signed-image verification).
    assert_eq!(interlock.state(), InterlockState::Disarmed);

    let camera_id = CanId::standard(0x200).expect("valid id");

    // Sensor node publishes a +20cm right lane offset.
    let offset: i32 = 20;
    let frame = CanFrame::new(camera_id, &offset.to_le_bytes()).expect("valid frame");
    let mut sensor = sensor;
    sensor.transmit(frame).expect("transmit offset");

    // ECU receives, decodes offset, feeds the autonomy stack.
    assert!(ecu.can_receive());
    let rx = ecu.receive().expect("receive offset");
    let reported = i32::from_le_bytes(rx.data().try_into().unwrap());
    interlock
        .ingest(SensorSample {
            kind: SensorKind::Camera,
            timestamp: 1,
            value: reported,
        })
        .expect("ingest");

    let cmd: Option<ControlCommand> = interlock
        .next_command(VehicleState {
            speed_cmps: 0,
            timestamp: 1,
        })
        .expect("next_command");
    // Disarmed => safe stop: no command may be emitted.
    assert!(cmd.is_none());
    println!("disarmed interlock suppressed command (safe stop OK)");

    // Trusted supervisor arms the interlock; the loop now passes commands.
    interlock.arm();
    assert_eq!(interlock.state(), InterlockState::Armed);
    let cmd = interlock
        .next_command(VehicleState {
            speed_cmps: 0,
            timestamp: 1,
        })
        .expect("next_command")
        .expect("command when armed");
    // +20cm offset * gain 10 = +200 deci-degrees (steer right toward center).
    assert_eq!(cmd.steer_deci_deg, 200);
    println!(
        "armed interlock produced command: steer {} deci-deg, throttle {}%",
        cmd.steer_deci_deg, cmd.throttle_pct
    );

    println!("autonomy_lane_keep: interlock gating verified");
}
