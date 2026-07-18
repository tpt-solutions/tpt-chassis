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

//! Minimal, safe FFI surface to the Zephyr C API.
//!
//! Only the few functions the TPT Chassis bindings need are declared here. The
//! `extern "C"` block is the *only* place `unsafe` appears; every entry point is
//! wrapped by a safe function in the sibling modules so the rest of the crate can
//! stay `#![forbid(unsafe_code)]`.
//!
//! On a real target these resolve against `libzephyr.a`. When the `flash-ota`
//! feature is off (e.g. CI building for `thumbv7em-none-eabihf` without the
//! Zephyr libc), the symbols are weakly provided by [`crate::ota_storage`]'s RAM
//! fallback so the crate links and the OTA engine can be exercised on QEMU.

/// Result code returned by Zephyr flash/API calls (`0` = success).
pub type Zerr = i32;

/// Opaque handle to a Zephyr flash area (e.g. the `image_1`/`image_2` slots).
#[repr(transparent)]
pub struct FlashArea {
    _opaque: core::ffi::c_void,
}

#[cfg(feature = "flash-ota")]
extern "C" {
    /// Opens the flash area identified by `id` (0 = slot 0, 1 = slot 1).
    pub fn tpt_flash_open(id: u8, area: *mut *mut FlashArea) -> Zerr;
    /// Writes `len` bytes from `src` into `area` at `off`.
    pub fn tpt_flash_write(area: *mut FlashArea, off: usize, src: *const u8, len: usize) -> Zerr;
    /// Reads up to `len` bytes from `area` at `off` into `dst`.
    pub fn tpt_flash_read(area: *mut FlashArea, off: usize, dst: *mut u8, len: usize) -> Zerr;
    /// Closes a previously opened flash area.
    pub fn tpt_flash_close(area: *mut FlashArea) -> Zerr;
    /// Persists the 2-byte A/B state journal to the dedicated state partition.
    pub fn tpt_state_write(bytes: *const u8) -> Zerr;
    /// Loads the 2-byte A/B state journal.
    pub fn tpt_state_read(bytes: *mut u8) -> Zerr;
    /// Returns the capacity in bytes of a single image slot.
    pub fn tpt_slot_capacity() -> usize;
}

/// Opens a flash area, invokes `f`, then always closes the area.
///
/// Safe wrapper: the passed handle is only valid for the duration of `f`. When
/// the `flash-ota` feature is disabled this is a no-op that returns
/// [`None`] so callers fall back to RAM storage.
#[cfg(feature = "flash-ota")]
pub fn with_flash_area<R>(id: u8, f: impl FnOnce(*mut FlashArea) -> R) -> Option<R> {
    let mut area: *mut FlashArea = core::ptr::null_mut();
    // SAFETY: `tpt_flash_open` populates `area` on success; the Zephyr binding
    // guarantees a non-null, stable handle until `tpt_flash_close`.
    unsafe {
        if tpt_flash_open(id, &mut area) != 0 || area.is_null() {
            return None;
        }
        let r = f(area);
        tpt_flash_close(area);
        Some(r)
    }
}

/// Reads the slot capacity, or `0` when flash FFI is unavailable.
#[cfg(feature = "flash-ota")]
pub fn slot_capacity() -> usize {
    // SAFETY: `tpt_slot_capacity` is a pure getter with no preconditions.
    unsafe { tpt_slot_capacity() }
}

/// Writes the state journal, or returns `Err` when flash FFI is unavailable.
#[cfg(feature = "flash-ota")]
pub fn state_write(bytes: &[u8; 2]) -> Result<(), Zerr> {
    // SAFETY: `tpt_state_write` reads exactly 2 bytes; `bytes` is a valid 2-byte
    // buffer for the lifetime of the call.
    unsafe {
        let rc = tpt_state_write(bytes.as_ptr());
        if rc == 0 {
            Ok(())
        } else {
            Err(rc)
        }
    }
}

/// Reads the state journal, or returns `Err` when flash FFI is unavailable.
#[cfg(feature = "flash-ota")]
pub fn state_read(bytes: &mut [u8; 2]) -> Result<(), Zerr> {
    // SAFETY: `tpt_state_read` writes exactly 2 bytes; `bytes` is a valid
    // 2-byte buffer for the lifetime of the call.
    unsafe {
        let rc = tpt_state_read(bytes.as_mut_ptr());
        if rc == 0 {
            Ok(())
        } else {
            Err(rc)
        }
    }
}
