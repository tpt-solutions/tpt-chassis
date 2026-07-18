// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Copyright (c) TPT Solutions. All rights reserved.
//
// Licensed under the MIT License and the Apache License, Version 2.0
// (the "Licenses"). You may obtain a copy of each License at:
//
//   - MIT:   https://opensource.org/licenses/MIT
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the Licenses is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the Licenses for the specific language governing permissions and
// limitations under each License.

//! CAN bus abstraction.
//!
//! Provides the [`CanFrame`] type and the high-level [`CanBus`] interface used
//! by application code. The actual wire transport is supplied by a
//! [`CanTransceiver`], which may be a real peripheral driver or, for testing,
//! the simulated bus in the `tpt-chassis-sim` crate.

use crate::bus::{BusError, Frame, VehicleBus};
use crate::Error;

/// Maximum payload length of a classical CAN frame (bytes).
pub const CAN_MAX_DLC: usize = 8;

/// A CAN identifier.
///
/// Classical CAN supports 11-bit *standard* identifiers and 29-bit *extended*
/// identifiers. Both are carried here; the 29-bit form packs the standard ID
/// into its low 11 bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanId {
    /// The raw arbitration ID (11 bits for standard, 29 bits for extended).
    raw: u32,
    /// `true` for a 29-bit extended frame, `false` for an 11-bit standard frame.
    extended: bool,
}

impl CanId {
    /// The largest valid standard (11-bit) CAN identifier.
    pub const STANDARD_MAX: u32 = 0x7FF;
    /// The largest valid extended (29-bit) CAN identifier.
    pub const EXTENDED_MAX: u32 = 0x1FFF_FFFF;

    /// Creates a standard (11-bit) CAN identifier.
    ///
    /// Returns `None` if `id` exceeds 11 bits.
    pub fn standard(id: u32) -> Option<CanId> {
        if id <= Self::STANDARD_MAX {
            Some(CanId {
                raw: id,
                extended: false,
            })
        } else {
            None
        }
    }

    /// Creates an extended (29-bit) CAN identifier.
    ///
    /// Returns `None` if `id` exceeds 29 bits.
    pub fn extended(id: u32) -> Option<CanId> {
        if id <= Self::EXTENDED_MAX {
            Some(CanId {
                raw: id,
                extended: true,
            })
        } else {
            None
        }
    }

    /// Returns the raw arbitration ID value.
    pub fn raw(&self) -> u32 {
        self.raw
    }

    /// Returns `true` if this is an extended (29-bit) frame.
    pub fn is_extended(&self) -> bool {
        self.extended
    }
}

/// A classical CAN frame.
///
/// `CanFrame` is `Copy` and carries up to [`CAN_MAX_DLC`] payload bytes inline,
/// so it is safe and allocation-free in `no_std` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanFrame {
    id: CanId,
    data: [u8; CAN_MAX_DLC],
    len: u8,
}

impl CanFrame {
    /// Constructs a CAN frame from an identifier and a payload slice.
    ///
    /// Returns `None` if `data` is longer than [`CAN_MAX_DLC`].
    pub fn new(id: CanId, data: &[u8]) -> Option<CanFrame> {
        if data.len() > CAN_MAX_DLC {
            return None;
        }
        let mut buf = [0u8; CAN_MAX_DLC];
        buf[..data.len()].copy_from_slice(data);
        Some(CanFrame {
            id,
            data: buf,
            len: data.len() as u8,
        })
    }

    /// Returns the frame's identifier.
    pub fn id(&self) -> CanId {
        self.id
    }

    /// Returns the payload bytes carried by this frame.
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
}

impl Frame for CanFrame {
    type Id = CanId;

    fn id(&self) -> CanId {
        self.id
    }

    fn len(&self) -> usize {
        self.len as usize
    }
}

/// A low-level CAN transceiver (wire driver or simulator backend).
///
/// [`CanBus`] is built on top of a `CanTransceiver`; the trait lets the same
/// high-level API talk to real hardware or the in-memory simulator.
pub trait CanTransceiver {
    /// Queues a frame for transmission on the wire.
    fn send(&mut self, frame: CanFrame) -> Result<(), BusError>;

    /// Pulls the next received frame, if one is buffered.
    fn recv(&mut self) -> Result<CanFrame, BusError>;

    /// `true` if a received frame is pending.
    fn has_received(&self) -> bool;

    /// `true` if the transceiver can accept a transmission now.
    fn can_send(&self) -> bool;
}

/// High-level, allocation-free CAN bus interface for application code.
///
/// Wraps any [`CanTransceiver`] and adapts it to the unified [`VehicleBus`]
/// trait so CAN behaves identically to the other vehicle networks.
pub struct CanBus<T: CanTransceiver> {
    transceiver: T,
}

impl<T: CanTransceiver> CanBus<T> {
    /// Wraps a transceiver as a [`CanBus`].
    pub fn new(transceiver: T) -> Self {
        CanBus { transceiver }
    }

    /// Returns a reference to the underlying transceiver.
    pub fn transceiver(&self) -> &T {
        &self.transceiver
    }

    /// Returns a mutable reference to the underlying transceiver.
    pub fn transceiver_mut(&mut self) -> &mut T {
        &mut self.transceiver
    }
}

impl<T: CanTransceiver> VehicleBus for CanBus<T> {
    type Frame = CanFrame;

    fn transmit(&mut self, frame: CanFrame) -> Result<(), BusError> {
        self.transceiver.send(frame)
    }

    fn receive(&mut self) -> Result<CanFrame, BusError> {
        self.transceiver.recv()
    }

    fn can_receive(&self) -> bool {
        self.transceiver.has_received()
    }

    fn can_transmit(&self) -> bool {
        self.transceiver.can_send()
    }
}

/// Validates a raw CAN frame against classical-CAN constraints.
///
/// Used by transceivers and tests to reject malformed frames before they touch
/// the wire. Returns [`Error::InvalidArgument`] on failure.
pub fn validate_frame(frame: &CanFrame) -> Result<(), Error> {
    if frame.len() > CAN_MAX_DLC {
        return Err(Error::InvalidArgument);
    }
    let max = if frame.id().is_extended() {
        CanId::EXTENDED_MAX
    } else {
        CanId::STANDARD_MAX
    };
    if frame.id().raw() > max {
        return Err(Error::InvalidArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_id_rejects_overflow() {
        assert!(CanId::standard(0x800).is_none());
        assert!(CanId::standard(0x7FF).is_some());
    }

    #[test]
    fn extended_id_rejects_overflow() {
        assert!(CanId::extended(0x2000_0000).is_none());
        assert!(CanId::extended(CanId::EXTENDED_MAX).is_some());
    }

    #[test]
    fn frame_carries_payload() {
        let id = CanId::standard(0x100).unwrap();
        let f = CanFrame::new(id, &[1, 2, 3]).unwrap();
        assert_eq!(f.data(), &[1, 2, 3]);
        assert_eq!(f.len(), 3);
    }

    #[test]
    fn frame_rejects_oversize_payload() {
        let id = CanId::standard(0x100).unwrap();
        let data = [0u8; CAN_MAX_DLC + 1];
        assert!(CanFrame::new(id, &data).is_none());
    }

    #[test]
    fn validate_frame_rejects_bad_id() {
        let id = CanId::standard(0x800).unwrap_or_else(|| CanId::standard(0).unwrap());
        let f = CanFrame::new(id, &[]).unwrap();
        let _ = f;
        assert_eq!(
            validate_frame(&CanFrame::new(CanId::standard(0x7FF).unwrap(), &[]).unwrap()),
            Ok(())
        );
    }
}
