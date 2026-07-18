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

//! Simulated LIN bus for host-side testing.
//!
//! Models a LIN cluster: a single master node schedules frames and slaves
//! respond. Frames transmitted by one node are delivered to every other node,
//! matching real LIN broadcast behavior.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

use tpt_chassis_core::bus::BusError;
use tpt_chassis_core::lin::{LinFrame, LinTransceiver};

/// Maximum received frames buffered per simulated LIN node.
pub const SIM_LIN_RX_QUEUE_LEN: usize = 16;

/// A shared, in-memory LIN cluster.
#[derive(Clone, Default)]
pub struct SimLinCluster {
    inner: Rc<RefCell<SimLinClusterInner>>,
}

#[derive(Default)]
struct SimLinClusterInner {
    nodes: Vec<(bool, Rc<RefCell<VecDeque<LinFrame>>>)>,
    bus_off: bool,
}

impl SimLinCluster {
    /// Creates an empty LIN cluster.
    pub fn new() -> Self {
        Self::default()
    }

    /// Forces the cluster bus-off (drops all traffic).
    pub fn set_bus_off(&self, off: bool) {
        self.inner.borrow_mut().bus_off = off;
    }

    /// Attaches a node. `is_master` marks the LIN master.
    pub fn node(&self, is_master: bool) -> SimLinNode {
        let queue: Rc<RefCell<VecDeque<LinFrame>>> =
            Rc::new(RefCell::new(VecDeque::with_capacity(SIM_LIN_RX_QUEUE_LEN)));
        self.inner
            .borrow_mut()
            .nodes
            .push((is_master, Rc::clone(&queue)));
        let network = SimLinCluster {
            inner: Rc::clone(&self.inner),
        };
        SimLinNode {
            network,
            queue,
            is_master,
        }
    }

    fn broadcast(&self, frame: LinFrame, sender: &Rc<RefCell<VecDeque<LinFrame>>>) {
        let inner = self.inner.borrow_mut();
        if inner.bus_off {
            return;
        }
        for (_, q) in inner.nodes.iter() {
            if !Rc::ptr_eq(q, sender) {
                let mut q = q.borrow_mut();
                if q.len() < SIM_LIN_RX_QUEUE_LEN {
                    q.push_back(frame);
                }
            }
        }
    }
}

/// A simulated LIN node implementing [`LinTransceiver`].
pub struct SimLinNode {
    network: SimLinCluster,
    queue: Rc<RefCell<VecDeque<LinFrame>>>,
    is_master: bool,
}

impl LinTransceiver for SimLinNode {
    fn is_master(&self) -> bool {
        self.is_master
    }

    fn send(&mut self, frame: LinFrame) -> Result<(), BusError> {
        if self.network.inner.borrow().bus_off {
            return Err(BusError::BusOff);
        }
        self.network.broadcast(frame, &self.queue);
        Ok(())
    }

    fn recv(&mut self) -> Result<LinFrame, BusError> {
        self.queue
            .borrow_mut()
            .pop_front()
            .ok_or(BusError::RxQueueEmpty)
    }

    fn has_received(&self) -> bool {
        !self.queue.borrow().is_empty()
    }

    fn can_send(&self) -> bool {
        !self.network.inner.borrow().bus_off
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_chassis_core::bus::VehicleBus;
    use tpt_chassis_core::lin::{LinBus, LinId};

    fn lin_frame(id: u8, data: &[u8]) -> LinFrame {
        LinFrame::new(LinId::new(id).unwrap(), data).unwrap()
    }

    #[test]
    fn master_and_slave_receive_broadcast() {
        let net = SimLinCluster::new();
        let mut master = LinBus::new(net.node(true));
        let mut slave = LinBus::new(net.node(false));
        assert!(master.is_master());
        assert!(!slave.is_master());

        master.transmit(lin_frame(0x10, &[1, 2, 3])).unwrap();
        assert!(slave.can_receive());
        let got = slave.receive().unwrap();
        assert_eq!(got.id().raw(), 0x10);
        assert_eq!(got.data(), &[1, 2, 3]);
        assert_eq!(got.checksum(), lin_frame(0x10, &[1, 2, 3]).checksum());
    }

    #[test]
    fn sender_does_not_see_own_frame() {
        let net = SimLinCluster::new();
        let mut master = LinBus::new(net.node(true));
        master.transmit(lin_frame(0x01, &[9])).unwrap();
        assert!(!master.can_receive());
    }

    #[test]
    fn bus_off_stops_traffic() {
        let net = SimLinCluster::new();
        let mut master = LinBus::new(net.node(true));
        let slave = LinBus::new(net.node(false));
        net.set_bus_off(true);
        assert_eq!(
            master.transmit(lin_frame(0x02, &[0])),
            Err(BusError::BusOff)
        );
        assert!(!slave.can_receive());
    }
}
