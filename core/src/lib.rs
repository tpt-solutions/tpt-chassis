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

//! # TPT Chassis Core
//!
//! Core, `no_std`-compatible building blocks for the TPT Chassis vehicle
//! operating system — an open-source, memory-safe AUTOSAR replacement.
//!
//! The crate provides a unified vehicle-bus abstraction ([`bus`]) implemented
//! for CAN ([`can`]), automotive Ethernet/SOME/IP ([`someip`]), and LIN
//! ([`lin`]), plus AUTOSAR-compatible interfaces ([`autosar`]), UDS diagnostics
//! ([`uds`]), a secure atomic OTA engine ([`ota`]), the autonomous-driving
//! plugin contract ([`autonomy`]), and the safety interlock/telemetry
//! primitives ([`safety`]).
//!
//! # Modules
//!
//! - [`bus`] — the transport-agnostic [`bus::VehicleBus`] trait and the
//!   [`bus::Frame`] contract shared by every network type.
//! - [`can`] — the CAN frame type and the [`can::CanBus`] high-level interface.
//! - [`lin`] — the LIN frame type and the [`lin::LinBus`] interface.
//! - [`someip`] — the SOME/IP message type and the [`someip::SomeIpBus`]
//!   interface for automotive Ethernet.
//! - [`autosar`] — safe-Rust AUTOSAR equivalents (DIO driver, COM signals).
//! - [`uds`] — UDS (ISO 14229) diagnostic server over CAN.
//! - [`isotp`] - ISO-TP (ISO 15765-2) segmentation for multi-frame CAN messages.
//! - [`ota`] — secure, atomic A/B update engine with rollback.
//! - [`autonomy`] — the [`autonomy::AutonomyStack`] plugin contract and a
//!   reference lane-keeping stack.
//! - [`safety`] — the [`safety::Interlock`] kill-switch and
//!   [`safety::TelemetryRing`] field-logging buffer.

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod autonomy;
pub mod autosar;
pub mod bus;
pub mod can;
pub mod isotp;
pub mod lin;
pub mod ota;
pub mod safety;
pub mod someip;
pub mod uds;

/// Crate version, matching `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Re-exports the license identifier string for embedded diagnostics.
pub const LICENSE: &str = "MIT OR Apache-2.0";

/// Common error type used across the TPT Chassis core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// The requested operation is not supported in the current configuration.
    Unsupported,
    /// A resource was exhausted (e.g. buffer full).
    ResourceExhausted,
    /// An invalid argument or configuration was supplied.
    InvalidArgument,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Error::Unsupported => "operation not supported",
            Error::ResourceExhausted => "resource exhausted",
            Error::InvalidArgument => "invalid argument",
        };
        f.write_str(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn license_identifier_matches_manifest() {
        assert_eq!(LICENSE, "MIT OR Apache-2.0");
    }

    #[test]
    fn error_display_is_readable() {
        use core::fmt::Write;
        struct Buf {
            data: [u8; 32],
            len: usize,
        }
        impl core::fmt::Write for Buf {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let bytes = s.as_bytes();
                if self.len + bytes.len() > self.data.len() {
                    return Err(core::fmt::Error);
                }
                self.data[self.len..self.len + bytes.len()].copy_from_slice(bytes);
                self.len += bytes.len();
                Ok(())
            }
        }
        let mut buf = Buf {
            data: [0u8; 32],
            len: 0,
        };
        let _ = core::write!(buf, "{}", Error::Unsupported);
        let printed = core::str::from_utf8(&buf.data[..buf.len]).unwrap();
        assert!(printed.starts_with("operation not supported"));
    }
}
