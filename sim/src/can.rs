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

//! Simulated CAN bus for host-side testing.
//!
//! Provides an in-memory CAN network ([`SimCanNetwork`]) and node endpoints
//! ([`SimCanNode`]) so the `no_std` CAN abstraction in `tpt-chassis-core` can be
//! exercised without any hardware. Frames sent by one node are broadcast to
//! every *other* node on the network, mirroring how a real CAN bus behaves.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use tpt_chassis_core::bus::BusError;
use tpt_chassis_core::can::{CanFrame, CanTransceiver};

/// Maximum number of received frames buffered per simulated node.
pub const SIM_RX_QUEUE_LEN: usize = 16;

/// A shared, in-memory CAN network.
///
/// `SimCanNetwork` is cheap to clone (it wraps an `Rc`); every clone refers to
/// the same underlying bus. Attach nodes with [`SimCanNetwork::node`].
#[derive(Clone, Default)]
pub struct SimCanNetwork {
    inner: Rc<RefCell<SimCanNetworkInner>>,
}

#[derive(Default)]
struct SimCanNetworkInner {
    /// All node RX queues currently attached to the bus.
    nodes: Vec<Rc<RefCell<VecDeque<CanFrame>>>>,
    /// When `true`, transmitted frames are dropped instead of broadcast.
    bus_off: bool,
}

impl SimCanNetwork {
    /// Creates an empty simulated CAN network.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forces the bus into a fault ("bus-off") state. While bus-off, transmits
    /// are rejected with [`BusError::BusOff`] and no frames are broadcast.
    pub fn set_bus_off(&self, off: bool) {
        self.inner.borrow_mut().bus_off = off;
    }

    /// Returns `true` if the network is currently in a bus-off fault state.
    pub fn is_bus_off(&self) -> bool {
        self.inner.borrow().bus_off
    }

    /// Attaches a new node to the network and returns its endpoint.
    pub fn node(&self) -> SimCanNode {
        let queue: Rc<RefCell<VecDeque<CanFrame>>> =
            Rc::new(RefCell::new(VecDeque::with_capacity(SIM_RX_QUEUE_LEN)));
        self.inner.borrow_mut().nodes.push(Rc::clone(&queue));
        SimCanNode {
            network: Rc::clone(&self.inner),
            queue,
        }
    }

    /// Broadcasts `frame` to every node except `sender`.
    fn broadcast(&self, frame: CanFrame, sender: &Rc<RefCell<VecDeque<CanFrame>>>) {
        let inner = self.inner.borrow_mut();
        if inner.bus_off {
            return;
        }
        for node in inner.nodes.iter() {
            if !Rc::ptr_eq(node, sender) {
                let mut q = node.borrow_mut();
                if q.len() < SIM_RX_QUEUE_LEN {
                    q.push_back(frame);
                }
            }
        }
    }
}

/// A node attached to a [`SimCanNetwork`].
///
/// Implements [`CanTransceiver`] so it can be wrapped by
/// `tpt_chassis_core::can::CanBus` and driven through the unified
/// [`tpt_chassis_core::bus::VehicleBus`] API.
pub struct SimCanNode {
    network: Rc<RefCell<SimCanNetworkInner>>,
    queue: Rc<RefCell<VecDeque<CanFrame>>>,
}

impl CanTransceiver for SimCanNode {
    fn send(&mut self, frame: CanFrame) -> Result<(), BusError> {
        let inner = self.network.borrow();
        if inner.bus_off {
            return Err(BusError::BusOff);
        }
        drop(inner);
        let network = SimCanNetwork {
            inner: Rc::clone(&self.network),
        };
        network.broadcast(frame, &self.queue);
        Ok(())
    }

    fn recv(&mut self) -> Result<CanFrame, BusError> {
        self.queue
            .borrow_mut()
            .pop_front()
            .ok_or(BusError::RxQueueEmpty)
    }

    fn has_received(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn can_send(&self) -> bool {
        !self.network.borrow().bus_off
    }
}

/// Builds a [`CanFrame`] for tests; panics on malformed input (test-only).
#[cfg(test)]
pub(crate) fn make_frame(id: u32, extended: bool, data: &[u8]) -> CanFrame {
    use tpt_chassis_core::can::CanId;
    let id = if extended {
        CanId::extended(id)
    } else {
        CanId::standard(id)
    }
    .unwrap();
    CanFrame::new(id, data).expect("valid frame in test")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chassis_core::bus::VehicleBus;
    use tpt_chassis_core::can::{CanBus, CAN_MAX_DLC};

    #[test]
    fn frames_broadcast_to_other_nodes() {
        let net = SimCanNetwork::new();
        let mut a = CanBus::new(net.node());
        let mut b = CanBus::new(net.node());
        let mut c = CanBus::new(net.node());

        let frame = make_frame(0x123, false, &[0xDE, 0xAD]);
        a.transmit(frame).unwrap();

        assert!(b.can_receive());
        assert!(c.can_receive());
        let from_b = b.receive().unwrap();
        assert_eq!(from_b.data(), &[0xDE, 0xAD]);
        assert_eq!(from_b.id().raw(), 0x123);

        assert_eq!(c.receive().unwrap().data(), &[0xDE, 0xAD]);
    }

    #[test]
    fn sender_does_not_receive_own_frame() {
        let net = SimCanNetwork::new();
        let mut a = CanBus::new(net.node());
        let frame = make_frame(0x1, false, &[1]);
        a.transmit(frame).unwrap();
        assert!(!a.can_receive());
        assert_eq!(a.receive(), Err(BusError::RxQueueEmpty));
    }

    #[test]
    fn empty_receive_queue_errors() {
        let net = SimCanNetwork::new();
        let mut a = CanBus::new(net.node());
        assert_eq!(a.receive(), Err(BusError::RxQueueEmpty));
    }

    #[test]
    fn bus_off_rejects_transmit() {
        let net = SimCanNetwork::new();
        let mut a = CanBus::new(net.node());
        let b = CanBus::new(net.node());
        net.set_bus_off(true);
        assert!(!a.can_transmit());
        let frame = make_frame(0x2, false, &[9]);
        assert_eq!(a.transmit(frame), Err(BusError::BusOff));
        assert!(!b.can_receive());
    }

    #[test]
    fn extended_id_frame_round_trips() {
        let net = SimCanNetwork::new();
        let mut a = CanBus::new(net.node());
        let mut b = CanBus::new(net.node());
        let frame = make_frame(0x18DAF100, true, &[0xAA; CAN_MAX_DLC]);
        a.transmit(frame).unwrap();
        let got = b.receive().unwrap();
        assert!(got.id().is_extended());
        assert_eq!(got.id().raw(), 0x18DAF100);
        assert_eq!(got.data().len(), CAN_MAX_DLC);
    }

    #[test]
    fn rx_queue_overflow_drops_silently() {
        let net = SimCanNetwork::new();
        let mut producer = CanBus::new(net.node());
        let mut consumer = CanBus::new(net.node());
        for i in 0..(SIM_RX_QUEUE_LEN + 4) {
            let frame = make_frame(0x10, false, &[(i & 0xFF) as u8]);
            producer.transmit(frame).unwrap();
        }
        let mut count = 0;
        while consumer.can_receive() {
            consumer.receive().unwrap();
            count += 1;
        }
        assert_eq!(count, SIM_RX_QUEUE_LEN);
    }
}
