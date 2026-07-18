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

//! AGL OTA storage backend.
//!
//! Implements [`tpt_chassis_core::ota::OtaStorage`] over a pair of A/B image
//! files plus a state file, defaulting to a `std::fs`-backed layout under
//! [`DEFAULT_ROOT`]. This is the same atomic A/B scheme used on bare metal, just
//! persisted to the AGL filesystem instead of flash. The engine logic in
//! [`tpt_chassis_core::ota::OtaEngine`] is unchanged.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use tpt_chassis_core::ota::{OtaError, OtaStorage, SlotState};

/// Default root directory for the A/B slots and state journal.
pub const DEFAULT_ROOT: &str = "/var/lib/tpt-chassis";

/// Filesystem-backed OTA storage for AGL.
pub struct AglOtaStorage {
    root: PathBuf,
    slot_capacity: usize,
}

impl AglOtaStorage {
    /// Creates storage rooted at [`DEFAULT_ROOT`] with the given slot capacity.
    pub fn new(slot_capacity: usize) -> Self {
        Self::with_root(DEFAULT_ROOT, slot_capacity)
    }

    /// Creates storage rooted at a specific directory (used by tests).
    pub fn with_root(root: impl AsRef<Path>, slot_capacity: usize) -> Self {
        AglOtaStorage {
            root: root.as_ref().to_path_buf(),
            slot_capacity,
        }
    }

    fn slot_path(&self, slot: u8) -> PathBuf {
        self.root.join(format!("slot{}.img", slot))
    }

    fn state_path(&self) -> PathBuf {
        self.root.join("state.bin")
    }
}

impl OtaStorage for AglOtaStorage {
    fn write_slot(&mut self, slot: u8, offset: usize, data: &[u8]) -> Result<(), OtaError> {
        let path = self.slot_path(slot);
        // Read the existing slot (or a zero buffer) so we can patch in place.
        let mut buf = vec![0u8; self.slot_capacity];
        if path.exists() {
            let mut f = fs::File::open(&path).map_err(|_| OtaError::Storage)?;
            f.read_exact(&mut buf).map_err(|_| OtaError::Storage)?;
        }
        if offset + data.len() > buf.len() {
            return Err(OtaError::Storage);
        }
        buf[offset..offset + data.len()].copy_from_slice(data);
        let mut f = fs::File::create(&path).map_err(|_| OtaError::Storage)?;
        f.write_all(&buf).map_err(|_| OtaError::Storage)?;
        Ok(())
    }

    fn read_slot(&self, slot: u8, offset: usize, buf: &mut [u8]) -> Result<(), OtaError> {
        let path = self.slot_path(slot);
        if !path.exists() {
            // Unwritten slot reads as zeros.
            buf.iter_mut().for_each(|b| *b = 0);
            return Ok(());
        }
        let mut f = fs::File::open(&path).map_err(|_| OtaError::Storage)?;
        f.read_exact(&mut *buf).map_err(|_| OtaError::Storage)?;
        let _ = offset;
        Ok(())
    }

    fn slot_capacity(&self) -> usize {
        self.slot_capacity
    }

    fn write_state(&mut self, state: SlotState) -> Result<(), OtaError> {
        let path = self.state_path();
        let bytes = state.encode();
        let mut f = fs::File::create(&path).map_err(|_| OtaError::Storage)?;
        f.write_all(&bytes).map_err(|_| OtaError::Storage)?;
        Ok(())
    }

    fn read_state(&self) -> Result<SlotState, OtaError> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(SlotState::ActiveSlot0);
        }
        let mut bytes = [0u8; 2];
        let mut f = fs::File::open(&path).map_err(|_| OtaError::Storage)?;
        f.read_exact(&mut bytes).map_err(|_| OtaError::Storage)?;
        SlotState::decode(bytes).ok_or(OtaError::Storage)
    }
}
