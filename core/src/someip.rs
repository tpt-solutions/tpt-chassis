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

//! SOME/IP protocol support over Ethernet.
//!
//! SOME/IP (Scalable service-Oriented MiddlewarE over IP) is the automotive
//! Ethernet middleware used for service-oriented communication. This module
//! implements the SOME/IP header ([`SomeIpHeader`]), message container
//! ([`SomeIpMessage`]), a [`SomeIpTransceiver`] backend trait, and a
//! [`SomeIpBus`] wrapper that implements the unified [`crate::bus::VehicleBus`]
//! trait — so SOME/IP rides the same calling convention as CAN and LIN.

use crate::bus::{BusError, Frame, VehicleBus};
use crate::Error;

/// SOME/IP protocol version (fixed at 1).
pub const SOMEIP_PROTOCOL_VERSION: u8 = 0x01;

/// Size of the fixed SOME/IP header in bytes.
pub const SOMEIP_HEADER_LEN: usize = 16;

/// Maximum SOME/IP payload length in bytes (restricts a single message to fit
/// typical automotive Ethernet MTUs without allocation).
pub const SOMEIP_MAX_PAYLOAD: usize = 1024;

/// SOME/IP message types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SomeIpMessageType {
    /// Request (client -> server, expects response).
    Request,
    /// Request (fire & forget, no response expected).
    RequestNoReturn,
    /// Notification (server -> client event).
    Notification,
    /// Response (server -> client).
    Response,
    /// Error response.
    Error,
}

impl SomeIpMessageType {
    /// Encodes the message type as its wire value.
    pub fn to_u8(self) -> u8 {
        match self {
            SomeIpMessageType::Request => 0x00,
            SomeIpMessageType::RequestNoReturn => 0x01,
            SomeIpMessageType::Notification => 0x02,
            SomeIpMessageType::Response => 0x80,
            SomeIpMessageType::Error => 0x81,
        }
    }

    /// Decodes a wire value, returning `None` for unknown types.
    pub fn from_u8(v: u8) -> Option<SomeIpMessageType> {
        match v {
            0x00 => Some(SomeIpMessageType::Request),
            0x01 => Some(SomeIpMessageType::RequestNoReturn),
            0x02 => Some(SomeIpMessageType::Notification),
            0x80 => Some(SomeIpMessageType::Response),
            0x81 => Some(SomeIpMessageType::Error),
            _ => None,
        }
    }
}

/// SOME/IP return codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum SomeIpReturnCode {
    /// Success.
    Ok,
    /// Unknown service / method.
    NotOk,
    /// Wrong protocol version.
    WrongProtocolVersion,
}

impl SomeIpReturnCode {
    /// Encodes the return code as its wire value.
    pub fn to_u8(self) -> u8 {
        match self {
            SomeIpReturnCode::Ok => 0x00,
            SomeIpReturnCode::NotOk => 0x01,
            SomeIpReturnCode::WrongProtocolVersion => 0x02,
        }
    }

    /// Decodes a wire value, returning `None` for unknown codes.
    pub fn from_u8(v: u8) -> Option<SomeIpReturnCode> {
        match v {
            0x00 => Some(SomeIpReturnCode::Ok),
            0x01 => Some(SomeIpReturnCode::NotOk),
            0x02 => Some(SomeIpReturnCode::WrongProtocolVersion),
            _ => None,
        }
    }
}

/// A SOME/IP message identifier (Service ID + Method/Event ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SomeIpMessageId {
    /// Service identifier (16 bits).
    pub service: u16,
    /// Method or event identifier (16 bits).
    pub method: u16,
}

impl SomeIpMessageId {
    /// Packs the message ID into its 32-bit wire representation.
    pub fn to_u32(self) -> u32 {
        ((self.service as u32) << 16) | (self.method as u32)
    }

    /// Unpacks a 32-bit wire value into a message ID.
    pub fn from_u32(v: u32) -> Self {
        SomeIpMessageId {
            service: (v >> 16) as u16,
            method: (v & 0xFFFF) as u16,
        }
    }
}

/// A SOME/IP request identifier (Client ID + Session ID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SomeIpRequestId {
    /// Client identifier (16 bits).
    pub client: u16,
    /// Session identifier (16 bits), incremented per request.
    pub session: u16,
}

impl SomeIpRequestId {
    /// Packs the request ID into its 32-bit wire representation.
    pub fn to_u32(self) -> u32 {
        ((self.client as u32) << 16) | (self.session as u32)
    }

    /// Unpacks a 32-bit wire value into a request ID.
    pub fn from_u32(v: u32) -> Self {
        SomeIpRequestId {
            client: (v >> 16) as u16,
            session: (v & 0xFFFF) as u16,
        }
    }
}

/// A SOME/IP header (fixed 16-byte layout, big-endian on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SomeIpHeader {
    /// Message ID (service + method/event).
    pub message_id: SomeIpMessageId,
    /// Request ID (client + session).
    pub request_id: SomeIpRequestId,
    /// Interface version.
    pub interface_version: u8,
    /// Message type.
    pub message_type: SomeIpMessageType,
    /// Return code.
    pub return_code: SomeIpReturnCode,
}

impl SomeIpHeader {
    /// Returns the on-wire length field for a payload of `payload_len` bytes.
    ///
    /// Per SOME/IP, `Length` covers everything after the Message ID: the
    /// Request ID (4) + the 4 header bytes after it + payload.
    pub fn length_field(payload_len: usize) -> u32 {
        4 + 4 + payload_len as u32
    }

    /// Encodes the header into `out` (must be at least [`SOMEIP_HEADER_LEN`]).
    ///
    /// Returns `None` if `out` is too small.
    pub fn encode(&self, out: &mut [u8]) -> Option<()> {
        if out.len() < SOMEIP_HEADER_LEN {
            return None;
        }
        let msg = self.message_id.to_u32().to_be_bytes();
        out[0..4].copy_from_slice(&msg);
        // Length field is filled by the message encoder; placeholder here.
        let req = self.request_id.to_u32().to_be_bytes();
        out[8..12].copy_from_slice(&req);
        out[12] = SOMEIP_PROTOCOL_VERSION;
        out[13] = self.interface_version;
        out[14] = self.message_type.to_u8();
        out[15] = self.return_code.to_u8();
        Some(())
    }

    /// Decodes a header from `bytes` (must be at least [`SOMEIP_HEADER_LEN`]).
    pub fn decode(bytes: &[u8]) -> Option<SomeIpHeader> {
        if bytes.len() < SOMEIP_HEADER_LEN {
            return None;
        }
        let message_id =
            SomeIpMessageId::from_u32(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        let request_id = SomeIpRequestId::from_u32(u32::from_be_bytes([
            bytes[8], bytes[9], bytes[10], bytes[11],
        ]));
        let message_type = SomeIpMessageType::from_u8(bytes[14])?;
        let return_code = SomeIpReturnCode::from_u8(bytes[15])?;
        Some(SomeIpHeader {
            message_id,
            request_id,
            interface_version: bytes[13],
            message_type,
            return_code,
        })
    }
}

/// A complete SOME/IP message (header + payload).
///
/// `SomeIpMessage` is `Copy`; the payload is stored inline up to
/// [`SOMEIP_MAX_PAYLOAD`] bytes so it works in `no_std` without allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SomeIpMessage {
    header: SomeIpHeader,
    payload: [u8; SOMEIP_MAX_PAYLOAD],
    payload_len: u16,
}

impl SomeIpMessage {
    /// Builds a message from a header and payload slice.
    ///
    /// Returns `None` if the payload exceeds [`SOMEIP_MAX_PAYLOAD`].
    pub fn new(header: SomeIpHeader, payload: &[u8]) -> Option<SomeIpMessage> {
        if payload.len() > SOMEIP_MAX_PAYLOAD {
            return None;
        }
        let mut buf = [0u8; SOMEIP_MAX_PAYLOAD];
        buf[..payload.len()].copy_from_slice(payload);
        Some(SomeIpMessage {
            header,
            payload: buf,
            payload_len: payload.len() as u16,
        })
    }

    /// Returns the header.
    pub fn header(&self) -> SomeIpHeader {
        self.header
    }

    /// Returns the payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload[..self.payload_len as usize]
    }

    /// Encodes the full message (header + payload) into `out`.
    ///
    /// Returns `None` if `out` is too small. The SOME/IP `Length` field is
    /// written correctly here (it cannot be known at header-encode time alone).
    pub fn encode(&self, out: &mut [u8]) -> Option<usize> {
        let total = SOMEIP_HEADER_LEN + self.payload_len as usize;
        if out.len() < total {
            return None;
        }
        let length = SomeIpHeader::length_field(self.payload_len as usize);
        out[4..8].copy_from_slice(&length.to_be_bytes());
        self.header.encode(&mut out[..SOMEIP_HEADER_LEN])?;
        out[SOMEIP_HEADER_LEN..total].copy_from_slice(self.payload());
        Some(total)
    }

    /// Decodes a full message from `bytes`.
    ///
    /// Validates the declared `Length` field against the buffer and rejects
    /// `Request` messages that do not carry the expected protocol version.
    pub fn decode(bytes: &[u8]) -> Option<SomeIpMessage> {
        if bytes.len() < SOMEIP_HEADER_LEN {
            return None;
        }
        let header = SomeIpHeader::decode(bytes)?;
        let declared_len = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) as usize;
        // `Length` covers Request ID (4) + the 4 post-length header bytes + payload.
        let declared_payload = declared_len.checked_sub(8)?;
        let expected_total = SOMEIP_HEADER_LEN + declared_payload;
        if bytes.len() < expected_total {
            return None;
        }
        if header.message_type == SomeIpMessageType::Request && bytes[12] != SOMEIP_PROTOCOL_VERSION
        {
            return None;
        }
        let payload = &bytes[SOMEIP_HEADER_LEN..expected_total];
        SomeIpMessage::new(header, payload)
    }
}

impl Frame for SomeIpMessage {
    type Id = SomeIpMessageId;

    fn id(&self) -> SomeIpMessageId {
        self.header.message_id
    }

    fn len(&self) -> usize {
        SOMEIP_HEADER_LEN + self.payload_len as usize
    }
}

/// Low-level SOME/IP transceiver (socket or simulator backend).
pub trait SomeIpTransceiver {
    /// Sends a SOME/IP message.
    fn send(&mut self, message: SomeIpMessage) -> Result<(), BusError>;

    /// Receives the next buffered message, if any.
    fn recv(&mut self) -> Result<SomeIpMessage, BusError>;

    /// `true` if a message is pending receipt.
    fn has_received(&self) -> bool;

    /// `true` if the transceiver can transmit now.
    fn can_send(&self) -> bool;
}

/// High-level SOME/IP interface implementing the unified [`VehicleBus`] trait.
pub struct SomeIpBus<T: SomeIpTransceiver> {
    transceiver: T,
}

impl<T: SomeIpTransceiver> SomeIpBus<T> {
    /// Wraps a [`SomeIpTransceiver`] as a [`SomeIpBus`].
    pub fn new(transceiver: T) -> Self {
        SomeIpBus { transceiver }
    }
}

impl<T: SomeIpTransceiver> VehicleBus for SomeIpBus<T> {
    type Frame = SomeIpMessage;

    fn transmit(&mut self, frame: SomeIpMessage) -> Result<(), BusError> {
        self.transceiver.send(frame)
    }

    fn receive(&mut self) -> Result<SomeIpMessage, BusError> {
        self.transceiver.recv()
    }

    fn can_receive(&self) -> bool {
        self.transceiver.has_received()
    }

    fn can_transmit(&self) -> bool {
        self.transceiver.can_send()
    }
}

/// Validates a SOME/IP message against protocol constraints.
pub fn validate_message(message: &SomeIpMessage) -> Result<(), Error> {
    if message.payload_len as usize > SOMEIP_MAX_PAYLOAD {
        return Err(Error::InvalidArgument);
    }
    if message.header.message_type == SomeIpMessageType::Request
        && message.header.message_id.service == 0
    {
        return Err(Error::InvalidArgument);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_header() -> SomeIpHeader {
        SomeIpHeader {
            message_id: SomeIpMessageId {
                service: 0x1234,
                method: 0x0001,
            },
            request_id: SomeIpRequestId {
                client: 0xAB,
                session: 0x0001,
            },
            interface_version: 0x01,
            message_type: SomeIpMessageType::Request,
            return_code: SomeIpReturnCode::Ok,
        }
    }

    #[test]
    fn message_type_round_trips() {
        for mt in [
            SomeIpMessageType::Request,
            SomeIpMessageType::RequestNoReturn,
            SomeIpMessageType::Notification,
            SomeIpMessageType::Response,
            SomeIpMessageType::Error,
        ] {
            assert_eq!(SomeIpMessageType::from_u8(mt.to_u8()), Some(mt));
        }
    }

    #[test]
    fn return_code_round_trips() {
        for rc in [
            SomeIpReturnCode::Ok,
            SomeIpReturnCode::NotOk,
            SomeIpReturnCode::WrongProtocolVersion,
        ] {
            assert_eq!(SomeIpReturnCode::from_u8(rc.to_u8()), Some(rc));
        }
    }

    #[test]
    fn header_encode_decode_round_trips() {
        let h = sample_header();
        let mut buf = [0u8; SOMEIP_HEADER_LEN];
        h.encode(&mut buf).unwrap();
        let decoded = SomeIpHeader::decode(&buf).unwrap();
        assert_eq!(decoded, h);
        assert_eq!(decoded.message_id.to_u32(), 0x1234_0001);
    }

    #[test]
    fn message_encode_decode_round_trips() {
        let h = sample_header();
        let msg = SomeIpMessage::new(h, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        let mut buf = [0u8; 64];
        let n = msg.encode(&mut buf).unwrap();
        assert_eq!(n, SOMEIP_HEADER_LEN + 4);
        let decoded = SomeIpMessage::decode(&buf[..n]).unwrap();
        assert_eq!(decoded.payload(), &[0xDE, 0xAD, 0xBE, 0xEF]);
        assert_eq!(decoded.header.message_id, h.message_id);
    }

    #[test]
    fn oversize_payload_rejected() {
        let h = sample_header();
        let data = [0u8; SOMEIP_MAX_PAYLOAD + 1];
        assert!(SomeIpMessage::new(h, &data).is_none());
    }

    #[test]
    fn length_field_accounts_for_request_and_header() {
        assert_eq!(SomeIpHeader::length_field(0), 8);
        assert_eq!(SomeIpHeader::length_field(4), 12);
    }

    #[test]
    fn validate_rejects_zero_service_request() {
        let h = SomeIpHeader {
            message_id: SomeIpMessageId {
                service: 0,
                method: 0,
            },
            ..sample_header()
        };
        let msg = SomeIpMessage::new(h, &[]).unwrap();
        assert_eq!(validate_message(&msg), Err(Error::InvalidArgument));
    }
}
