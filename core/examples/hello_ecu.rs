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

//! `hello_ecu` — bring up a `CanBus` over the simulated network, send a frame
//! from one node, and receive it on another. Run with:
//!
//! ```sh
//! cargo run -p tpt-chassis-core --example hello_ecu
//! ```

use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId, CAN_MAX_DLC};
use tpt_chassis_sim::can::SimCanNetwork;

fn main() {
    let net = SimCanNetwork::new();
    let mut tx = CanBus::new(net.node());
    let mut rx = CanBus::new(net.node());

    let id = CanId::standard(0x100).expect("valid standard id");
    let frame = CanFrame::new(id, &[0xDE, 0xAD, 0xBE, 0xEF]).expect("valid frame");

    println!(
        "transmitting frame id=0x{:X} data={:?}",
        frame.id().raw(),
        frame.data()
    );
    tx.transmit(frame).expect("transmit ok");

    assert!(rx.can_receive(), "receiver should have a pending frame");
    let received = rx.receive().expect("receive ok");
    assert_eq!(received.data(), &[0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(received.id().raw(), 0x100);

    println!(
        "received frame id=0x{:X} data={:?} ({} bytes)",
        received.id().raw(),
        received.data(),
        received.data().len()
    );
    assert_eq!(received.data().len(), CAN_MAX_DLC.min(4));
    println!("hello_ecu: CAN loopback works");
}
