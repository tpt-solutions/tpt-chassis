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

//! `custom_stack` — a *from-scratch* `impl AutonomyStack` skeleton you can copy
//! to integrate your own self-driving stack (Apollo, Autoware, or bespoke).
//! Feed sensor samples in via `ingest`, pull `ControlCommand`s out via
//! `next_command`. Run with:
//!
//! ```sh
//! cargo run -p tpt-chassis-core --example custom_stack
//! ```

use tpt_chassis_core::autonomy::{
    AutonomyError, AutonomyStack, ControlCommand, SensorKind, SensorSample, VehicleState,
};

/// Your bespoke autonomy stack. Replace the inner state with your policy.
struct DemoStack {
    latest_steering_cm: i32,
    ready: bool,
}

impl DemoStack {
    fn new() -> Self {
        DemoStack {
            latest_steering_cm: 0,
            ready: false,
        }
    }
}

impl AutonomyStack for DemoStack {
    fn ingest(&mut self, sample: SensorSample) -> Result<(), AutonomyError> {
        // Only lateral-perception sensors inform this demo policy.
        if matches!(sample.kind, SensorKind::Camera | SensorKind::Lidar) {
            self.latest_steering_cm = sample.value;
            self.ready = true;
        }
        Ok(())
    }

    fn next_command(
        &mut self,
        state: VehicleState,
    ) -> Result<Option<ControlCommand>, AutonomyError> {
        if !self.ready {
            return Ok(None);
        }
        // Toy policy: steer proportional to the lateral offset, hold a slow speed.
        let steer = ControlCommand::clamp_steer(self.latest_steering_cm as i16 * 10);
        let throttle = if state.speed_cmps < 500 { 30 } else { 0 };
        Ok(Some(ControlCommand::new(
            steer,
            throttle,
            0,
            state.timestamp,
        )))
    }

    fn has_control(&self) -> bool {
        self.ready
    }
}

fn main() {
    let mut stack = DemoStack::new();
    assert!(!stack.has_control());

    stack
        .ingest(SensorSample {
            kind: SensorKind::Camera,
            timestamp: 1,
            value: 15,
        })
        .expect("ingest");

    let cmd = stack
        .next_command(VehicleState {
            speed_cmps: 0,
            timestamp: 1,
        })
        .expect("next_command")
        .expect("command ready");
    println!(
        "custom_stack: offset 15cm -> steer {} deci-deg, throttle {}%",
        cmd.steer_deci_deg, cmd.throttle_pct
    );
    assert_eq!(cmd.steer_deci_deg, 150);

    println!("custom_stack: skeleton runs — replace DemoStack with your own policy");
}
