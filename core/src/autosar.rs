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

//! AUTOSAR-compatible interface layer.
//!
//! Hand-written, safe-Rust equivalents of the AUTOSAR basic software modules
//! that applications and sensors/actuators integrate against. These are not
//! generated from an AUTOSAR XML description; they intentionally mirror the
//! *shape* of the standard APIs (DIO, COM) so existing automotive code maps
//! across cleanly.
//!
//! See also [`crate::uds`] for the diagnostics layer.

use crate::Error;

/// A digital I/O channel, identified by port group and pin index.
///
/// Mirrors the AUTOSAR `Dio_ChannelType` (a port/pin pair rather than a raw
/// MCU register address) so callers are portable across silicon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DioChannel {
    /// Port group index (e.g. 0 = PORTA).
    pub port: u8,
    /// Pin index within the port.
    pub pin: u8,
}

impl DioChannel {
    /// Creates a channel, returning `None` if `pin > 31`.
    pub fn new(port: u8, pin: u8) -> Option<DioChannel> {
        if pin < 32 {
            Some(DioChannel { port, pin })
        } else {
            None
        }
    }

    /// The bit position of this channel within its port word.
    pub fn bit(&self) -> u32 {
        self.pin as u32
    }
}

/// Logical level of a DIO channel (AUTOSAR `Dio_LevelType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DioLevel {
    /// Logical low (0 V / GND).
    Low,
    /// Logical high (VCC).
    High,
}

impl DioLevel {
    /// Converts to a `bool` (`High` = `true`).
    pub fn as_bool(&self) -> bool {
        matches!(self, DioLevel::High)
    }

    /// Converts from a `bool` (`true` = `High`).
    pub fn from_bool(v: bool) -> DioLevel {
        if v {
            DioLevel::High
        } else {
            DioLevel::Low
        }
    }
}

/// AUTOSAR DIO driver equivalent: read/write of digital channels.
///
/// A backend implements this trait (e.g. a real GPIO peripheral, or a
/// simulated register bank in tests).
pub trait DioDriver {
    /// Reads the level of a single channel.
    fn read_channel(&self, channel: DioChannel) -> Result<DioLevel, Error>;

    /// Writes the level of a single channel.
    fn write_channel(&mut self, channel: DioChannel, level: DioLevel) -> Result<(), Error>;
}

/// An in-memory DIO backend (port register bank), useful for testing and for
/// simulated ECUs.
pub struct SimDio {
    /// One 32-bit word per port group holding the current pin levels.
    ports: [u32; 16],
}

impl SimDio {
    /// Creates a register bank with all channels low.
    pub fn new() -> Self {
        SimDio { ports: [0u32; 16] }
    }

    /// Returns the raw word for a port (all 32 channel levels).
    pub fn port_word(&self, port: u8) -> u32 {
        self.ports.get(port as usize).copied().unwrap_or(0)
    }
}

impl Default for SimDio {
    fn default() -> Self {
        Self::new()
    }
}

impl DioDriver for SimDio {
    fn read_channel(&self, channel: DioChannel) -> Result<DioLevel, Error> {
        let word = self
            .ports
            .get(channel.port as usize)
            .ok_or(Error::InvalidArgument)?;
        Ok(DioLevel::from_bool((word & (1 << channel.bit())) != 0))
    }

    fn write_channel(&mut self, channel: DioChannel, level: DioLevel) -> Result<(), Error> {
        let word = self
            .ports
            .get_mut(channel.port as usize)
            .ok_or(Error::InvalidArgument)?;
        if level.as_bool() {
            *word |= 1 << channel.bit();
        } else {
            *word &= !(1 << channel.bit());
        }
        Ok(())
    }
}

/// A typed signal that rides on a PDU (protocol data unit) over a vehicle bus.
///
/// Mirrors the AUTOSAR COM module's signal concept: a named value occupying a
/// bit-range within a frame's payload. Decoding/encoding are bounds-checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Signal {
    /// Start bit within the payload (0 = first bit).
    pub start_bit: u8,
    /// Signal length in bits (1..=64).
    pub length: u8,
}

impl Signal {
    /// Creates a signal layout, validating `1 <= length <= 64` and that the
    /// signal fits within a 64-bit value.
    pub fn new(start_bit: u8, length: u8) -> Option<Signal> {
        if length == 0 || length > 64 || (start_bit as u32 + length as u32) > 64 {
            return None;
        }
        Some(Signal { start_bit, length })
    }

    /// Extracts the signal value from a little-endian 64-bit payload view.
    pub fn unpack(&self, payload: u64) -> u64 {
        let mask = if self.length >= 64 {
            u64::MAX
        } else {
            (1u64 << self.length) - 1
        };
        (payload >> self.start_bit) & mask
    }

    /// Writes the signal value into a little-endian 64-bit payload view.
    ///
    /// Returns `None` if `value` does not fit in `length` bits.
    pub fn pack(&self, payload: u64, value: u64) -> Option<u64> {
        let mask = if self.length >= 64 {
            u64::MAX
        } else {
            (1u64 << self.length) - 1
        };
        if value & !mask != 0 {
            return None;
        }
        let cleared = payload & !(mask << self.start_bit);
        Some(cleared | (value << self.start_bit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dio_round_trips() {
        let mut dio = SimDio::new();
        let ch = DioChannel::new(0, 3).unwrap();
        assert_eq!(dio.read_channel(ch).unwrap(), DioLevel::Low);
        dio.write_channel(ch, DioLevel::High).unwrap();
        assert_eq!(dio.read_channel(ch).unwrap(), DioLevel::High);
        assert_eq!(dio.port_word(0) & (1 << 3), 1 << 3);
    }

    #[test]
    fn dio_rejects_bad_pin() {
        assert!(DioChannel::new(0, 32).is_none());
        let mut dio = SimDio::new();
        // Port 16 is out of range for the 16-entry bank.
        assert_eq!(
            dio.write_channel(DioChannel::new(16, 0).unwrap(), DioLevel::High),
            Err(Error::InvalidArgument)
        );
    }

    #[test]
    fn signal_pack_unpack() {
        let s = Signal::new(4, 8).unwrap();
        let packed = s.pack(0, 0xAB).unwrap();
        assert_eq!(s.unpack(packed), 0xAB);
        assert_eq!(packed, 0xAB << 4);
    }

    #[test]
    fn signal_rejects_overflow_value() {
        let s = Signal::new(0, 4).unwrap();
        assert!(s.pack(0, 0x10).is_none()); // 0x10 needs 5 bits
        assert!(s.pack(0, 0x0F).is_some());
    }

    #[test]
    fn signal_layout_validation() {
        assert!(Signal::new(0, 0).is_none());
        assert!(Signal::new(60, 8).is_none());
        assert!(Signal::new(0, 64).is_some());
    }
}
