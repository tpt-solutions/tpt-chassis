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

//! UDS (Unified Diagnostic Services, ISO 14229-1) support.
//!
//! Provides a small but conformant UDS *server* that decodes single-frame
//! requests (the layout used on CAN with N_PCI single-frame, ISO 15765-2), and
//! produces responses. It is transport-agnostic: a caller feeds it the request
//! payload bytes and gets back the response payload bytes, so it can sit on top
//! of the CAN abstraction in [`crate::can`] or any other link.
//!
//! This implements the subset of services needed for ECU diagnostics and is
//! intentionally safe-Rust and `no_std`.

/// Maximum UDS payload size for a single CAN frame (ISO-TP single frame).
pub const UDS_MAX_PAYLOAD: usize = 7;

/// Number of `tick()` calls a session stays alive without a fresh TesterPresent.
/// Once it elapses the server auto-disarms (drops to the default session and
/// re-locks security), so a stalled tester cannot hold an elevated session.
pub const TESTER_PRESENT_TIMEOUT: u8 = 5;

/// UDS service identifiers (SID).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UdsService {
    /// Diagnostic Session Control (0x10).
    DiagnosticSessionControl,
    /// ECU Reset (0x11).
    EcuReset,
    /// Security Access (0x27).
    SecurityAccess,
    /// ReadDataByIdentifier (0x22).
    ReadDataByIdentifier,
    /// WriteDataByIdentifier (0x2E).
    WriteDataByIdentifier,
    /// TesterPresent (0x3E).
    TesterPresent,
}

impl UdsService {
    /// Encodes the SID as its wire value.
    pub fn to_u8(self) -> u8 {
        match self {
            UdsService::DiagnosticSessionControl => 0x10,
            UdsService::EcuReset => 0x11,
            UdsService::SecurityAccess => 0x27,
            UdsService::ReadDataByIdentifier => 0x22,
            UdsService::WriteDataByIdentifier => 0x2E,
            UdsService::TesterPresent => 0x3E,
        }
    }

    /// Decodes a SID, returning `None` for unimplemented services.
    pub fn from_u8(v: u8) -> Option<UdsService> {
        match v {
            0x10 => Some(UdsService::DiagnosticSessionControl),
            0x11 => Some(UdsService::EcuReset),
            0x27 => Some(UdsService::SecurityAccess),
            0x22 => Some(UdsService::ReadDataByIdentifier),
            0x2E => Some(UdsService::WriteDataByIdentifier),
            0x3E => Some(UdsService::TesterPresent),
            _ => None,
        }
    }

    /// Returns the negative-response SID (request SID | 0x40).
    pub fn response_sid(self) -> u8 {
        self.to_u8() | 0x40
    }
}

/// UDS negative response codes (NRC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum UdsNrc {
    /// Service not supported (0x11).
    ServiceNotSupported,
    /// Sub-function not supported (0x12).
    SubFunctionNotSupported,
    /// Conditions not correct (0x22).
    ConditionsNotCorrect,
    /// Request out of range (0x31).
    RequestOutOfRange,
    /// Security access denied (0x33).
    SecurityAccessDenied,
    /// Invalid key (0x35).
    InvalidKey,
    /// Request correctly received, response pending (0x78).
    RequestCorrectlyReceivedResponsePending,
}

impl UdsNrc {
    /// Encodes the NRC as its wire value.
    pub fn to_u8(self) -> u8 {
        match self {
            UdsNrc::ServiceNotSupported => 0x11,
            UdsNrc::SubFunctionNotSupported => 0x12,
            UdsNrc::ConditionsNotCorrect => 0x22,
            UdsNrc::RequestOutOfRange => 0x31,
            UdsNrc::SecurityAccessDenied => 0x33,
            UdsNrc::InvalidKey => 0x35,
            UdsNrc::RequestCorrectlyReceivedResponsePending => 0x78,
        }
    }
}

/// Diagnostic session types (sub-function of 0x10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdsSession {
    /// Default session (0x01).
    Default,
    /// Extended diagnostic session (0x03).
    Extended,
}

impl UdsSession {
    /// Decodes the sub-function byte.
    pub fn from_u8(v: u8) -> Option<UdsSession> {
        match v {
            0x01 => Some(UdsSession::Default),
            0x03 => Some(UdsSession::Extended),
            _ => None,
        }
    }
}

/// Security access levels (sub-function of 0x27).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UdsSecurityLevel {
    /// Locked (no elevated access).
    Locked,
    /// Unlocked (seed/key exchange succeeded).
    Unlocked,
}

/// Backend data source for `ReadDataByIdentifier` / `WriteDataByIdentifier`.
///
/// Implementors map data identifiers (DID) to readable/writable values. DIDs
/// implemented by real hardware (VIN, calibration, etc.) plug in here.
pub trait UdsDataProvider {
    /// Returns `Some(len)` if `did` is readable, with the number of data bytes.
    fn did_read_len(&self, did: u16) -> Option<usize>;

    /// Reads up to `out.len()` bytes for `did` (caller ensures DID is readable).
    fn did_read(&self, did: u16, out: &mut [u8]);

    /// Writes `data` to `did`. Returns `false` if the write is rejected.
    fn did_write(&mut self, did: u16, data: &[u8]) -> bool;
}

/// A UDS server state machine.
///
/// Tracks the active diagnostic session, security level, and tester-present
/// liveness. Feed it request payloads via [`UdsServer::handle`].
pub struct UdsServer<D: UdsDataProvider> {
    data: D,
    session: UdsSession,
    security: UdsSecurityLevel,
    /// Counts down tester-present liveness; `None` means no timeout active.
    tester_present_pending: Option<u8>,
}

impl<D: UdsDataProvider> UdsServer<D> {
    /// Creates a server starting in the default session, locked.
    pub fn new(data: D) -> Self {
        UdsServer {
            data,
            session: UdsSession::Default,
            security: UdsSecurityLevel::Locked,
            tester_present_pending: None,
        }
    }

    /// Returns the currently active session.
    pub fn session(&self) -> UdsSession {
        self.session
    }

    /// Returns the current security level.
    pub fn security(&self) -> UdsSecurityLevel {
        self.security
    }

    /// Processes a UDS request payload and returns the response payload.
    /// Returns `true` while a tester-present liveness window is active.
    pub fn is_held(&self) -> bool {
        self.tester_present_pending.is_some()
    }

    /// Advances the tester-present liveness watchdog by one tick.
    ///
    /// Each call decrements the remaining liveness window set by the last
    /// `TesterPresent` request. When the window elapses, the server auto-disarms:
    /// the active diagnostic session reverts to [`UdsSession::Default`] and the
    /// security level re-locks, so a disconnected or stalled tester cannot keep
    /// an elevated session open. Returns `true` if the server just disarmed.
    pub fn tick(&mut self) -> bool {
        match self.tester_present_pending {
            Some(0) => {
                self.session = UdsSession::Default;
                self.security = UdsSecurityLevel::Locked;
                self.tester_present_pending = None;
                true
            }
            Some(n) => {
                let next = n - 1;
                if next == 0 {
                    self.session = UdsSession::Default;
                    self.security = UdsSecurityLevel::Locked;
                    self.tester_present_pending = None;
                    true
                } else {
                    self.tester_present_pending = Some(next);
                    false
                }
            }
            None => false,
        }
    }

    ///
    /// `request`/`response` use the single-frame layout (first byte = SID).
    /// Returns `None` if `response` is too small for the generated response.
    pub fn handle(&mut self, request: &[u8], response: &mut [u8]) -> Option<usize> {
        let sid = *request.first()?;
        let service = match UdsService::from_u8(sid) {
            Some(s) => s,
            None => {
                // Unsupported service: negative response 0x7F, SID, NRC 0x11.
                if response.len() < 3 {
                    return None;
                }
                response[0] = 0x7F;
                response[1] = sid;
                response[2] = UdsNrc::ServiceNotSupported.to_u8();
                return Some(3);
            }
        };
        let params = &request[1..];

        match service {
            UdsService::TesterPresent => {
                // Sub-function 0x00 = suppressPosRspMsgIndicationBit clear.
                if params.first().copied().unwrap_or(0) & 0x7F != 0x00 {
                    return self.nrc(response, service, UdsNrc::SubFunctionNotSupported);
                }
                self.tester_present_pending = None;
                // Refresh the liveness window; if no further TesterPresent arrives
                // within TESTER_PRESENT_TIMEOUT ticks, 	ick() auto-disarms.
                self.tester_present_pending = Some(TESTER_PRESENT_TIMEOUT);
                response[0] = service.to_u8() + 0x40;
                response[1] = 0x00;
                Some(2)
            }
            UdsService::DiagnosticSessionControl => {
                let sub = *params.first()?;
                let next = UdsSession::from_u8(sub)?;
                self.session = next;
                // Positive response: SID+0x40, sub-function, then 2-byte
                // session timing (P2 server) â€” fixed at 0x0032 (50ms) here.
                response[0] = service.response_sid();
                response[1] = sub;
                response[2] = 0x00;
                response[3] = 0x32;
                Some(4)
            }
            UdsService::EcuReset => {
                let sub = *params.first()?;
                if sub != 0x01 {
                    return self.nrc(response, service, UdsNrc::SubFunctionNotSupported);
                }
                self.session = UdsSession::Default;
                self.security = UdsSecurityLevel::Locked;
                response[0] = service.response_sid();
                response[1] = sub;
                Some(2)
            }
            UdsService::SecurityAccess => {
                let sub = *params.first()?;
                match sub {
                    0x01 => {
                        // Request seed. In this minimal impl the seed is fixed.
                        response[0] = service.response_sid();
                        response[1] = sub;
                        response[2] = 0xAA; // seed (low complexity demo)
                        Some(3)
                    }
                    0x02 => {
                        // Send key. A correct key (seed ^ 0xFF) unlocks.
                        let key = *params.get(1)?;
                        let expected = 0xAA ^ 0xFF;
                        if key == expected {
                            self.security = UdsSecurityLevel::Unlocked;
                            response[0] = service.response_sid();
                            response[1] = sub;
                            Some(2)
                        } else {
                            self.nrc(response, service, UdsNrc::InvalidKey)
                        }
                    }
                    _ => self.nrc(response, service, UdsNrc::SubFunctionNotSupported),
                }
            }
            UdsService::ReadDataByIdentifier => {
                let did = u16::from_be_bytes([*params.first()?, *params.get(1)?]);
                let len = match self.data.did_read_len(did) {
                    Some(l) => l,
                    None => return self.nrc(response, service, UdsNrc::RequestOutOfRange),
                };
                if len > UDS_MAX_PAYLOAD {
                    return self.nrc(response, service, UdsNrc::RequestOutOfRange);
                }
                if self.security == UdsSecurityLevel::Locked && did_reserved(did) {
                    return self.nrc(response, service, UdsNrc::SecurityAccessDenied);
                }
                response[0] = service.response_sid();
                response[1] = params[0];
                response[2] = params[1];
                self.data.did_read(did, &mut response[3..3 + len]);
                Some(3 + len)
            }
            UdsService::WriteDataByIdentifier => {
                if self.security == UdsSecurityLevel::Locked {
                    return self.nrc(response, service, UdsNrc::SecurityAccessDenied);
                }
                let did = u16::from_be_bytes([*params.first()?, *params.get(1)?]);
                let data = &params[2..];
                if !self.data.did_write(did, data) {
                    return self.nrc(response, service, UdsNrc::RequestOutOfRange);
                }
                response[0] = service.response_sid();
                response[1] = params[0];
                response[2] = params[1];
                Some(3)
            }
        }
    }

    fn nrc(&self, response: &mut [u8], service: UdsService, nrc: UdsNrc) -> Option<usize> {
        if response.len() < 3 {
            return None;
        }
        // UDS negative response format: 0x7F, original SID, NRC.
        response[0] = 0x7F;
        response[1] = service.to_u8();
        response[2] = nrc.to_u8();
        Some(3)
    }
}

/// DIDs in this reserved range require an unlocked security level to read.
fn did_reserved(did: u16) -> bool {
    (0xF100..=0xF1FF).contains(&did)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockData {
        store: [u8; 32],
    }

    impl UdsDataProvider for MockData {
        fn did_read_len(&self, did: u16) -> Option<usize> {
            match did {
                0x0001 => Some(2),
                0xF190 => Some(4), // VIN-ish, reserved range
                _ => None,
            }
        }

        fn did_read(&self, _did: u16, out: &mut [u8]) {
            out.copy_from_slice(&self.store[..out.len()]);
        }

        fn did_write(&mut self, _did: u16, data: &[u8]) -> bool {
            if data.len() > self.store.len() {
                return false;
            }
            self.store[..data.len()].copy_from_slice(data);
            true
        }
    }

    fn server() -> UdsServer<MockData> {
        UdsServer::new(MockData { store: [0u8; 32] })
    }

    #[test]
    fn tester_present_responds() {
        let mut s = server();
        let mut resp = [0u8; 16];
        let n = s.handle(&[0x3E, 0x00], &mut resp).unwrap();
        assert_eq!(&resp[..n], &[0x7E, 0x00]);
    }

    #[test]
    fn session_control_enters_extended() {
        let mut s = server();
        let mut resp = [0u8; 16];
        let n = s.handle(&[0x10, 0x03], &mut resp).unwrap();
        assert_eq!(&resp[..n], &[0x50, 0x03, 0x00, 0x32]);
        assert_eq!(s.session(), UdsSession::Extended);
    }

    #[test]
    fn read_did_unknown_is_nrc() {
        let mut s = server();
        let mut resp = [0u8; 16];
        let n = s.handle(&[0x22, 0x99, 0x99], &mut resp).unwrap();
        assert_eq!(&resp[..n], &[0x7F, 0x22, 0x31]); // requestOutOfRange
    }

    #[test]
    fn read_reserved_did_requires_security() {
        let mut s = server();
        let mut resp = [0u8; 16];
        let n = s.handle(&[0x22, 0xF1, 0x90], &mut resp).unwrap();
        assert_eq!(&resp[..n], &[0x7F, 0x22, 0x33]); // securityAccessDenied
    }

    #[test]
    fn security_unlock_then_read_reserved() {
        let mut s = server();
        let mut resp = [0u8; 16];
        // request seed
        let n = s.handle(&[0x27, 0x01], &mut resp).unwrap();
        assert_eq!(&resp[..n], &[0x67, 0x01, 0xAA]);
        // send correct key (seed ^ 0xFF)
        let n = s.handle(&[0x27, 0x02, 0x55], &mut resp).unwrap();
        assert_eq!(&resp[..n], &[0x67, 0x02]);
        assert_eq!(s.security(), UdsSecurityLevel::Unlocked);
        // now read the reserved DID
        let n = s.handle(&[0x22, 0xF1, 0x90], &mut resp).unwrap();
        assert_eq!(&resp[..3], &[0x62, 0xF1, 0x90]);
        assert_eq!(n, 3 + 4);
    }

    #[test]
    fn write_requires_security() {
        let mut s = server();
        let mut resp = [0u8; 16];
        let n = s
            .handle(&[0x2E, 0x00, 0x01, 0xAB, 0xCD], &mut resp)
            .unwrap();
        assert_eq!(&resp[..n], &[0x7F, 0x2E, 0x33]); // securityAccessDenied
    }

    #[test]
    fn unsupported_service_is_nrc() {
        let mut s = server();
        let mut resp = [0u8; 16];
        let n = s.handle(&[0x19, 0x01], &mut resp).unwrap(); // 0x19 not implemented
        assert_eq!(&resp[..n], &[0x7F, 0x19, 0x11]); // serviceNotSupported
    }

    #[test]
    fn multi_frame_write_via_isotp_then_unlock() {
        use crate::isotp::{encode_message, IsoTpEvent, IsoTpLink};
        let mut s = server();
        // A 20-byte WriteDataByIdentifier request must travel as FF + consecutive
        // frames because a single CAN frame holds only 7 data bytes.
        let mut req = [0u8; 20];
        req[0] = 0x2E;
        req[1] = 0x00;
        req[2] = 0x01;
        for b in req.iter_mut().skip(3) {
            *b = 0xAB;
        }
        let mut frames = [[0u8; 8]; 8];
        let n = encode_message(&req, &mut frames).unwrap();
        assert!(n >= 2, "message must be segmented");
        let mut link: IsoTpLink<64> = IsoTpLink::new();
        let mut payload = [0u8; 20];
        let mut got = 0;
        for frame in &frames[..n] {
            if let IsoTpEvent::Complete(p) = link.feed(frame).unwrap() {
                payload[..p.len()].copy_from_slice(p);
                got = p.len();
            }
        }
        assert_eq!(got, 20);
        // While locked, the write is denied.
        let mut resp = [0u8; 16];
        let r = s.handle(&payload[..got], &mut resp).unwrap();
        assert_eq!(&resp[..r], &[0x7F, 0x2E, 0x33]);
        // Unlock and resend the same reassembled request -> positive response.
        let _ = s.handle(&[0x27, 0x01], &mut resp);
        let _ = s.handle(&[0x27, 0x02, 0x55], &mut resp);
        let r = s.handle(&payload[..got], &mut resp).unwrap();
        assert_eq!(&resp[..r], &[0x6E, 0x00, 0x01]);
    }

    #[test]
    fn tester_present_timeout_auto_disarms() {
        let mut s = server();
        let mut resp = [0u8; 16];
        // Enter extended session + unlock so we have an elevated state to lose.
        let _ = s.handle(&[0x10, 0x03], &mut resp);
        let _ = s.handle(&[0x27, 0x01], &mut resp);
        let _ = s.handle(&[0x27, 0x02, 0x55], &mut resp);
        assert_eq!(s.session(), UdsSession::Extended);
        assert_eq!(s.security(), UdsSecurityLevel::Unlocked);

        // TesterPresent refreshes the liveness window.
        let _ = s.handle(&[0x3E, 0x00], &mut resp);
        assert!(s.is_held());

        // Each tick consumes one unit of the window.
        for _ in 0..(crate::uds::TESTER_PRESENT_TIMEOUT as usize - 1) {
            assert!(!s.tick(), "should still be held before timeout");
        }
        // Final tick elapses the window and auto-disarms.
        assert!(s.tick(), "should disarm on timeout");
        assert_eq!(s.session(), UdsSession::Default);
        assert_eq!(s.security(), UdsSecurityLevel::Locked);
        assert!(!s.is_held());
    }
}
