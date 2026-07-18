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

//! Integration test: drive CAN, SOME/IP (Ethernet), and LIN through the single
//! unified [`tpt_chassis_core::bus::VehicleBus`] trait using the simulated
//! backends.

use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_core::lin::{LinBus, LinFrame, LinId};
use tpt_chassis_core::someip::{
    SomeIpBus, SomeIpHeader, SomeIpMessage, SomeIpMessageId, SomeIpMessageType, SomeIpRequestId,
    SomeIpReturnCode,
};

use tpt_chassis_sim::can::SimCanNetwork;
use tpt_chassis_sim::lin::SimLinCluster;
use tpt_chassis_sim::someip::SimSomeIpNetwork;

#[test]
fn all_three_buses_share_one_api() {
    // CAN
    let can_net = SimCanNetwork::new();
    let mut can_a = CanBus::new(can_net.node());
    let mut can_b = CanBus::new(can_net.node());
    let can_frame = CanFrame::new(CanId::standard(0x7DF).unwrap(), &[0x02, 0x01, 0x0C]).unwrap();
    can_a.transmit(can_frame).unwrap();
    assert!(can_b.can_receive());
    let got_can = can_b.receive().unwrap();
    assert_eq!(got_can.data(), &[0x02, 0x01, 0x0C]);

    // SOME/IP (Ethernet)
    let eth_net = SimSomeIpNetwork::new();
    let mut eth_a = SomeIpBus::new(eth_net.node());
    let mut eth_b = SomeIpBus::new(eth_net.node());
    let header = SomeIpHeader {
        message_id: SomeIpMessageId {
            service: 0x4500,
            method: 0x0003,
        },
        request_id: SomeIpRequestId {
            client: 0x00A1,
            session: 0x0007,
        },
        interface_version: 1,
        message_type: SomeIpMessageType::Request,
        return_code: SomeIpReturnCode::Ok,
    };
    let someip_msg: SomeIpMessage = SomeIpMessage::new(header, &[0x11, 0x22, 0x33, 0x44]).unwrap();
    eth_a.transmit(someip_msg).unwrap();
    assert!(eth_b.can_receive());
    let got_eth = eth_b.receive().unwrap();
    assert_eq!(got_eth.payload(), &[0x11, 0x22, 0x33, 0x44]);
    assert_eq!(got_eth.header().message_id.service, 0x4500);

    // LIN
    let lin_net = SimLinCluster::new();
    let mut lin_master = LinBus::new(lin_net.node(true));
    let mut lin_slave = LinBus::new(lin_net.node(false));
    let lin_frame = LinFrame::new(LinId::new(0x2A).unwrap(), &[0xAA, 0xBB]).unwrap();
    lin_master.transmit(lin_frame).unwrap();
    assert!(lin_slave.can_receive());
    let got_lin = lin_slave.receive().unwrap();
    assert_eq!(got_lin.id().raw(), 0x2A);
    assert_eq!(got_lin.data(), &[0xAA, 0xBB]);
}

#[test]
fn unified_loopback_free_property_holds_on_all_buses() {
    // None of the buses should echo a transmitted frame back to its sender.
    let can_net = SimCanNetwork::new();
    let mut can = CanBus::new(can_net.node());
    can.transmit(CanFrame::new(CanId::standard(0x1).unwrap(), &[1]).unwrap())
        .unwrap();
    assert!(!can.can_receive());

    let eth_net = SimSomeIpNetwork::new();
    let mut eth = SomeIpBus::new(eth_net.node());
    let h = SomeIpHeader {
        message_id: SomeIpMessageId {
            service: 1,
            method: 1,
        },
        request_id: SomeIpRequestId {
            client: 1,
            session: 1,
        },
        interface_version: 1,
        message_type: SomeIpMessageType::Request,
        return_code: SomeIpReturnCode::Ok,
    };
    eth.transmit(SomeIpMessage::new(h, &[1]).unwrap()).unwrap();
    assert!(!eth.can_receive());

    let lin_net = SimLinCluster::new();
    let mut lin = LinBus::new(lin_net.node(true));
    lin.transmit(LinFrame::new(LinId::new(0x05).unwrap(), &[1]).unwrap())
        .unwrap();
    assert!(!lin.can_receive());
}
