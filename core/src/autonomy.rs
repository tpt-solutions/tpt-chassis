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

//! Autonomous driving interface.
//!
//! Defines the plug-and-play integration surface that self-driving stacks
//! (Apollo, Autoware, or a custom stack) use to drive TPT Chassis. The contract
//! is intentionally small and `no_std`:
//!
//! ```text
//! sensors in ──▶ AutonomyStack::ingest(...) ──▶ (internal policy)
//!                                                  │
//!                          AutonomyStack::next_command() ──▶ control commands out
//! ```
//!
//! A reference proportional lane-keeping stack ([`LaneKeepingStack`]) is
//! provided to validate the interface end-to-end without hardware.

use crate::Error;

/// A timestamp in milliseconds since a monotonically increasing epoch.
pub type TimestampMs = u32;

/// Kinds of sensor feeding the autonomy stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SensorKind {
    /// Forward-facing camera.
    Camera,
    /// LiDAR point cloud summary.
    Lidar,
    /// Radar object track.
    Radar,
    /// GNSS / GPS fix.
    Gps,
    /// Wheel speed / odometry.
    Odometry,
}

/// A single sensor sample handed to the autonomy stack.
///
/// `value` is sensor-specific: e.g. for an inferred lane-center offset it is the
/// signed lateral offset in centimeters (positive = right of center). Keeping a
/// single `i32` payload keeps the type `no_std`- and allocation-friendly while
/// still carrying rich, stack-defined semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SensorSample {
    /// Which sensor produced the sample.
    pub kind: SensorKind,
    /// Sample timestamp.
    pub timestamp: TimestampMs,
    /// Sensor-specific signed value.
    pub value: i32,
}

/// A vehicle control command produced by the autonomy stack.
///
/// All actuator values are bounded and `Copy`. Steering is a signed angle in
/// deci-degrees (-300..=300 → -30.0°..=30.0°); throttle and brake are
/// percentages (0..=100).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlCommand {
    /// Desired steering angle, deci-degrees (positive = right).
    pub steer_deci_deg: i16,
    /// Throttle command, percent (0..=100).
    pub throttle_pct: u8,
    /// Brake command, percent (0..=100).
    pub brake_pct: u8,
    /// Sample timestamp the command is based on.
    pub timestamp: TimestampMs,
}

impl ControlCommand {
    /// Maximum steering magnitude in deci-degrees (±30.0°).
    pub const MAX_STEER: i16 = 300;

    /// Clamps the steering value into the valid range.
    pub fn clamp_steer(steer: i16) -> i16 {
        steer.clamp(-Self::MAX_STEER, Self::MAX_STEER)
    }

    /// Constructs a command, clamping steering and bounding throttle/brake.
    pub fn new(
        steer_deci_deg: i16,
        throttle_pct: u8,
        brake_pct: u8,
        timestamp: TimestampMs,
    ) -> ControlCommand {
        ControlCommand {
            steer_deci_deg: Self::clamp_steer(steer_deci_deg),
            throttle_pct: throttle_pct.min(100),
            brake_pct: brake_pct.min(100),
            timestamp,
        }
    }
}

/// A snapshot of vehicle state the stack may use for context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VehicleState {
    /// Current speed in centimeters per second.
    pub speed_cmps: u32,
    /// Current timestamp.
    pub timestamp: TimestampMs,
}

/// Errors from an autonomy stack integration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutonomyError {
    /// A generic core error.
    Core(Error),
    /// The stack's input buffer is full.
    InputFull,
    /// The sensor sample was malformed or out of range.
    InvalidSample,
    /// The stack is not in a state to emit commands.
    NotReady,
}

impl From<Error> for AutonomyError {
    fn from(e: Error) -> Self {
        AutonomyError::Core(e)
    }
}

/// The autonomy plugin contract.
///
/// A self-driving stack implements this trait. TPT Chassis feeds it sensor
/// samples and periodically pulls the resulting control command to send to the
/// actuators.
pub trait AutonomyStack {
    /// Ingests one sensor sample.
    fn ingest(&mut self, sample: SensorSample) -> Result<(), AutonomyError>;

    /// Produces the next control command, if the stack has one ready.
    ///
    /// A stack may return `None` when it has not yet received enough data or
    /// has deliberately relinquished control (e.g. safe stop).
    fn next_command(
        &mut self,
        state: VehicleState,
    ) -> Result<Option<ControlCommand>, AutonomyError>;

    /// Returns `true` if the stack currently holds control (has produced a
    /// recent command).
    fn has_control(&self) -> bool;
}

/// Maximum number of pending sensor samples buffered inside a stack.
pub const AUTONOMY_SAMPLE_BUFFER: usize = 16;

/// A reference self-driving stack: a proportional lane-keeper.
///
/// It tracks the most recent lane-center offset sample (in centimeters) and
/// commands steering proportional to that offset, keeping speed constant while
/// a valid offset is known. It is deterministic and allocation-free, intended
/// to validate the integration surface rather than to drive a real vehicle.
pub struct LaneKeepingStack {
    last_offset_cm: Option<i32>,
    last_timestamp: TimestampMs,
    ready: bool,
    /// Proportional gain (steering deci-degrees per cm of offset).
    gain: i16,
    /// Target cruise speed in cm/s (0 = hold, command throttle only if slower).
    cruise_cmps: u32,
}

impl LaneKeepingStack {
    /// Creates a lane-keeping stack with the given gain and cruise speed.
    pub fn new(gain: i16, cruise_cmps: u32) -> Self {
        LaneKeepingStack {
            last_offset_cm: None,
            last_timestamp: 0,
            ready: false,
            gain,
            cruise_cmps,
        }
    }
}

impl AutonomyStack for LaneKeepingStack {
    fn ingest(&mut self, sample: SensorSample) -> Result<(), AutonomyError> {
        if sample.kind != SensorKind::Camera && sample.kind != SensorKind::Lidar {
            // Only lateral-perception sensors inform lane keeping; ignore others.
            return Ok(());
        }
        self.last_offset_cm = Some(sample.value);
        self.last_timestamp = sample.timestamp;
        self.ready = true;
        Ok(())
    }

    fn next_command(
        &mut self,
        state: VehicleState,
    ) -> Result<Option<ControlCommand>, AutonomyError> {
        let offset = match self.last_offset_cm {
            Some(o) => o,
            None => return Ok(None),
        };
        if !self.ready {
            return Ok(None);
        }
        // Steering proportional to offset; positive offset (right) -> steer right.
        let steer = ControlCommand::clamp_steer((offset as i16).saturating_mul(self.gain));
        // Simple cruise: add throttle if below target speed, else coast.
        let throttle = if state.speed_cmps < self.cruise_cmps {
            (((self.cruise_cmps - state.speed_cmps) * 100) / self.cruise_cmps.max(1)) as u8
        } else {
            0
        };
        Ok(Some(ControlCommand::new(
            steer,
            throttle,
            0,
            self.last_timestamp,
        )))
    }

    fn has_control(&self) -> bool {
        self.ready && self.last_offset_cm.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_command_clamps_steering() {
        let c = ControlCommand::new(9999, 50, 0, 0);
        assert_eq!(c.steer_deci_deg, ControlCommand::MAX_STEER);
        assert_eq!(c.throttle_pct, 50);
        let c = ControlCommand::new(-9999, 200, 0, 0);
        assert_eq!(c.steer_deci_deg, -ControlCommand::MAX_STEER);
        assert_eq!(c.throttle_pct, 100);
    }

    #[test]
    fn stack_commands_toward_lane_center() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        // Offset +20cm to the right -> steer right (positive), proportional.
        stack
            .ingest(SensorSample {
                kind: SensorKind::Camera,
                timestamp: 100,
                value: 20,
            })
            .unwrap();
        assert!(stack.has_control());
        let cmd = stack
            .next_command(VehicleState {
                speed_cmps: 500,
                timestamp: 100,
            })
            .unwrap()
            .unwrap();
        assert_eq!(cmd.steer_deci_deg, 200); // 20 * 10
        assert!(cmd.throttle_pct > 0); // below cruise speed
    }

    #[test]
    fn stack_centers_when_on_lane() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        stack
            .ingest(SensorSample {
                kind: SensorKind::Camera,
                timestamp: 100,
                value: 0,
            })
            .unwrap();
        let cmd = stack
            .next_command(VehicleState {
                speed_cmps: 500,
                timestamp: 100,
            })
            .unwrap()
            .unwrap();
        assert_eq!(cmd.steer_deci_deg, 0);
    }

    #[test]
    fn stack_not_ready_without_samples() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        assert!(!stack.has_control());
        let cmd = stack
            .next_command(VehicleState {
                speed_cmps: 0,
                timestamp: 0,
            })
            .unwrap();
        assert!(cmd.is_none());
    }

    #[test]
    fn steering_does_not_exceed_bounds_at_large_offset() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        stack
            .ingest(SensorSample {
                kind: SensorKind::Lidar,
                timestamp: 1,
                value: 1000,
            })
            .unwrap();
        let cmd = stack
            .next_command(VehicleState {
                speed_cmps: 0,
                timestamp: 1,
            })
            .unwrap()
            .unwrap();
        assert_eq!(cmd.steer_deci_deg, ControlCommand::MAX_STEER);
    }
}
