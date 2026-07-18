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

//! # TPT Flight Control bridge
//!
//! A bridge stub that streams vehicle telemetry from a TPT Chassis ECU to
//! [`tpt-flight-control`](https://github.com/tpt-solutions) — the smart-city
//! autonomous-vehicle routing service — over the **TPT Beam** 5G transport
//! (see `spec.txt` §4).
//!
//! The crate is intentionally `no_std` and allocation-free so it can run inside
//! the same ECU image as [`tpt_chassis_core`]. Transport is abstracted behind the
//! [`BeamTransport`] trait, so the exact 5G/Beam link can be swapped for a
//! simulator (or a SocketCAN-backed loop) without touching the telemetry
//! encoding logic.
//!
//! ## Shape of the data
//!
//! [`TelemetryFrame`] is a fixed-layout, `Copy` snapshot (speed, steering,
//! interlock state, timestamp) suitable for high-rate streaming.
//! [`FlightControlBridge`] collects frames and pushes them through a
//! [`BeamTransport`].

#![no_std]
#![forbid(unsafe_code)]

use tpt_chassis_core::safety::InterlockState;

/// A single, fixed-layout telemetry snapshot streamed to tpt-flight-control.
///
/// `Copy` + fixed size keeps it allocation-free for `no_std` ECUs and lets a
/// frame be cheaply cloned into a transmit queue or ring buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TelemetryFrame {
    /// Monotonic ECU timestamp (ms).
    pub timestamp_ms: u32,
    /// Vehicle speed in centimetres per second.
    pub speed_cmps: u32,
    /// Current steering command in deci-degrees (signed).
    pub steer_deci_deg: i16,
    /// `true` while the autonomy interlock is armed (actuator authority granted).
    pub autonomy_armed: bool,
    /// A 16-bit free-form event code (e.g. an interlock transition).
    pub event_code: u16,
}

impl TelemetryFrame {
    /// Builds a frame from the core vehicle state plus the interlock state.
    pub fn from_state(
        timestamp_ms: u32,
        speed_cmps: u32,
        steer_deci_deg: i16,
        interlock: InterlockState,
        event_code: u16,
    ) -> Self {
        TelemetryFrame {
            timestamp_ms,
            speed_cmps,
            steer_deci_deg,
            autonomy_armed: matches!(interlock, InterlockState::Armed),
            event_code,
        }
    }
}

/// The transport that actually moves encoded telemetry off the ECU.
///
/// Implementors wrap the TPT Beam 5G link (or, for testing, an in-memory sink).
/// The bridge never assumes a particular framing beyond "accept a byte slice".
pub trait BeamTransport {
    /// Pushes one encoded telemetry payload. Returns `Err` on transport failure.
    fn send(&mut self, payload: &[u8]) -> Result<(), BeamError>;
}

/// Errors raised by the Beam transport / bridge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeamError {
    /// The underlying 5G/Beam link is unavailable (out of coverage, detached).
    LinkDown,
    /// The payload exceeded the transport's maximum MTU.
    TooLarge,
    /// A transient send failure; safe to retry.
    Transient,
}

impl core::fmt::Display for BeamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BeamError::LinkDown => f.write_str("beam: link down"),
            BeamError::TooLarge => f.write_str("beam: payload too large"),
            BeamError::Transient => f.write_str("beam: transient send failure"),
        }
    }
}

/// Maximum encoded telemetry payload the default bridge emits (bytes).
///
/// Sized to fit a single [`TelemetryFrame`] plus a small header, comfortably
/// within a typical 5G/Beam MTU.
pub const MAX_FRAME_BYTES: usize = 64;

/// Encodes a [`TelemetryFrame`] into the wire payload format.
///
/// The layout is intentionally simple and stable: an 8-byte little-endian
/// header (magic + frame size) followed by the fixed-layout record. Encoded
/// size never exceeds [`MAX_FRAME_BYTES`]; the function returns the populated
/// slice.
pub fn encode_frame<'a>(frame: &TelemetryFrame, out: &'a mut [u8; MAX_FRAME_BYTES]) -> &'a [u8] {
    // Magic + version (LE): 'T' 'P' 'T' 0x01, then 2-byte little-endian size.
    out[0] = b'T';
    out[1] = b'P';
    out[2] = b'T';
    out[3] = 0x01;
    // Payload is the fixed-layout record below (timestamp + speed + steer +
    // armed flag + event code = 13 bytes).
    let size: u16 = 13;
    out[4..6].copy_from_slice(&size.to_le_bytes());

    // Fixed-layout record: timestamp, speed, steer, armed flag, event code.
    let mut off = 6usize;
    out[off..off + 4].copy_from_slice(&frame.timestamp_ms.to_le_bytes());
    off += 4;
    out[off..off + 4].copy_from_slice(&frame.speed_cmps.to_le_bytes());
    off += 4;
    out[off..off + 2].copy_from_slice(&frame.steer_deci_deg.to_le_bytes());
    off += 2;
    out[off] = frame.autonomy_armed as u8;
    off += 1;
    out[off..off + 2].copy_from_slice(&frame.event_code.to_le_bytes());
    off += 2;

    &out[..off]
}

/// Bridges TPT Chassis telemetry onto a TPT Beam transport.
///
/// The bridge is allocation-free and reuses a single internal encode buffer, so
/// it can live inside an ECU's main loop and stream frames at the vehicle's
/// sensor rate.
pub struct FlightControlBridge<T: BeamTransport> {
    transport: T,
    buf: [u8; MAX_FRAME_BYTES],
}

impl<T: BeamTransport> FlightControlBridge<T> {
    /// Wraps a Beam transport as a telemetry bridge.
    pub fn new(transport: T) -> Self {
        FlightControlBridge {
            transport,
            buf: [0u8; MAX_FRAME_BYTES],
        }
    }

    /// Encodes and transmits a single telemetry frame.
    pub fn publish(&mut self, frame: &TelemetryFrame) -> Result<(), BeamError> {
        let payload = encode_frame(frame, &mut self.buf);
        self.transport.send(payload)
    }

    /// Returns a reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Returns a mutable reference to the underlying transport.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recording transport used to assert what the bridge emits.
    struct Sink {
        last: [u8; MAX_FRAME_BYTES],
        len: usize,
    }

    impl Default for Sink {
        fn default() -> Self {
            Sink {
                last: [0u8; MAX_FRAME_BYTES],
                len: 0,
            }
        }
    }

    impl BeamTransport for Sink {
        fn send(&mut self, payload: &[u8]) -> Result<(), BeamError> {
            self.len = payload.len().min(MAX_FRAME_BYTES);
            self.last[..self.len].copy_from_slice(&payload[..self.len]);
            Ok(())
        }
    }

    #[test]
    fn encode_round_trips_layout() {
        let frame = TelemetryFrame::from_state(1_234, 5_600, -45, InterlockState::Armed, 0xA001);
        let mut buf = [0u8; MAX_FRAME_BYTES];
        let wire = encode_frame(&frame, &mut buf);
        // Header: magic "TPT" + version 0x01, then 2-byte little-endian payload size.
        assert_eq!(&wire[..3], b"TPT");
        assert_eq!(wire[3], 0x01);
        assert_eq!(u16::from_le_bytes([wire[4], wire[5]]), 13);
        // Payload: timestamp(4) + speed(4) + steer(2) + armed(1) + event(2) = 13 bytes.
        assert_eq!(wire.len(), 6 + 13);
        assert_eq!(u32::from_le_bytes(wire[6..10].try_into().unwrap()), 1_234);
        assert_eq!(u32::from_le_bytes(wire[10..14].try_into().unwrap()), 5_600);
        assert_eq!(i16::from_le_bytes(wire[14..16].try_into().unwrap()), -45);
        assert_eq!(wire[16], 1);
        assert_eq!(u16::from_le_bytes(wire[17..19].try_into().unwrap()), 0xA001);
    }

    #[test]
    fn bridge_publishes_well_formed_payload() {
        let mut bridge = FlightControlBridge::new(Sink::default());
        let frame = TelemetryFrame::from_state(9, 100, 7, InterlockState::Disarmed, 0);
        assert_eq!(bridge.publish(&frame), Ok(()));
    }
}
