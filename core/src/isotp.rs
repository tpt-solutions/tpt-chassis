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

//! ISO-TP (ISO 15765-2) transport layer for CAN.
//!
//! CAN frames carry at most 8 payload bytes, far too small for most UDS
//! requests/responses (e.g. large `ReadDataByIdentifier` or firmware chunks).
//! ISO-TP wraps a logical message across one or more CAN frames:
//!
//! - **Single Frame (SF)** — message fits in 7 bytes (PCI type `0`).
//! - **First Frame (FF) + Consecutive Frames (CF)** — message split across
//!   frames (PCI types `1`/`2`), with a 12-bit total length in the FF.
//! - **Flow Control (FC)** — the receiver answers a FF with `0x30` to grant
//!   transmission (used here in "clear to send" form).
//!
//! [`IsoTpLink`] reassembles incoming frames into a complete message and reports
//! when a flow-control frame must be sent; [`encode_message`] segments an
//! outgoing message into CAN frames. The layer is `no_std`, allocation-free, and
//! buffer-sized via a `const` generic.

use crate::Error;

/// Maximum data bytes in a Single Frame (8-byte CAN frame minus 1 PCI byte).
pub const ISOTP_SF_MAX: usize = 7;
/// Maximum data bytes in a First Frame (8-byte CAN frame minus 2 PCI bytes).
pub const ISOTP_FF_MAX: usize = 6;
/// Maximum data bytes in a Consecutive Frame (8-byte CAN frame minus 1 PCI byte).
pub const ISOTP_CF_MAX: usize = 7;
/// Maximum ISO-TP message length (12-bit length field).
pub const ISOTP_MAX_MESSAGE: usize = 4095;

/// Errors from ISO-TP segmentation / reassembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum IsoTpError {
    /// A generic core error.
    Core(Error),
    /// The message/segment exceeds the configured buffer capacity.
    Overflow,
    /// A frame arrived out of sequence (wrong consecutive-frame SN).
    WrongSequence,
    /// A consecutive/first frame arrived without a preceding first frame.
    UnexpectedFrame,
    /// The frame's PCI type is unknown.
    InvalidPci,
    /// The output frame buffer was too small to hold the segmented message.
    BufferTooSmall,
}

impl From<Error> for IsoTpError {
    fn from(e: Error) -> Self {
        IsoTpError::Core(e)
    }
}

/// Result of feeding one CAN frame into an [`IsoTpLink`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsoTpEvent<'a, const N: usize> {
    /// More frames are needed to complete the message.
    Incomplete,
    /// A complete message is ready (borrowed from the link's reassembly buffer).
    Complete(&'a [u8]),
    /// A First Frame was received; the caller must transmit a flow-control frame
    /// (see [`flow_control_frame`]) before the sender continues.
    NeedFlowControl,
}

/// A reassembly state machine for one ISO-TP conversation.
///
/// `N` is the maximum message size the link can buffer. Use the same `N` on both
/// ends of a conversation.
pub struct IsoTpLink<const N: usize> {
    buf: [u8; N],
    total: usize,
    filled: usize,
    sn: u8,
    receiving: bool,
}

impl<const N: usize> IsoTpLink<N> {
    /// Creates an idle link.
    pub fn new() -> Self {
        IsoTpLink {
            buf: [0u8; N],
            total: 0,
            filled: 0,
            sn: 0,
            receiving: false,
        }
    }

    /// Feeds one received CAN frame (its 8-byte payload) into the link.
    pub fn feed(&mut self, frame: &[u8]) -> Result<IsoTpEvent<'_, N>, IsoTpError> {
        let first = *frame.first().ok_or(IsoTpError::InvalidPci)?;
        let pci = first >> 4;
        match pci {
            0 => {
                // Single Frame: lower nibble is the data length.
                let len = (first & 0x0F) as usize;
                if len == 0 || len > ISOTP_SF_MAX || len > frame.len() - 1 {
                    return Err(IsoTpError::InvalidPci);
                }
                self.buf[..len].copy_from_slice(&frame[1..1 + len]);
                self.receiving = false;
                Ok(IsoTpEvent::Complete(&self.buf[..len]))
            }
            1 => {
                // First Frame: 12-bit total length, then up to 6 data bytes.
                let len =
                    (((first & 0x0F) as usize) << 8) | frame.get(1).copied().unwrap_or(0) as usize;
                if len < ISOTP_FF_MAX + 1 || len > N {
                    return Err(IsoTpError::Overflow);
                }
                let data = &frame[2..];
                self.buf[..data.len()].copy_from_slice(data);
                self.filled = data.len();
                self.total = len;
                self.sn = 1;
                self.receiving = true;
                Ok(IsoTpEvent::NeedFlowControl)
            }
            2 => {
                // Consecutive Frame: lower nibble is the sequence number.
                if !self.receiving {
                    return Err(IsoTpError::UnexpectedFrame);
                }
                let sn = first & 0x0F;
                if sn != self.sn {
                    return Err(IsoTpError::WrongSequence);
                }
                self.sn = (self.sn + 1) & 0x0F;
                let data = &frame[1..];
                if self.filled + data.len() > self.total {
                    return Err(IsoTpError::Overflow);
                }
                self.buf[self.filled..self.filled + data.len()].copy_from_slice(data);
                self.filled += data.len();
                if self.filled >= self.total {
                    self.receiving = false;
                    Ok(IsoTpEvent::Complete(&self.buf[..self.total]))
                } else {
                    Ok(IsoTpEvent::Incomplete)
                }
            }
            _ => Err(IsoTpError::InvalidPci),
        }
    }
}

impl<const N: usize> Default for IsoTpLink<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a flow-control "clear to send" frame (PCI `0x30`).
///
/// `block_size` = 0 means "send all frames"; `st_min` is the minimum separation
/// time in ms (0 = none). The returned 8-byte slice is the CAN payload.
pub fn flow_control_frame(block_size: u8, st_min: u8) -> [u8; 8] {
    [0x30, block_size, st_min, 0, 0, 0, 0, 0]
}

/// Segments a logical message into CAN frames written into `out`.
///
/// Returns the number of frames produced, or [`IsoTpError::BufferTooSmall`] if
/// `out` cannot hold them all. Each frame is the 8-byte CAN payload.
pub fn encode_message(msg: &[u8], out: &mut [[u8; 8]]) -> Result<usize, IsoTpError> {
    if msg.len() > ISOTP_MAX_MESSAGE {
        return Err(IsoTpError::Overflow);
    }
    if msg.len() <= ISOTP_SF_MAX {
        if out.is_empty() {
            return Err(IsoTpError::BufferTooSmall);
        }
        let mut f = [0u8; 8];
        f[0] = msg.len() as u8; // PCI type 0, length in low nibble
        f[1..1 + msg.len()].copy_from_slice(msg);
        out[0] = f;
        return Ok(1);
    }
    // First frame: 12-bit length across bytes 0/1, then up to 6 data bytes.
    let ff_len = msg.len().min(ISOTP_FF_MAX);
    if out.is_empty() {
        return Err(IsoTpError::BufferTooSmall);
    }
    let mut f = [0u8; 8];
    f[0] = 0x10 | ((msg.len() >> 8) as u8 & 0x0F);
    f[1] = (msg.len() & 0xFF) as u8;
    f[2..2 + ff_len].copy_from_slice(&msg[..ff_len]);
    out[0] = f;

    let mut idx = ff_len;
    let mut sn: u8 = 1;
    let mut count = 1;
    while idx < msg.len() {
        if count >= out.len() {
            return Err(IsoTpError::BufferTooSmall);
        }
        let chunk = (msg.len() - idx).min(ISOTP_CF_MAX);
        let mut cf = [0u8; 8];
        cf[0] = 0x20 | sn;
        cf[1..1 + chunk].copy_from_slice(&msg[idx..idx + chunk]);
        out[count] = cf;
        idx += chunk;
        sn = (sn + 1) & 0x0F;
        count += 1;
    }
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_frame_round_trips() {
        let msg = [0x22, 0xF1, 0x90];
        let mut frames = [[0u8; 8]; 8];
        let n = encode_message(&msg, &mut frames).unwrap();
        assert_eq!(n, 1);

        let mut link: IsoTpLink<64> = IsoTpLink::new();
        match link.feed(&frames[0]).unwrap() {
            IsoTpEvent::Complete(got) => assert_eq!(got, &msg),
            _ => panic!("expected complete"),
        }
    }

    #[test]
    fn multi_frame_reassembles() {
        // 20-byte message -> FF (6 bytes) + 2 CF (7 + 7).
        let msg: [u8; 20] = core::array::from_fn(|i| i as u8);
        let mut frames = [[0u8; 8]; 8];
        let n = encode_message(&msg, &mut frames).unwrap();
        assert_eq!(n, 3);

        let mut link: IsoTpLink<64> = IsoTpLink::new();
        // FF -> need flow control.
        match link.feed(&frames[0]).unwrap() {
            IsoTpEvent::NeedFlowControl => {}
            other => panic!("expected NeedFlowControl, got {:?}", other),
        }
        // Sender would expect a flow-control frame here.
        let _fc = flow_control_frame(0, 0);
        // CF #1 (SN=1).
        match link.feed(&frames[1]).unwrap() {
            IsoTpEvent::Incomplete => {}
            other => panic!("expected Incomplete, got {:?}", other),
        }
        // CF #2 (SN=2).
        match link.feed(&frames[2]).unwrap() {
            IsoTpEvent::Complete(got) => assert_eq!(got, &msg),
            other => panic!("expected Complete, got {:?}", other),
        }
    }

    #[test]
    fn wrong_sequence_number_is_rejected() {
        let msg: [u8; 20] = core::array::from_fn(|i| i as u8);
        let mut frames = [[0u8; 8]; 8];
        encode_message(&msg, &mut frames).unwrap();

        let mut link: IsoTpLink<64> = IsoTpLink::new();
        let _ = link.feed(&frames[0]).unwrap(); // FF
                                                // Corrupt the sequence number of the first CF to 5 (expected 1).
        let mut bad = frames[1];
        bad[0] = 0x25;
        assert_eq!(link.feed(&bad), Err(IsoTpError::WrongSequence));
    }

    #[test]
    fn consecutive_without_first_is_rejected() {
        let mut link: IsoTpLink<64> = IsoTpLink::new();
        let cf = [0x21, 1, 2, 3, 4, 5, 6, 7];
        assert_eq!(link.feed(&cf), Err(IsoTpError::UnexpectedFrame));
    }
}
