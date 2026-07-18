// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) TPT Solutions. All rights reserved.
//
// Licensed under the MIT License and the Apache License, Version 2.0
// (the "Licenses"). You may obtain a copy of each License at:
//
//   - MIT:   https://opensource.org/licenses/MIT
//
//   - Apache: https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the Licenses is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the Licenses for the specific language governing permissions and
// limitations under each License.

//! End-to-end autonomous-driving integration: sensor samples arrive on CAN,
//! the autonomy stack converts them to control commands, and the commands are
//! dispatched back onto CAN to the actuators — exercising the full
//! "sensors in, control commands out" data flow.

use tpt_chassis_core::autonomy::{
    AutonomyStack, ControlCommand, LaneKeepingStack, SensorKind, SensorSample, VehicleState,
};
use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};

use tpt_chassis_sim::can::SimCanNetwork;

/// Encodes a control command into a CAN frame payload (steer:i16 be, thr, brk).
fn encode_command(cmd: ControlCommand) -> [u8; 5] {
    let mut out = [0u8; 5];
    out[0..2].copy_from_slice(&cmd.steer_deci_deg.to_be_bytes());
    out[2] = cmd.throttle_pct;
    out[3] = cmd.brake_pct;
    out[4] = 0;
    out
}

#[test]
fn sensors_in_commands_out_over_can() {
    let net = SimCanNetwork::new();
    // Three logical nodes on the bus: sensor source, ECU running the stack,
    // and the actuator consumer of the resulting commands.
    let mut sensor = CanBus::new(net.node());
    let mut ecu = CanBus::new(net.node());
    let mut actuator = CanBus::new(net.node());
    let mut stack = LaneKeepingStack::new(10, 1000);

    // Sensor sample (lane offset +20cm) arrives on the sensor CAN ID.
    let sample = SensorSample {
        kind: SensorKind::Camera,
        timestamp: 123,
        value: 20,
    };
    let mut payload = [0u8; 8];
    payload[0..4].copy_from_slice(&sample.value.to_be_bytes());
    payload[4..8].copy_from_slice(&sample.timestamp.to_be_bytes());
    let req = CanFrame::new(CanId::standard(0x200).unwrap(), &payload).unwrap();
    sensor.transmit(req).unwrap();

    // ECU ingests the sensor frame and feeds the stack.
    let incoming = ecu.receive().unwrap();
    let offset = i32::from_be_bytes([
        incoming.data()[0],
        incoming.data()[1],
        incoming.data()[2],
        incoming.data()[3],
    ]);
    let ts = u32::from_be_bytes([
        incoming.data()[4],
        incoming.data()[5],
        incoming.data()[6],
        incoming.data()[7],
    ]);
    stack
        .ingest(SensorSample {
            kind: SensorKind::Camera,
            timestamp: ts,
            value: offset,
        })
        .unwrap();

    let cmd = stack
        .next_command(VehicleState {
            speed_cmps: 500,
            timestamp: 123,
        })
        .unwrap()
        .expect("stack should produce a command");

    // Dispatch the command to the actuators on the command CAN ID.
    let payload = encode_command(cmd);
    ecu.transmit(CanFrame::new(CanId::standard(0x300).unwrap(), &payload).unwrap())
        .unwrap();

    // Actuator receives the command frame (it also received the broadcast
    // sensor frame, so drain until the command frame appears).
    let mut out = None;
    while let Ok(f) = actuator.receive() {
        if f.id().raw() == 0x300 {
            out = Some(f);
            break;
        }
    }
    let out = out.expect("actuator should receive the command frame");
    assert_eq!(out.id().raw(), 0x300);
    let steer = i16::from_be_bytes([out.data()[0], out.data()[1]]);
    assert_eq!(steer, 200); // 20cm * gain 10
    assert!(out.data()[2] > 0); // throttle applied (below cruise)
}

#[test]
fn stack_relinquishes_control_without_data() {
    let mut stack = LaneKeepingStack::new(10, 1000);
    let cmd = stack
        .next_command(VehicleState {
            speed_cmps: 0,
            timestamp: 0,
        })
        .unwrap();
    assert!(cmd.is_none());
    assert!(!stack.has_control());
}
