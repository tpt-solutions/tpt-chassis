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

//! End-to-end UDS-over-CAN conformance test: a UDS server is driven through the
//! simulated CAN bus exactly as it would be by a diagnostic tester on the wire.

use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_core::uds::{UdsDataProvider, UdsServer};

use tpt_chassis_sim::can::{SimCanNetwork, SimCanNode};

struct Calibration {
    values: [u8; 8],
}

impl UdsDataProvider for Calibration {
    fn did_read_len(&self, did: u16) -> Option<usize> {
        if did == 0x1234 {
            Some(4)
        } else {
            None
        }
    }

    fn did_read(&self, _did: u16, out: &mut [u8]) {
        out.copy_from_slice(&self.values[..out.len()]);
    }

    fn did_write(&mut self, _did: u16, data: &[u8]) -> bool {
        if data.len() > self.values.len() {
            return false;
        }
        self.values[..data.len()].copy_from_slice(data);
        true
    }
}

/// Drives one UDS request through the CAN bus: the tester transmits a request
/// frame, the ECU server processes it and transmits the response frame, and
/// the tester reads it back.
fn exchange(
    tester: &mut CanBus<SimCanNode>,
    ecu: &mut CanBus<SimCanNode>,
    server: &mut UdsServer<Calibration>,
    request: &[u8],
) -> Option<[u8; 8]> {
    let req_frame = CanFrame::new(CanId::standard(0x7E0).unwrap(), request).unwrap();
    tester.transmit(req_frame).ok()?;

    let incoming = ecu.receive().ok()?;
    let mut resp = [0u8; 8];
    let n = server.handle(incoming.data(), &mut resp)?;

    let resp_frame = CanFrame::new(CanId::standard(0x7E8).unwrap(), &resp[..n]).unwrap();
    ecu.transmit(resp_frame).ok()?;

    let out_frame = tester.receive().ok()?;
    let mut out = [0u8; 8];
    out[..out_frame.data().len()].copy_from_slice(out_frame.data());
    Some(out)
}

#[test]
fn uds_over_can_round_trips() {
    let net = SimCanNetwork::new();
    let mut tester = CanBus::new(net.node());
    let mut ecu = CanBus::new(net.node());
    let mut server = UdsServer::new(Calibration { values: [0u8; 8] });

    // TesterPresent
    let r = exchange(&mut tester, &mut ecu, &mut server, &[0x3E, 0x00]).unwrap();
    assert_eq!(&r[..2], &[0x7E, 0x00]);

    // Read known DID (unlocked, non-reserved) -> positive response 0x62
    let r = exchange(&mut tester, &mut ecu, &mut server, &[0x22, 0x12, 0x34]).unwrap();
    assert_eq!(&r[..3], &[0x62, 0x12, 0x34]);

    // Read unknown DID -> NRC 0x31
    let r = exchange(&mut tester, &mut ecu, &mut server, &[0x22, 0xFF, 0xFF]).unwrap();
    assert_eq!(&r[..3], &[0x7F, 0x22, 0x31]);

    // Write without security -> NRC 0x33
    let r = exchange(
        &mut tester,
        &mut ecu,
        &mut server,
        &[0x2E, 0x12, 0x34, 0x01, 0x02, 0x03, 0x04],
    )
    .unwrap();
    assert_eq!(&r[..3], &[0x7F, 0x2E, 0x33]);

    // Unlock, then write succeeds
    exchange(&mut tester, &mut ecu, &mut server, &[0x27, 0x01]).unwrap();
    let r = exchange(&mut tester, &mut ecu, &mut server, &[0x27, 0x02, 0x55]).unwrap();
    assert_eq!(&r[..2], &[0x67, 0x02]);

    let r = exchange(
        &mut tester,
        &mut ecu,
        &mut server,
        &[0x2E, 0x12, 0x34, 0x0A, 0x0B, 0x0C, 0x0D],
    )
    .unwrap();
    assert_eq!(&r[..3], &[0x6E, 0x12, 0x34]);

    // Read back the written value
    let r = exchange(&mut tester, &mut ecu, &mut server, &[0x22, 0x12, 0x34]).unwrap();
    assert_eq!(&r[3..7], [0x0A, 0x0B, 0x0C, 0x0D]);
}
