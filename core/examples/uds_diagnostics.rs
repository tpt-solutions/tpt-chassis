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

//! `uds_diagnostics` — run a `UdsServer` over the simulated CAN bus. A tester
//! node sends UDS request frames (ReadDataByIdentifier, SecurityAccess,
//! WriteDataByIdentifier, TesterPresent) and an ECU node answers them. Run with:
//!
//! ```sh
//! cargo run -p tpt-chassis-core --example uds_diagnostics
//! ```

use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_core::uds::{UdsDataProvider, UdsNrc, UdsServer, UdsService, UdsSession};
use tpt_chassis_sim::can::SimCanNetwork;

/// A tiny data provider backing two DIDs.
struct MockData {
    store: [u8; 32],
}

impl UdsDataProvider for MockData {
    fn did_read_len(&self, did: u16) -> Option<usize> {
        match did {
            0x0001 => Some(2),
            0xF190 => Some(4), // VIN-ish, lives in the reserved range
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

fn main() {
    let net = SimCanNetwork::new();
    let tester = CanBus::new(net.node());
    let mut ecu = CanBus::new(net.node());
    let mut server = UdsServer::new(MockData { store: [0u8; 32] });

    let ecu_id = CanId::standard(0x7E0).expect("valid id");
    let resp_id = CanId::standard(0x7E8).expect("valid id");

    // Helper to send a UDS request and read the response payload.
    let mut tester = tester;
    let send_request = |tester: &mut CanBus<_>,
                        ecu: &mut CanBus<_>,
                        server: &mut UdsServer<MockData>,
                        req: &[u8]|
     -> Vec<u8> {
        let frame = CanFrame::new(ecu_id, req).expect("valid frame");
        tester.transmit(frame).expect("transmit request");
        assert!(ecu.can_receive());
        let rx = ecu.receive().expect("receive request");
        let mut resp = [0u8; 8];
        let n = server.handle(rx.data(), &mut resp).expect("handle");
        // Echo the response back on the response ID so the tester sees it.
        let out = CanFrame::new(resp_id, &resp[..n]).expect("valid frame");
        ecu.transmit(out).expect("transmit response");
        let got = tester.receive().expect("receive response");
        got.data().to_vec()
    };

    // 1) Enter extended diagnostic session.
    let r = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::DiagnosticSessionControl.to_u8(), 0x03],
    );
    assert_eq!(r[..4], [0x50, 0x03, 0x00, 0x32]);
    assert_eq!(server.session(), UdsSession::Extended);
    println!("session control -> extended (0x{:02X}{:02X})", r[0], r[1]);

    // 2) Read a readable DID (0x0001) — should succeed.
    let r = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::ReadDataByIdentifier.to_u8(), 0x00, 0x01],
    );
    assert_eq!(&r[..3], &[0x62, 0x00, 0x01]);
    println!("read DID 0x0001 -> data {:?}", &r[3..]);

    // 3) Read a reserved DID (0xF190) while locked — security denied.
    let r = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::ReadDataByIdentifier.to_u8(), 0xF1, 0x90],
    );
    assert_eq!(
        r,
        [
            0x7F,
            UdsService::ReadDataByIdentifier.to_u8(),
            UdsNrc::SecurityAccessDenied.to_u8()
        ]
    );
    println!("read reserved DID while locked -> NRC 0x33 (security denied)");

    // 4) Unlock via seed/key (key = seed ^ 0xFF, seed is fixed 0xAA).
    let _ = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::SecurityAccess.to_u8(), 0x01],
    );
    let r = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::SecurityAccess.to_u8(), 0x02, 0x55],
    );
    assert_eq!(&r[..2], &[0x67, 0x02]);
    println!("security access -> unlocked");

    // 5) Write DID (now allowed) then read it back.
    let _ = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[
            UdsService::WriteDataByIdentifier.to_u8(),
            0x00,
            0x01,
            0xAB,
            0xCD,
        ],
    );
    let r = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::ReadDataByIdentifier.to_u8(), 0x00, 0x01],
    );
    assert_eq!(&r[3..], &[0xAB, 0xCD]);
    println!("write+read DID 0x0001 -> 0xABCD");

    // 6) TesterPresent keeps the session alive.
    let r = send_request(
        &mut tester,
        &mut ecu,
        &mut server,
        &[UdsService::TesterPresent.to_u8(), 0x00],
    );
    assert_eq!(&r[..2], &[0x7E, 0x00]);
    println!("tester present -> alive");

    println!("uds_diagnostics: all services exercised OK");
}
