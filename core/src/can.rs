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

/// Maps a frame-validation failure onto the bus transport enum.
fn invalid_frame_err(e: Error) -> BusError {
    match e {
        Error::InvalidArgument => BusError::InvalidFrame,
        other => BusError::Core(other),
    }
}

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
    pub const EXTENDED_MAX: u32 = 0x1FFFFFFF;

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

    /// Constructs a `CanId` from a raw value without range checking.
    ///
    /// Intended for tests that need to exercise validation on out-of-range IDs;
    /// production code must use [`CanId::standard`] / [`CanId::extended`].
    #[cfg(test)]
    pub(crate) fn from_raw(raw: u32, extended: bool) -> CanId {
        CanId { raw, extended }
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

    /// Constructs a `CanFrame` from raw parts without validation.
    ///
    /// Test-only helper to exercise [`validate_frame`] / `transmit` on
    /// out-of-range inputs that the public constructors reject.
    #[cfg(test)]
    pub(crate) fn from_raw(id: CanId, data: [u8; CAN_MAX_DLC], len: u8) -> CanFrame {
        CanFrame { id, data, len }
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
        validate_frame(&frame).map_err(invalid_frame_err)?;
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
/// Maximum payload length of a CAN FD frame (bytes).
///
/// CAN FD raises the maximum payload from 8 to 64 bytes, enabling larger
/// diagnostic/OTA chunks per frame. The rest of the framing (ID, transceiver
/// model) is unchanged from classical CAN.
pub const CANFD_MAX_DLC: usize = 64;

/// A CAN FD (flexible data-rate) frame.
///
/// Like [`CanFrame`] but carries up to [`CANFD_MAX_DLC`] payload bytes. It is
/// `Copy` and allocation-free, so it is safe to use in `no_std` targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanFdFrame {
    id: CanId,
    data: [u8; CANFD_MAX_DLC],
    len: u8,
}

impl CanFdFrame {
    /// Constructs a CAN FD frame, returning `None` if `data` exceeds
    /// [`CANFD_MAX_DLC`].
    pub fn new(id: CanId, data: &[u8]) -> Option<CanFdFrame> {
        if data.len() > CANFD_MAX_DLC {
            return None;
        }
        let mut buf = [0u8; CANFD_MAX_DLC];
        buf[..data.len()].copy_from_slice(data);
        Some(CanFdFrame {
            id,
            data: buf,
            len: data.len() as u8,
        })
    }

    /// Returns the frame identifier.
    pub fn id(&self) -> CanId {
        self.id
    }

    /// Returns the payload bytes.
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }
}

impl Frame for CanFdFrame {
    type Id = CanId;

    fn id(&self) -> CanId {
        self.id
    }

    fn len(&self) -> usize {
        self.len as usize
    }
}

/// Low-level CAN FD transceiver (wire driver or simulator backend).
pub trait FdCanTransceiver {
    /// Queues a CAN FD frame for transmission on the wire.
    fn send(&mut self, frame: CanFdFrame) -> Result<(), BusError>;

    /// Pulls the next received CAN FD frame, if one is buffered.
    fn recv(&mut self) -> Result<CanFdFrame, BusError>;

    /// `true` if a received frame is pending.
    fn has_received(&self) -> bool;

    /// `true` if the transceiver can accept a transmission now.
    fn can_send(&self) -> bool;
}

/// High-level CAN FD bus interface implementing the unified [`VehicleBus`].
pub struct CanFdBus<T: FdCanTransceiver> {
    transceiver: T,
}

impl<T: FdCanTransceiver> CanFdBus<T> {
    /// Wraps a CAN FD transceiver as a [`CanFdBus`].
    pub fn new(transceiver: T) -> Self {
        CanFdBus { transceiver }
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

impl<T: FdCanTransceiver> VehicleBus for CanFdBus<T> {
    type Frame = CanFdFrame;

    fn transmit(&mut self, frame: CanFdFrame) -> Result<(), BusError> {
        validate_fd_frame(&frame)?;
        self.transceiver.send(frame)
    }

    fn receive(&mut self) -> Result<CanFdFrame, BusError> {
        self.transceiver.recv()
    }

    fn can_receive(&self) -> bool {
        self.transceiver.has_received()
    }

    fn can_transmit(&self) -> bool {
        self.transceiver.can_send()
    }
}

/// Validates a CAN FD frame against protocol constraints.
///
/// Mirrors [`validate_frame`] but permits up to [`CANFD_MAX_DLC`] payload bytes.
pub fn validate_fd_frame(frame: &CanFdFrame) -> Result<(), BusError> {
    if frame.len() > CANFD_MAX_DLC {
        return Err(BusError::InvalidFrame);
    }
    let max = if frame.id().is_extended() {
        CanId::EXTENDED_MAX
    } else {
        CanId::STANDARD_MAX
    };
    if frame.id().raw() > max {
        return Err(BusError::InvalidFrame);
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
    fn extended_id_accepts_max() {
        // 29-bit CAN IDs max out at 0x1FFFFFFF (0x1FFF_FFFF was off by one).
        assert!(CanId::extended(0x1FFFFFFF).is_some());
        assert!(CanId::extended(0x20000000).is_none());
        assert_eq!(CanId::EXTENDED_MAX, 0x1FFFFFFF);
    }

    #[test]
    fn validate_frame_rejects_overrange_id() {
        let id = CanId::from_raw(0x2000_0000, true);
        let f = CanFrame::from_raw(id, [0u8; CAN_MAX_DLC], 0);
        assert_eq!(validate_frame(&f), Err(Error::InvalidArgument));
    }

    /// A transceiver that records whether `send` was invoked.
    struct CountingTransceiver {
        sent: bool,
    }

    impl CanTransceiver for CountingTransceiver {
        fn send(&mut self, _frame: CanFrame) -> Result<(), BusError> {
            self.sent = true;
            Ok(())
        }
        fn recv(&mut self) -> Result<CanFrame, BusError> {
            Err(BusError::RxQueueEmpty)
        }
        fn has_received(&self) -> bool {
            false
        }
        fn can_send(&self) -> bool {
            true
        }
    }

    #[test]
    fn transmit_invalid_frame_errors_before_send() {
        let mut bus = CanBus::new(CountingTransceiver { sent: false });
        let id = CanId::from_raw(0x2000_0000, true);
        let f = CanFrame::from_raw(id, [0u8; CAN_MAX_DLC], 0);
        assert_eq!(bus.transmit(f), Err(BusError::InvalidFrame));
        assert!(!bus.transceiver().sent);
    }

    #[test]
    fn transmit_valid_frame_reaches_transceiver() {
        let mut bus = CanBus::new(CountingTransceiver { sent: false });
        let id = CanId::extended(0x18DAF100).unwrap();
        let f = CanFrame::new(id, &[1, 2, 3]).unwrap();
        assert_eq!(bus.transmit(f), Ok(()));
        assert!(bus.transceiver().sent);
    }

    #[test]
    fn canfd_frame_carries_up_to_64_bytes() {
        let id = CanId::extended(0x18DAF100).unwrap();
        let data = [0x5A; CANFD_MAX_DLC];
        let f = CanFdFrame::new(id, &data).unwrap();
        assert_eq!(f.data().len(), CANFD_MAX_DLC);
        assert_eq!(f.data(), &data[..]);
    }

    #[test]
    fn canfd_rejects_oversize_payload() {
        let id = CanId::standard(0x100).unwrap();
        let data = [0u8; CANFD_MAX_DLC + 1];
        assert!(CanFdFrame::new(id, &data).is_none());
    }

    struct FdCountingTransceiver {
        sent: bool,
    }

    impl FdCanTransceiver for FdCountingTransceiver {
        fn send(&mut self, _frame: CanFdFrame) -> Result<(), BusError> {
            self.sent = true;
            Ok(())
        }
        fn recv(&mut self) -> Result<CanFdFrame, BusError> {
            Err(BusError::RxQueueEmpty)
        }
        fn has_received(&self) -> bool {
            false
        }
        fn can_send(&self) -> bool {
            true
        }
    }

    #[test]
    fn canfd_transmit_invalid_frame_errors() {
        let mut bus = CanFdBus::new(FdCountingTransceiver { sent: false });
        let id = CanId::from_raw(0x2000_0000, true);
        let f = CanFdFrame::new(id, &[0u8; CANFD_MAX_DLC]).unwrap();
        assert_eq!(bus.transmit(f), Err(BusError::InvalidFrame));
        assert!(!bus.transceiver().sent);
    }

    #[test]
    fn canfd_transmit_valid_frame_reaches_transceiver() {
        let mut bus = CanFdBus::new(FdCountingTransceiver { sent: false });
        let id = CanId::standard(0x200).unwrap();
        let f = CanFdFrame::new(id, &[1, 2, 3, 4]).unwrap();
        assert_eq!(bus.transmit(f), Ok(()));
        assert!(bus.transceiver().sent);
    }
}
