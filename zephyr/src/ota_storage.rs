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

//! Zephyr OTA storage backend.
//!
//! Implements [`tpt_chassis_core::ota::OtaStorage`] either against the on-device
//! flash partition (with the `flash-ota` feature, linking Zephyr's flash API) or
//! against a small in-RAM buffer (default). The RAM backend lets the exact same
//! [`tpt_chassis_core::ota::OtaEngine`] logic run on `thumbv7em-none-eabihf`
//! under QEMU for CI validation, then drop in real flash on hardware simply by
//! enabling the feature — no changes to the engine or application code.

use tpt_chassis_core::ota::{OtaError, OtaStorage, SlotState};

#[cfg(feature = "flash-ota")]
use crate::ffi;

/// Capacity of each A/B image slot in the RAM fallback (kept tiny for `no_std`).
const RAM_SLOT_CAP: usize = 1024;

/// OTA storage backed by Zephyr flash or, when `flash-ota` is off, by RAM.
pub struct ZephyrOtaStorage {
    #[cfg(not(feature = "flash-ota"))]
    slots: [heapless_ram::RamSlot; 2],
    #[cfg(not(feature = "flash-ota"))]
    state: SlotState,
}

#[cfg(not(feature = "flash-ota"))]
mod heapless_ram {
    use tpt_chassis_core::ota::SlotState;

    /// A fixed-capacity, allocation-free image slot mirror.
    pub struct RamSlot {
        pub data: [u8; super::RAM_SLOT_CAP],
    }

    impl RamSlot {
        pub const fn new() -> Self {
            RamSlot {
                data: [0u8; super::RAM_SLOT_CAP],
            }
        }
    }

    impl Default for RamSlot {
        fn default() -> Self {
            Self::new()
        }
    }

    impl core::ops::Deref for RamSlot {
        type Target = [u8; super::RAM_SLOT_CAP];
        fn deref(&self) -> &Self::Target {
            &self.data
        }
    }

    impl core::ops::DerefMut for RamSlot {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.data
        }
    }

    impl Default for super::ZephyrOtaStorage {
        fn default() -> Self {
            Self::new()
        }
    }

    impl From<&super::ZephyrOtaStorage> for SlotState {
        fn from(_: &super::ZephyrOtaStorage) -> SlotState {
            SlotState::ActiveSlot0
        }
    }
}

impl ZephyrOtaStorage {
    /// Creates a storage backend (RAM fallback, or flash when `flash-ota`).
    pub fn new() -> Self {
        #[cfg(feature = "flash-ota")]
        {
            ZephyrOtaStorage {}
        }
        #[cfg(not(feature = "flash-ota"))]
        {
            ZephyrOtaStorage {
                slots: [heapless_ram::RamSlot::new(), heapless_ram::RamSlot::new()],
                state: SlotState::ActiveSlot0,
            }
        }
    }
}

impl OtaStorage for ZephyrOtaStorage {
    fn write_slot(&mut self, slot: u8, offset: usize, data: &[u8]) -> Result<(), OtaError> {
        #[cfg(feature = "flash-ota")]
        {
            let cap = ffi::slot_capacity();
            if offset + data.len() > cap {
                return Err(OtaError::Storage);
            }
            ffi::with_flash_area(slot, |area| {
                // SAFETY: `area` is valid for this call; `data` is a valid slice
                // of `data.len()` bytes and `offset` is bounds-checked above.
                unsafe { ffi::tpt_flash_write(area, offset, data.as_ptr(), data.len()) }
            })
            .and_then(|rc| if rc == 0 { Some(()) } else { None })
            .ok_or(OtaError::WriteFailed)
        }
        #[cfg(not(feature = "flash-ota"))]
        {
            let s = self.slots.get_mut(slot as usize).ok_or(OtaError::Storage)?;
            if offset + data.len() > s.len() {
                return Err(OtaError::Storage);
            }
            s[offset..offset + data.len()].copy_from_slice(data);
            Ok(())
        }
    }

    fn read_slot(&self, slot: u8, offset: usize, buf: &mut [u8]) -> Result<(), OtaError> {
        #[cfg(feature = "flash-ota")]
        {
            let cap = ffi::slot_capacity();
            if offset + buf.len() > cap {
                return Err(OtaError::Storage);
            }
            ffi::with_flash_area(slot, |area| {
                // SAFETY: `area` is valid for this call; `buf` is a valid slice
                // of `buf.len()` bytes and `offset` is bounds-checked above.
                unsafe { ffi::tpt_flash_read(area, offset, buf.as_mut_ptr(), buf.len()) }
            })
            .and_then(|rc| if rc == 0 { Some(()) } else { None })
            .ok_or(OtaError::Storage)
        }
        #[cfg(not(feature = "flash-ota"))]
        {
            let s = self.slots.get(slot as usize).ok_or(OtaError::Storage)?;
            if offset + buf.len() > s.len() {
                return Err(OtaError::Storage);
            }
            buf.copy_from_slice(&s[offset..offset + buf.len()]);
            Ok(())
        }
    }

    fn slot_capacity(&self) -> usize {
        #[cfg(feature = "flash-ota")]
        {
            ffi::slot_capacity()
        }
        #[cfg(not(feature = "flash-ota"))]
        {
            RAM_SLOT_CAP
        }
    }

    fn write_state(&mut self, state: SlotState) -> Result<(), OtaError> {
        #[cfg(feature = "flash-ota")]
        {
            let bytes = state.encode();
            ffi::state_write(&bytes).map_err(|_| OtaError::Storage)
        }
        #[cfg(not(feature = "flash-ota"))]
        {
            self.state = state;
            Ok(())
        }
    }

    fn read_state(&self) -> Result<SlotState, OtaError> {
        #[cfg(feature = "flash-ota")]
        {
            let mut bytes = [0u8; 2];
            ffi::state_read(&mut bytes).map_err(|_| OtaError::Storage)?;
            SlotState::decode(bytes).ok_or(OtaError::Storage)
        }
        #[cfg(not(feature = "flash-ota"))]
        {
            Ok(self.state)
        }
    }
}
