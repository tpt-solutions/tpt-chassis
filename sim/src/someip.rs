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

//! Simulated SOME/IP Ethernet network for host-side testing.
//!
//! Models a switched automotive Ethernet segment: a SOME/IP message sent by
//! one node is delivered to every other node, irrespective of service — the
//! simulator does not implement routing tables, keeping it minimal and
//! deterministic for unit tests.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use tpt_chassis_core::bus::BusError;
use tpt_chassis_core::someip::{SomeIpMessage, SomeIpTransceiver};

/// Maximum received messages buffered per simulated SOME/IP node.
pub const SIM_SOMEIP_RX_QUEUE_LEN: usize = 16;

/// A shared, in-memory SOME/IP network.
#[derive(Clone, Default)]
pub struct SimSomeIpNetwork {
    inner: Rc<RefCell<SimSomeIpNetworkInner>>,
}

#[derive(Default)]
struct SimSomeIpNetworkInner {
    nodes: Vec<Rc<RefCell<VecDeque<SomeIpMessage>>>>,
    down: bool,
}

impl SimSomeIpNetwork {
    /// Creates an empty SOME/IP network.
    pub fn new() -> Self {
        Self::default()
    }

    /// Brings the network link down (drops traffic, rejects sends).
    pub fn set_link_down(&self, down: bool) {
        self.inner.borrow_mut().down = down;
    }

    /// Attaches a node to the network.
    pub fn node(&self) -> SimSomeIpNode {
        let queue: Rc<RefCell<VecDeque<SomeIpMessage>>> = Rc::new(RefCell::new(
            VecDeque::with_capacity(SIM_SOMEIP_RX_QUEUE_LEN),
        ));
        self.inner.borrow_mut().nodes.push(Rc::clone(&queue));
        SimSomeIpNode {
            network: SimSomeIpNetwork {
                inner: Rc::clone(&self.inner),
            },
            queue,
        }
    }

    fn broadcast(&self, message: SomeIpMessage, sender: &Rc<RefCell<VecDeque<SomeIpMessage>>>) {
        let inner = self.inner.borrow_mut();
        if inner.down {
            return;
        }
        for node in inner.nodes.iter() {
            if !Rc::ptr_eq(node, sender) {
                let mut q = node.borrow_mut();
                if q.len() < SIM_SOMEIP_RX_QUEUE_LEN {
                    q.push_back(message);
                }
            }
        }
    }
}

/// A simulated SOME/IP node implementing [`SomeIpTransceiver`].
pub struct SimSomeIpNode {
    network: SimSomeIpNetwork,
    queue: Rc<RefCell<VecDeque<SomeIpMessage>>>,
}

impl SomeIpTransceiver for SimSomeIpNode {
    fn send(&mut self, message: SomeIpMessage) -> Result<(), BusError> {
        if self.network.inner.borrow().down {
            return Err(BusError::BusOff);
        }
        self.network.broadcast(message, &self.queue);
        Ok(())
    }

    fn recv(&mut self) -> Result<SomeIpMessage, BusError> {
        self.queue
            .borrow_mut()
            .pop_front()
            .ok_or(BusError::RxQueueEmpty)
    }

    fn has_received(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn can_send(&self) -> bool {
        !self.network.inner.borrow().down
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chassis_core::bus::VehicleBus;
    use tpt_chassis_core::someip::{
        SomeIpBus, SomeIpHeader, SomeIpMessageId, SomeIpMessageType, SomeIpRequestId,
        SomeIpReturnCode,
    };

    fn msg(service: u16, method: u16, payload: &[u8]) -> tpt_chassis_core::someip::SomeIpMessage {
        let header = SomeIpHeader {
            message_id: SomeIpMessageId { service, method },
            request_id: SomeIpRequestId {
                client: 1,
                session: 1,
            },
            interface_version: 1,
            message_type: SomeIpMessageType::Request,
            return_code: SomeIpReturnCode::Ok,
        };
        SomeIpMessage::new(header, payload).unwrap()
    }

    #[test]
    fn message_broadcasts_to_other_nodes() {
        let net = SimSomeIpNetwork::new();
        let mut a = SomeIpBus::new(net.node());
        let mut b = SomeIpBus::new(net.node());
        a.transmit(msg(0x1111, 0x01, &[0x42])).unwrap();
        assert!(b.can_receive());
        let got = b.receive().unwrap();
        assert_eq!(got.header().message_id.service, 0x1111);
        assert_eq!(got.payload(), &[0x42]);
    }

    #[test]
    fn sender_excluded() {
        let net = SimSomeIpNetwork::new();
        let mut a = SomeIpBus::new(net.node());
        a.transmit(msg(0x2222, 0x01, &[])).unwrap();
        assert!(!a.can_receive());
    }

    #[test]
    fn link_down_rejects_send() {
        let net = SimSomeIpNetwork::new();
        let mut a = SomeIpBus::new(net.node());
        let b = SomeIpBus::new(net.node());
        net.set_link_down(true);
        assert_eq!(a.transmit(msg(0x3333, 0x01, &[1])), Err(BusError::BusOff));
        assert!(!b.can_receive());
    }
}
