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

//! Vehicle safety interlocks and field telemetry (Phase 8).
//!
//! Implements the bench-to-vehicle transition controls called out in
//! `docs/phase-08-vehicle.md`:
//!
//! - [`Interlock`] — a hardware/software kill-switch that gates actuator
//!   authority. An autonomy stack wrapped by an `Interlock` can only emit
//!   control commands while the interlock is *armed*; disarming (e-stop,
//!   supervisor override, or failed health check) forces a safe stop and
//!   suppresses all actuator output.
//! - [`TelemetryRing`] — a fixed-capacity, allocation-free ring buffer used to
//!   record field-test events for offload, so a malfunction can be reconstructed
//!   after the fact.

use crate::autonomy::{AutonomyError, AutonomyStack, ControlCommand, SensorSample, VehicleState};

/// State of the autonomy kill-switch / interlock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterlockState {
    /// Actuator authority is granted (kill-switch open, supervisor armed).
    Armed,
    /// Actuator authority is denied; commands are suppressed (safe stop).
    Disarmed,
}

/// A safety interlock wrapping an [`AutonomyStack`].
///
/// While [`InterlockState::Armed`], commands produced by the inner stack are
/// passed through unchanged. While [`InterlockState::Disarmed`], every call to
/// [`next_command`](Interlock::next_command) returns `None` (safe stop) and
/// [`has_control`](Interlock::has_control) returns `false`, regardless of what
/// the inner stack reports — guaranteeing the kill-switch can never be defeated
/// by the autonomy software.
pub struct Interlock<S: AutonomyStack> {
    stack: S,
    state: InterlockState,
}

impl<S: AutonomyStack> Interlock<S> {
    /// Wraps `stack`, starting disarmed (fail-safe default).
    pub fn new(stack: S) -> Self {
        Interlock {
            stack,
            state: InterlockState::Disarmed,
        }
    }

    /// Arms the interlock, granting actuator authority.
    ///
    /// Must only be called by trusted supervisor code (e.g. after the bench
    /// checks and a signed-image check pass).
    pub fn arm(&mut self) {
        self.state = InterlockState::Armed;
    }

    /// Disarms the interlock, cutting actuator authority (safe stop).
    pub fn disarm(&mut self) {
        self.state = InterlockState::Disarmed;
    }

    /// Returns the current interlock state.
    pub fn state(&self) -> InterlockState {
        self.state
    }

    /// Returns `true` only if the interlock is armed AND the inner stack holds
    /// control.
    pub fn has_control(&self) -> bool {
        matches!(self.state, InterlockState::Armed) && self.stack.has_control()
    }

    /// Ingests a sensor sample, forwarded to the inner stack unconditionally.
    pub fn ingest(&mut self, sample: SensorSample) -> Result<(), AutonomyError> {
        self.stack.ingest(sample)
    }

    /// Produces the next command, but only if the interlock is armed.
    ///
    /// When disarmed this returns `Ok(None)` — a safe stop — so callers that
    /// drive actuators cannot accidentally apply stale or unauthorized output.
    pub fn next_command(
        &mut self,
        state: VehicleState,
    ) -> Result<Option<ControlCommand>, AutonomyError> {
        if !matches!(self.state, InterlockState::Armed) {
            return Ok(None);
        }
        self.stack.next_command(state)
    }
}

impl<S: AutonomyStack> AutonomyStack for Interlock<S> {
    fn ingest(&mut self, sample: SensorSample) -> Result<(), AutonomyError> {
        self.ingest(sample)
    }

    fn next_command(
        &mut self,
        state: VehicleState,
    ) -> Result<Option<ControlCommand>, AutonomyError> {
        self.next_command(state)
    }

    fn has_control(&self) -> bool {
        self.has_control()
    }
}

/// A single telemetry record for field logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetryRecord {
    /// Monotonic timestamp (ms).
    pub timestamp: u32,
    /// Free-form event code (caller-defined).
    pub code: u16,
    /// Optional 32-bit payload (e.g. speed, slot, error code).
    pub value: u32,
}

/// A fixed-capacity, allocation-free telemetry ring buffer.
///
/// Records overwrite the oldest entry once full, so it is safe to leave running
/// indefinitely on resource-constrained ECUs and offload periodically.
pub struct TelemetryRing<const N: usize> {
    buf: [TelemetryRecord; N],
    head: usize,
    len: usize,
}

impl<const N: usize> TelemetryRing<N> {
    /// Creates an empty ring buffer.
    pub fn new() -> Self {
        // `TelemetryRecord` is `Copy` with all-zero fields, which is a valid
        // (if meaningless) initial record; safe to zero-init the array.
        const ZERO: TelemetryRecord = TelemetryRecord {
            timestamp: 0,
            code: 0,
            value: 0,
        };
        TelemetryRing {
            buf: [ZERO; N],
            head: 0,
            len: 0,
        }
    }

    /// Records one event, overwriting the oldest if the buffer is full.
    pub fn push(&mut self, record: TelemetryRecord) {
        self.buf[self.head] = record;
        self.head = (self.head + 1) % N;
        if self.len < N {
            self.len += 1;
        }
    }

    /// Returns the number of stored records (<= N).
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns `true` if no records are stored.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Iterates the stored records from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &TelemetryRecord> {
        let start = if self.len == N { self.head } else { 0 };
        (0..self.len).map(move |i| &self.buf[(start + i) % N])
    }
}

impl<const N: usize> Default for TelemetryRing<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autonomy::{LaneKeepingStack, SensorKind};

    #[test]
    fn disarmed_interlock_suppresses_commands() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        stack
            .ingest(SensorSample {
                kind: SensorKind::Camera,
                timestamp: 1,
                value: 20,
            })
            .unwrap();
        let mut interlock = Interlock::new(stack);
        assert!(!interlock.has_control());
        let cmd = interlock
            .next_command(VehicleState {
                speed_cmps: 0,
                timestamp: 1,
            })
            .unwrap();
        assert!(cmd.is_none());
    }

    #[test]
    fn armed_interlock_passes_commands() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        stack
            .ingest(SensorSample {
                kind: SensorKind::Camera,
                timestamp: 1,
                value: 20,
            })
            .unwrap();
        let mut interlock = Interlock::new(stack);
        interlock.arm();
        assert!(interlock.has_control());
        let cmd = interlock
            .next_command(VehicleState {
                speed_cmps: 0,
                timestamp: 1,
            })
            .unwrap()
            .unwrap();
        assert_eq!(cmd.steer_deci_deg, 200);
    }

    #[test]
    fn disarm_after_arm_cuts_authority() {
        let mut stack = LaneKeepingStack::new(10, 1000);
        stack
            .ingest(SensorSample {
                kind: SensorKind::Camera,
                timestamp: 1,
                value: 5,
            })
            .unwrap();
        let mut interlock = Interlock::new(stack);
        interlock.arm();
        assert!(interlock.has_control());
        interlock.disarm();
        assert!(!interlock.has_control());
        assert!(interlock
            .next_command(VehicleState {
                speed_cmps: 0,
                timestamp: 1
            })
            .unwrap()
            .is_none());
    }

    #[test]
    fn telemetry_ring_overwrites_oldest() {
        let mut ring: TelemetryRing<4> = TelemetryRing::new();
        for i in 0..6u32 {
            ring.push(TelemetryRecord {
                timestamp: i,
                code: 1,
                value: i,
            });
        }
        assert_eq!(ring.len(), 4);
        // Oldest surviving record is the one written at i=2.
        let mut values = [0u32; 4];
        for (i, r) in ring.iter().enumerate() {
            values[i] = r.value;
        }
        assert_eq!(values, [2, 3, 4, 5]);
    }
}
