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

//! Unified vehicle bus abstraction.
//!
//! This module defines the transport-agnostic [`VehicleBus`] trait that the
//! rest of TPT Chassis builds on. Today it is implemented for CAN (see
//! [`crate::can`]); later phases extend it to Ethernet (SOME/IP) and LIN
//! without changing the calling code.

use crate::Error;

/// A single frame exchanged over a vehicle bus.
///
/// Implementors represent one protocol-specific frame (e.g. a CAN frame, an
/// Ethernet/SOME/IP message, or a LIN frame). They must be `Copy` so that a
/// frame can be cheaply broadcast to multiple listeners and queued without
/// allocation in `no_std` environments.
pub trait Frame: Copy {
    /// The type used to address the frame on its bus.
    type Id: Copy;

    /// Returns the frame identifier (e.g. CAN arbitration ID).
    fn id(&self) -> Self::Id;

    /// Returns the number of payload bytes carried by the frame.
    fn len(&self) -> usize;

    /// Returns `true` if the frame carries no payload bytes.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Errors specific to bus transport operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum BusError {
    /// A generic core error (e.g. unsupported, resource exhausted).
    Core(Error),
    /// The transmit queue is full and the frame could not be enqueued.
    TxQueueFull,
    /// The receive queue is empty (no frame available to read).
    RxQueueEmpty,
    /// The supplied frame is not valid for this bus (e.g. bad ID or length).
    InvalidFrame,
    /// The bus is in a fault state and cannot currently transmit/receive.
    BusOff,
}

impl From<Error> for BusError {
    fn from(e: Error) -> Self {
        BusError::Core(e)
    }
}

impl core::fmt::Display for BusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BusError::Core(e) => write!(f, "bus: {}", e),
            BusError::TxQueueFull => f.write_str("bus: transmit queue full"),
            BusError::RxQueueEmpty => f.write_str("bus: receive queue empty"),
            BusError::InvalidFrame => f.write_str("bus: invalid frame"),
            BusError::BusOff => f.write_str("bus: bus-off fault state"),
        }
    }
}

/// A vehicle bus transport.
///
/// A [`VehicleBus`] is the unified surface used throughout TPT Chassis. Code
/// that sends or receives frames is written against this trait and therefore
/// works across CAN, Ethernet, and LIN once each has an implementation.
pub trait VehicleBus {
    /// The frame type carried by this bus.
    type Frame: Frame;

    /// Transmits a single frame onto the bus.
    ///
    /// Returns [`BusError::TxQueueFull`] if the implementation cannot accept
    /// the frame at this time (callers should retry).
    fn transmit(&mut self, frame: Self::Frame) -> Result<(), BusError>;

    /// Receives the next frame queued for this node, if any.
    ///
    /// Returns [`BusError::RxQueueEmpty`] when no frame is pending.
    fn receive(&mut self) -> Result<Self::Frame, BusError>;

    /// Returns `true` if at least one frame is waiting to be received.
    fn can_receive(&self) -> bool;

    /// Returns `true` if the bus is ready to accept a transmit.
    fn can_transmit(&self) -> bool;
}
