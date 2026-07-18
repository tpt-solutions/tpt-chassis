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

//! LIN bus abstraction.
//!
//! LIN (Local Interconnect Network) is a low-cost, master-slave sub-bus used
//! for non-safety-critical body electronics. This module provides the
//! [`LinFrame`] type, a [`LinTransceiver`] trait, and a [`LinBus`] high-level
//! interface that implements the unified [`crate::bus::VehicleBus`] trait so
//! LIN behaves like CAN and Ethernet from the caller's point of view.

use crate::bus::{BusError, Frame, VehicleBus};
use crate::Error;

/// Maps a frame-validation failure onto the bus transport enum.
fn invalid_frame_err(e: Error) -> BusError {
    match e {
        Error::InvalidArgument => BusError::InvalidFrame,
        other => BusError::Core(other),
    }
}

/// Maximum payload length of a LIN frame (bytes).
pub const LIN_MAX_DLC: usize = 8;

/// A LIN identifier (protected identifier, 0..=0x3F).
///
/// LIN frame IDs occupy 6 bits. This type stores the raw unprotected ID; the
/// parity/protected-ID computation is provided by [`LinId::protected`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinId(u8);

impl LinId {
    /// The largest valid LIN frame identifier.
    pub const MAX: u8 = 0x3F;

    /// Creates a LIN identifier, returning `None` if `id > 0x3F`.
    pub fn new(id: u8) -> Option<LinId> {
        if id <= Self::MAX {
            Some(LinId(id))
        } else {
            None
        }
    }

    /// Returns the raw (unprotected) identifier value.
    pub fn raw(&self) -> u8 {
        self.0
    }

    /// Computes the protected identifier (PID) including parity bits,
    /// per the LIN 2.x specification.
    ///
    /// `P0 = ID0 ^ ID1 ^ ID2 ^ ID4` (bit 6), `P1 = !(ID1 ^ ID3 ^ ID4 ^ ID5)`
    /// (bit 7).
    pub fn protected(&self) -> u8 {
        let id = self.0;
        let p0 = (((id) ^ (id >> 1) ^ (id >> 2) ^ (id >> 4)) & 0x01) << 6;
        let p1 = (!((id >> 1) ^ (id >> 3) ^ (id >> 4) ^ (id >> 5)) & 0x01) << 7;
        id | p0 | p1
    }
}

/// A LIN frame (unprotected ID + up to [`LIN_MAX_DLC`] data bytes).
///
/// `LinFrame` is `Copy` so it can be broadcast without allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinFrame {
    id: LinId,
    data: [u8; LIN_MAX_DLC],
    len: u8,
}

impl LinFrame {
    /// Constructs a LIN frame, returning `None` if `id` is invalid or `data`
    /// exceeds [`LIN_MAX_DLC`].
    pub fn new(id: LinId, data: &[u8]) -> Option<LinFrame> {
        if data.len() > LIN_MAX_DLC {
            return None;
        }
        let mut buf = [0u8; LIN_MAX_DLC];
        buf[..data.len()].copy_from_slice(data);
        Some(LinFrame {
            id,
            data: buf,
            len: data.len() as u8,
        })
    }

    /// Returns the frame identifier.
    pub fn id(&self) -> LinId {
        self.id
    }

    /// Returns the payload bytes.
    pub fn data(&self) -> &[u8] {
        &self.data[..self.len as usize]
    }

    /// Computes the classic LIN checksum (classic = no ID included).
    pub fn checksum(&self) -> u8 {
        let mut sum: u16 = 0;
        for b in self.data() {
            sum += *b as u16;
            if sum > 0xFF {
                sum -= 0xFF;
            }
        }
        !(sum as u8)
    }

    /// Constructs a `LinFrame` from raw parts without validation.
    ///
    /// Test-only helper to exercise [`validate_frame`] / `transmit` on
    /// out-of-range inputs that the public constructors reject.
    #[cfg(test)]
    pub(crate) fn from_raw(id: LinId, data: [u8; LIN_MAX_DLC], len: u8) -> LinFrame {
        LinFrame { id, data, len }
    }
}

impl Frame for LinFrame {
    type Id = LinId;

    fn id(&self) -> LinId {
        self.id
    }

    fn len(&self) -> usize {
        self.len as usize
    }
}

/// Low-level LIN transceiver (driver or simulator backend).
///
/// LIN is master/slave: only the master may initiate a frame header; slaves
/// respond with the data. The transceiver models this by exposing
/// [`LinTransceiver::is_master`].
pub trait LinTransceiver {
    /// `true` for a master node (may schedule/transmit headers).
    fn is_master(&self) -> bool;

    /// Sends a LIN frame (master publishes data, slave responds to a header).
    fn send(&mut self, frame: LinFrame) -> Result<(), BusError>;

    /// Receives the next buffered frame, if any.
    fn recv(&mut self) -> Result<LinFrame, BusError>;

    /// `true` if a frame is pending receipt.
    fn has_received(&self) -> bool;

    /// `true` if the transceiver can transmit now.
    fn can_send(&self) -> bool;
}

/// High-level LIN interface implementing the unified [`VehicleBus`] trait.
pub struct LinBus<T: LinTransceiver> {
    transceiver: T,
}

impl<T: LinTransceiver> LinBus<T> {
    /// Wraps a [`LinTransceiver`] as a [`LinBus`].
    pub fn new(transceiver: T) -> Self {
        LinBus { transceiver }
    }

    /// Returns a reference to the underlying transceiver.
    pub fn transceiver(&self) -> &T {
        &self.transceiver
    }

    /// Returns a mutable reference to the underlying transceiver.
    pub fn transceiver_mut(&mut self) -> &mut T {
        &mut self.transceiver
    }

    /// Returns `true` if the underlying node is the LIN master.
    pub fn is_master(&self) -> bool {
        self.transceiver.is_master()
    }
}

impl<T: LinTransceiver> VehicleBus for LinBus<T> {
    type Frame = LinFrame;

    fn transmit(&mut self, frame: LinFrame) -> Result<(), BusError> {
        validate_frame(&frame).map_err(invalid_frame_err)?;
        self.transceiver.send(frame)
    }

    fn receive(&mut self) -> Result<LinFrame, BusError> {
        self.transceiver.recv()
    }

    fn can_receive(&self) -> bool {
        self.transceiver.has_received()
    }

    fn can_transmit(&self) -> bool {
        self.transceiver.can_send()
    }
}

/// Validates a LIN frame against LIN constraints.
pub fn validate_frame(frame: &LinFrame) -> Result<(), Error> {
    if frame.len() > LIN_MAX_DLC {
        return Err(Error::InvalidArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lin_id_rejects_overflow() {
        assert!(LinId::new(0x40).is_none());
        assert!(LinId::new(0x3F).is_some());
    }

    #[test]
    fn protected_pid_matches_known_values() {
        // Verified by hand from the LIN 2.1 parity formulas:
        // P0 = ID0 ^ ID1 ^ ID2 ^ ID4 (bit6), P1 = !(ID1 ^ ID3 ^ ID4 ^ ID5) (bit7).
        assert_eq!(LinId::new(0x00).unwrap().protected(), 0x80);
        assert_eq!(LinId::new(0x01).unwrap().protected(), 0xC1);
        assert_eq!(LinId::new(0x20).unwrap().protected(), 0x20);
    }

    #[test]
    fn checksum_is_inverted_modulo_sum() {
        let id = LinId::new(0x10).unwrap();
        let f = LinFrame::new(id, &[0x01, 0x02, 0x03]).unwrap();
        let mut sum: u16 = 0;
        for b in f.data() {
            sum += *b as u16;
            if sum > 0xFF {
                sum -= 0xFF;
            }
        }
        assert_eq!(f.checksum(), !(sum as u8));
    }

    #[test]
    fn frame_rejects_oversize_payload() {
        let id = LinId::new(0x05).unwrap();
        let data = [0u8; LIN_MAX_DLC + 1];
        assert!(LinFrame::new(id, &data).is_none());
    }

    #[test]
    fn validate_frame_ok() {
        let id = LinId::new(0x02).unwrap();
        assert_eq!(validate_frame(&LinFrame::new(id, &[9]).unwrap()), Ok(()));
    }

    /// A transceiver that records whether `send` was invoked.
    struct CountingTransceiver {
        sent: bool,
    }

    impl LinTransceiver for CountingTransceiver {
        fn is_master(&self) -> bool {
            true
        }
        fn send(&mut self, _frame: LinFrame) -> Result<(), BusError> {
            self.sent = true;
            Ok(())
        }
        fn recv(&mut self) -> Result<LinFrame, BusError> {
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
        let mut bus = LinBus::new(CountingTransceiver { sent: false });
        let id = LinId::new(0x05).unwrap();
        let f = LinFrame::from_raw(id, [0u8; LIN_MAX_DLC], (LIN_MAX_DLC + 1) as u8);
        assert_eq!(bus.transmit(f), Err(BusError::InvalidFrame));
        assert!(!bus.transceiver().sent);
    }

    #[test]
    fn transmit_valid_frame_reaches_transceiver() {
        let mut bus = LinBus::new(CountingTransceiver { sent: false });
        let id = LinId::new(0x05).unwrap();
        let f = LinFrame::new(id, &[1, 2, 3]).unwrap();
        assert_eq!(bus.transmit(f), Ok(()));
        assert!(bus.transceiver().sent);
    }
}
