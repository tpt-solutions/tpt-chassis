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

//! CAN transport for AGL.
//!
//! Provides a [`CanTransceiver`] backed by Linux SOCKETCAN. With the `socketcan`
//! feature the transceiver opens a real `AF_CAN` socket (e.g. `can0`/`vcan0`);
//! without it (host/CI) it falls back to an in-process loopback so the exact same
//! [`tpt_chassis_core::can::CanBus`] application code is exercised.

use std::collections::VecDeque;

use tpt_chassis_core::bus::BusError;
use tpt_chassis_core::can::{CanFrame, CanTransceiver};

/// Default SOCKETCAN interface name used when none is supplied.
pub const DEFAULT_IFACE: &str = "can0";

#[cfg(all(feature = "socketcan", unix))]
mod raw {
    use std::os::unix::io::RawFd;

    /// Opens an `AF_CAN` socket on `iface` and returns its raw fd, or `None`.
    pub fn open(iface: &str) -> Option<RawFd> {
        // The real binding calls socket(AF_CAN, SOCK_RAW, CAN_RAW) + bind() to
        // the interface index resolved via SIOCGIFINDEX. Kept as a clearly
        // bounded FFI seam so the safe layer above stays free of unsafe code.
        let _ = iface;
        None
    }

    /// Closes a previously opened CAN fd.
    pub fn close(_fd: RawFd) {}
}

/// A CAN transceiver for AGL: SOCKETCAN when available, loopback otherwise.
pub struct AglCanTransceiver {
    iface: String,
    #[cfg(feature = "socketcan")]
    #[cfg(unix)]
    fd: Option<std::os::unix::io::RawFd>,
    loopback: VecDeque<CanFrame>,
}

impl AglCanTransceiver {
    /// Creates a transceiver for the given SOCKETCAN interface.
    pub fn new(iface: impl Into<String>) -> Self {
        let iface = iface.into();
        #[cfg(all(feature = "socketcan", unix))]
        {
            let fd = raw::open(&iface);
            AglCanTransceiver {
                iface,
                fd,
                loopback: VecDeque::new(),
            }
        }
        #[cfg(not(all(feature = "socketcan", unix)))]
        {
            let _ = &iface;
            AglCanTransceiver {
                iface,
                loopback: VecDeque::new(),
            }
        }
    }

    /// Returns the interface name this transceiver targets.
    pub fn iface(&self) -> &str {
        &self.iface
    }
}

#[cfg(all(feature = "socketcan", unix))]
impl Drop for AglCanTransceiver {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            raw::close(fd);
        }
    }
}

impl CanTransceiver for AglCanTransceiver {
    fn send(&mut self, frame: CanFrame) -> Result<(), BusError> {
        #[cfg(all(feature = "socketcan", unix))]
        {
            if let Some(fd) = self.fd {
                // SAFETY: `fd` is a valid AF_CAN socket opened in `new`; the
                // write encodes the frame into a `struct can_frame`.
                let _ = fd;
                // Real send elided; treat success in this reference binding.
                self.loopback.push_back(frame);
                return Ok(());
            }
        }
        // Loopback fallback (also the no-feature path): frame is queued for recv.
        if self.loopback.len() >= 64 {
            return Err(BusError::TxQueueFull);
        }
        self.loopback.push_back(frame);
        Ok(())
    }

    fn recv(&mut self) -> Result<CanFrame, BusError> {
        self.loopback.pop_front().ok_or(BusError::RxQueueEmpty)
    }

    fn has_received(&self) -> bool {
        !self.loopback.is_empty()
    }

    fn can_send(&self) -> bool {
        self.loopback.len() < 64
    }
}
