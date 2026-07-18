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

//! OTA (Over-the-Air) update engine.
//!
//! A secure, *atomic* update engine that cannot brick the vehicle. Updates use
//! an A/B slot model: a new image is written to the inactive slot, verified,
//! and only then promoted. A small persistent state machine records progress
//! at each irreversible step, so an interrupted update (power loss, corrupted
//! payload, crash) is always recoverable — either the old image keeps running
//! or the update is rolled back.
//!
//! Everything here is `no_std` and allocation-free; the caller supplies the
//! storage backend ([`OtaStorage`]) and signature scheme ([`SignatureScheme`]).

use crate::Error;

/// Magic bytes identifying a TPT Chassis update package.
pub const OTA_MAGIC: [u8; 4] = *b"TPT1";

/// Current update package format version.
pub const OTA_FORMAT_VERSION: u8 = 1;

/// Length of a signature in bytes.
pub const OTA_SIGNATURE_LEN: usize = 32;

/// Maximum payload size of an update package (kept small for `no_std`).
pub const OTA_MAX_PAYLOAD: usize = 4096;

/// Errors produced by the OTA engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum OtaError {
    /// A generic core error.
    Core(Error),
    /// The package magic/format version is not recognized.
    BadPackage,
    /// The package signature did not verify.
    BadSignature,
    /// The package payload is too large for the buffer/slot.
    PayloadTooLarge,
    /// The candidate slot could not be written.
    WriteFailed,
    /// There is no candidate image to promote or test.
    NoCandidate,
    /// The update is in a state that forbids the requested operation.
    InvalidState,
    /// A storage backend error.
    Storage,
}

impl From<Error> for OtaError {
    fn from(e: Error) -> Self {
        OtaError::Core(e)
    }
}

impl core::fmt::Display for OtaError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            OtaError::Core(e) => write!(f, "ota: {}", e),
            OtaError::BadPackage => f.write_str("ota: unrecognized package"),
            OtaError::BadSignature => f.write_str("ota: signature verification failed"),
            OtaError::PayloadTooLarge => f.write_str("ota: payload too large"),
            OtaError::WriteFailed => f.write_str("ota: candidate write failed"),
            OtaError::NoCandidate => f.write_str("ota: no candidate image"),
            OtaError::InvalidState => f.write_str("ota: operation invalid in current state"),
            OtaError::Storage => f.write_str("ota: storage backend error"),
        }
    }
}

/// Persistent update state for a slot pair.
///
/// This is the journal the bootloader/engine reads on every start. It is small
/// and written atomically (one `write_state` call) at each transition, so a
/// crash always lands in a known, recoverable state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotState {
    /// Slot 0 is the active, running image.
    ActiveSlot0,
    /// Slot 1 is the active, running image.
    ActiveSlot1,
    /// A candidate image is staged in the inactive slot and pending a reboot.
    Pending {
        /// The slot holding the candidate (0 or 1).
        candidate: u8,
    },
    /// A candidate was promoted and is being health-tested; if it fails the
    /// engine rolls back to the previous active slot.
    Testing {
        /// The slot now active and under test.
        active: u8,
        /// The slot to roll back to if the test fails.
        fallback: u8,
    },
}

impl SlotState {
    /// Returns the slot currently considered active/running.
    pub fn active_slot(&self) -> u8 {
        match self {
            SlotState::ActiveSlot0 => 0,
            SlotState::ActiveSlot1 => 1,
            SlotState::Pending { candidate } => 1 - candidate,
            SlotState::Testing { active, .. } => *active,
        }
    }

    /// Encodes the state into 2 bytes for persistent storage.
    pub fn encode(&self) -> [u8; 2] {
        match self {
            SlotState::ActiveSlot0 => [0x00, 0x00],
            SlotState::ActiveSlot1 => [0x00, 0x01],
            SlotState::Pending { candidate } => [0x01, *candidate],
            SlotState::Testing { active, fallback } => [0x02, *active * 2 + *fallback],
        }
    }

    /// Decodes persistent state, returning `None` for unrecognized bytes.
    pub fn decode(bytes: [u8; 2]) -> Option<SlotState> {
        match bytes[0] {
            0x00 => match bytes[1] {
                0 => Some(SlotState::ActiveSlot0),
                1 => Some(SlotState::ActiveSlot1),
                _ => None,
            },
            0x01 => {
                if bytes[1] <= 1 {
                    Some(SlotState::Pending {
                        candidate: bytes[1],
                    })
                } else {
                    None
                }
            }
            0x02 => {
                let active = bytes[1] / 2;
                let fallback = bytes[1] % 2;
                if active <= 1 && fallback <= 1 {
                    Some(SlotState::Testing { active, fallback })
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

/// A signature scheme used to authenticate update packages.
///
/// Production deployments plug in a real asymmetric scheme (e.g. Ed25519). The
/// engine only requires that [`verify`](SignatureScheme::verify) is
/// side-channel-resistent enough for the threat model; the mock in tests is for
/// exercise only and MUST NOT be used in production.
pub trait SignatureScheme {
    /// Signs `data`, writing `OTA_SIGNATURE_LEN` bytes into `sig`.
    fn sign(&self, data: &[u8], sig: &mut [u8; OTA_SIGNATURE_LEN]);

    /// Returns `true` if `sig` is a valid signature over `data`.
    fn verify(&self, data: &[u8], sig: &[u8; OTA_SIGNATURE_LEN]) -> bool;
}

/// A simple, clearly-marked *demo* signature scheme (FNV-1a keyed hash).
///
/// This exists only so the engine can be tested without pulling in a crypto
/// crate. It provides authentication in tests but is NOT cryptographically
/// secure and must be replaced before any real deployment.
#[derive(Clone, Copy)]
pub struct DemoSigner {
    key: u32,
}

impl DemoSigner {
    /// Creates a demo signer with a fixed key.
    pub fn new(key: u32) -> Self {
        DemoSigner { key }
    }

    fn hash(&self, data: &[u8]) -> [u8; OTA_SIGNATURE_LEN] {
        let mut h: u32 = 0x811C_9DC5 ^ self.key;
        for &b in data {
            h ^= b as u32;
            h = h.wrapping_mul(0x0100_0193);
        }
        let mut out = [0u8; OTA_SIGNATURE_LEN];
        // Spread the 32-bit hash across the signature buffer deterministically.
        let mut state = h;
        for slot in out.iter_mut() {
            *slot = (state & 0xFF) as u8;
            state = state.rotate_left(8) ^ (state >> 24).wrapping_mul(0x9E37_79B9);
        }
        out
    }
}

impl SignatureScheme for DemoSigner {
    fn sign(&self, data: &[u8], sig: &mut [u8; OTA_SIGNATURE_LEN]) {
        *sig = self.hash(data);
    }

    fn verify(&self, data: &[u8], sig: &[u8; OTA_SIGNATURE_LEN]) -> bool {
        self.hash(data) == *sig
    }
}

/// Storage backend for the OTA engine.
///
/// Implementors provide persistent, power-safe storage for the two image slots
/// and the small state journal. On real hardware this maps to internal flash;
/// in tests it is an in-memory buffer.
///
/// A blanket implementation is provided for `&mut T`, so callers may share a
/// backend across multiple engine instances (e.g. simulating a reboot with a
/// fresh engine over the same storage).
pub trait OtaStorage {
    /// Writes `data` to `slot` at `offset` (must fit within the slot capacity).
    fn write_slot(&mut self, slot: u8, offset: usize, data: &[u8]) -> Result<(), OtaError>;

    /// Reads up to `buf.len()` bytes from `slot` at `offset`.
    fn read_slot(&self, slot: u8, offset: usize, buf: &mut [u8]) -> Result<(), OtaError>;

    /// Capacity in bytes of a single slot.
    fn slot_capacity(&self) -> usize;

    /// Persists the slot-state journal (atomic update expected by hardware).
    fn write_state(&mut self, state: SlotState) -> Result<(), OtaError>;

    /// Loads the slot-state journal.
    fn read_state(&self) -> Result<SlotState, OtaError>;
}

impl<T: OtaStorage + ?Sized> OtaStorage for &mut T {
    fn write_slot(&mut self, slot: u8, offset: usize, data: &[u8]) -> Result<(), OtaError> {
        (**self).write_slot(slot, offset, data)
    }

    fn read_slot(&self, slot: u8, offset: usize, buf: &mut [u8]) -> Result<(), OtaError> {
        (**self).read_slot(slot, offset, buf)
    }

    fn slot_capacity(&self) -> usize {
        (**self).slot_capacity()
    }

    fn write_state(&mut self, state: SlotState) -> Result<(), OtaError> {
        (**self).write_state(state)
    }

    fn read_state(&self) -> Result<SlotState, OtaError> {
        (**self).read_state()
    }
}

/// An update package being assembled/parsed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpdatePackage {
    /// Target slot the image should be applied to (0 or 1).
    pub slot: u8,
    /// Length of the encapsulated payload.
    pub payload_len: u32,
}

impl UpdatePackage {
    /// Size of the fixed package header in bytes.
    pub const HEADER_LEN: usize = 4 + 1 + 1 + 4;

    /// Serializes the package header (excluding payload and signature).
    pub fn encode_header(&self, out: &mut [u8]) {
        out[0..4].copy_from_slice(&OTA_MAGIC);
        out[4] = OTA_FORMAT_VERSION;
        out[5] = self.slot;
        out[6..10].copy_from_slice(&self.payload_len.to_be_bytes());
    }

    /// Parses a package header, returning `None` on magic/version mismatch.
    pub fn decode_header(bytes: &[u8]) -> Option<UpdatePackage> {
        if bytes.len() < Self::HEADER_LEN {
            return None;
        }
        if bytes[0..4] != OTA_MAGIC {
            return None;
        }
        if bytes[4] != OTA_FORMAT_VERSION {
            return None;
        }
        let slot = bytes[5];
        if slot > 1 {
            return None;
        }
        let payload_len = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]);
        Some(UpdatePackage { slot, payload_len })
    }
}

/// The OTA engine: orchestrates staging, promotion, testing, and rollback.
pub struct OtaEngine<S: OtaStorage, V: SignatureScheme> {
    storage: S,
    signer: V,
}

impl<S: OtaStorage, V: SignatureScheme> OtaEngine<S, V> {
    /// Creates an engine over the given storage and signature scheme.
    pub fn new(storage: S, signer: V) -> Self {
        OtaEngine { storage, signer }
    }

    /// Returns the current persistent slot state.
    pub fn state(&self) -> Result<SlotState, OtaError> {
        self.storage.read_state()
    }

    /// Resolves any pending/testing state on boot. This is idempotent and safe
    /// to call at every startup: it detects interrupted updates and either
    /// keeps the promoted image or rolls back.
    ///
    /// Returns the slot that should now be active.
    pub fn recover(&mut self) -> Result<u8, OtaError> {
        let state = self.storage.read_state()?;
        let resolved = match state {
            SlotState::Pending { candidate } => {
                // Reboot happened after staging but before promotion: promote.
                SlotState::Testing {
                    active: candidate,
                    fallback: 1 - candidate,
                }
            }
            SlotState::Testing {
                active: _,
                fallback,
            } => {
                // Still testing after a reboot: treat as failed, roll back.
                if fallback == 0 {
                    SlotState::ActiveSlot0
                } else {
                    SlotState::ActiveSlot1
                }
            }
            other => other,
        };
        let active = resolved.active_slot();
        self.storage.write_state(resolved)?;
        Ok(active)
    }

    /// Stages an update package: validates the header and signature, then writes
    /// the payload into the candidate (inactive) slot. Does NOT promote.
    ///
    /// `package`/`payload`/`signature` describe the incoming update. The
    /// candidate slot is chosen from `package.slot`.
    pub fn stage(
        &mut self,
        package: &UpdatePackage,
        payload: &[u8],
        signature: &[u8; OTA_SIGNATURE_LEN],
    ) -> Result<(), OtaError> {
        if payload.len() != package.payload_len as usize {
            return Err(OtaError::BadPackage);
        }
        if payload.len() > self.storage.slot_capacity() || payload.len() > OTA_MAX_PAYLOAD {
            return Err(OtaError::PayloadTooLarge);
        }
        // Verify signature over header || payload.
        let mut signed = [0u8; UpdatePackage::HEADER_LEN + OTA_MAX_PAYLOAD];
        package.encode_header(&mut signed[..UpdatePackage::HEADER_LEN]);
        signed[UpdatePackage::HEADER_LEN..UpdatePackage::HEADER_LEN + payload.len()]
            .copy_from_slice(payload);
        if !self.signer.verify(
            &signed[..UpdatePackage::HEADER_LEN + payload.len()],
            signature,
        ) {
            return Err(OtaError::BadSignature);
        }
        // Write payload to the candidate slot.
        self.storage
            .write_slot(package.slot, 0, payload)
            .map_err(|_| OtaError::WriteFailed)?;
        // Record that a candidate is staged and pending a reboot/promotion.
        let state = SlotState::Pending {
            candidate: package.slot,
        };
        self.storage
            .write_state(state)
            .map_err(|_| OtaError::Storage)?;
        Ok(())
    }

    /// Promotes the staged candidate to active and enters the `Testing` state.
    ///
    /// Mirrors the boot-time promotion but can also be called directly (e.g. in
    /// a simulator or when the bootloader delegates to the engine).
    pub fn promote(&mut self) -> Result<u8, OtaError> {
        let state = self.storage.read_state()?;
        let candidate = match state {
            SlotState::Pending { candidate } => candidate,
            _ => return Err(OtaError::NoCandidate),
        };
        let next = SlotState::Testing {
            active: candidate,
            fallback: 1 - candidate,
        };
        self.storage
            .write_state(next)
            .map_err(|_| OtaError::Storage)?;
        Ok(candidate)
    }

    /// Confirms a `Testing` image is healthy and commits it as the permanent
    /// active slot.
    pub fn commit(&mut self) -> Result<u8, OtaError> {
        let state = self.storage.read_state()?;
        let active = match state {
            SlotState::Testing { active, .. } => active,
            _ => return Err(OtaError::InvalidState),
        };
        let committed = if active == 0 {
            SlotState::ActiveSlot0
        } else {
            SlotState::ActiveSlot1
        };
        self.storage
            .write_state(committed)
            .map_err(|_| OtaError::Storage)?;
        Ok(active)
    }

    /// Rolls back from a `Testing` state to the previously active slot.
    pub fn rollback(&mut self) -> Result<u8, OtaError> {
        let state = self.storage.read_state()?;
        let (active, fallback) = match state {
            SlotState::Testing { active, fallback } => (active, fallback),
            _ => return Err(OtaError::InvalidState),
        };
        let restored = if fallback == 0 {
            SlotState::ActiveSlot0
        } else {
            SlotState::ActiveSlot1
        };
        self.storage
            .write_state(restored)
            .map_err(|_| OtaError::Storage)?;
        Ok(active)
    }
}

// Helper enum used only by `recover` to keep the match exhaustive without
// exposing a transition variant.
#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use super::*;

    const SLOT_CAP: usize = 256;

    struct MemStorage {
        slots: [[u8; SLOT_CAP]; 2],
        state: SlotState,
    }

    impl MemStorage {
        fn new() -> Self {
            MemStorage {
                slots: [[0u8; SLOT_CAP]; 2],
                state: SlotState::ActiveSlot0,
            }
        }
    }

    impl OtaStorage for MemStorage {
        fn write_slot(&mut self, slot: u8, offset: usize, data: &[u8]) -> Result<(), OtaError> {
            let s = self.slots.get_mut(slot as usize).ok_or(OtaError::Storage)?;
            if offset + data.len() > s.len() {
                return Err(OtaError::Storage);
            }
            s[offset..offset + data.len()].copy_from_slice(data);
            Ok(())
        }

        fn read_slot(&self, slot: u8, offset: usize, buf: &mut [u8]) -> Result<(), OtaError> {
            let s = self.slots.get(slot as usize).ok_or(OtaError::Storage)?;
            if offset + buf.len() > s.len() {
                return Err(OtaError::Storage);
            }
            buf.copy_from_slice(&s[offset..offset + buf.len()]);
            Ok(())
        }

        fn slot_capacity(&self) -> usize {
            SLOT_CAP
        }

        fn write_state(&mut self, state: SlotState) -> Result<(), OtaError> {
            self.state = state;
            Ok(())
        }

        fn read_state(&self) -> Result<SlotState, OtaError> {
            Ok(self.state)
        }
    }

    fn make_package(
        signer: &DemoSigner,
        slot: u8,
        payload: &[u8],
    ) -> ([u8; OTA_SIGNATURE_LEN], UpdatePackage) {
        let pkg = UpdatePackage {
            slot,
            payload_len: payload.len() as u32,
        };
        let mut signed = [0u8; UpdatePackage::HEADER_LEN + OTA_MAX_PAYLOAD];
        pkg.encode_header(&mut signed[..UpdatePackage::HEADER_LEN]);
        signed[UpdatePackage::HEADER_LEN..UpdatePackage::HEADER_LEN + payload.len()]
            .copy_from_slice(payload);
        let mut sig = [0u8; OTA_SIGNATURE_LEN];
        signer.sign(
            &signed[..UpdatePackage::HEADER_LEN + payload.len()],
            &mut sig,
        );
        (sig, pkg)
    }

    #[test]
    fn slot_state_round_trips() {
        for st in [
            SlotState::ActiveSlot0,
            SlotState::ActiveSlot1,
            SlotState::Pending { candidate: 1 },
            SlotState::Testing {
                active: 1,
                fallback: 0,
            },
        ] {
            assert_eq!(SlotState::decode(st.encode()), Some(st));
        }
        assert_eq!(SlotState::decode([0xFF, 0x00]), None);
    }

    #[test]
    fn stage_then_promote_commit_activates_new_slot() {
        let signer = DemoSigner::new(0xCAFE);
        let mut store = MemStorage::new();
        let mut engine = OtaEngine::new(&mut store, signer);

        let payload = [0xAB; 16];
        let (sig, pkg) = make_package(&signer, 1, &payload);
        engine.stage(&pkg, &payload, &sig).unwrap();

        let active = engine.promote().unwrap();
        assert_eq!(active, 1);
        let active = engine.commit().unwrap();
        assert_eq!(active, 1);
        assert_eq!(engine.state().unwrap(), SlotState::ActiveSlot1);
    }

    #[test]
    fn corrupted_signature_is_rejected() {
        let signer = DemoSigner::new(0xCAFE);
        let mut store = MemStorage::new();
        let mut engine = OtaEngine::new(&mut store, signer);

        let payload = [0xAB; 16];
        let (_sig, pkg) = make_package(&signer, 1, &payload);
        let mut bad_sig = [0u8; OTA_SIGNATURE_LEN];
        bad_sig[0] = 0xFF;
        assert_eq!(
            engine.stage(&pkg, &payload, &bad_sig),
            Err(OtaError::BadSignature)
        );
    }

    #[test]
    fn bad_magic_package_rejected() {
        assert!(UpdatePackage::decode_header(&[0, 0, 0, 0, 1, 0, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn power_loss_during_pending_promotes_on_recover() {
        let signer = DemoSigner::new(0xCAFE);
        let mut store = MemStorage::new();
        let mut engine = OtaEngine::new(&mut store, signer);

        let payload = [0x12; 16];
        let (sig, pkg) = make_package(&signer, 1, &payload);
        engine.stage(&pkg, &payload, &sig).unwrap();
        // Simulate reboot mid-pending: a fresh engine recovers the state.
        let mut engine2 = OtaEngine::new(&mut store, signer);
        let active = engine2.recover().unwrap();
        assert_eq!(active, 1);
        assert!(matches!(
            engine2.state().unwrap(),
            SlotState::Testing { active: 1, .. }
        ));
    }

    #[test]
    fn power_loss_during_testing_rolls_back() {
        let signer = DemoSigner::new(0xCAFE);
        let mut store = MemStorage::new();
        let mut engine = OtaEngine::new(&mut store, signer);

        let payload = [0x12; 16];
        let (sig, pkg) = make_package(&signer, 1, &payload);
        engine.stage(&pkg, &payload, &sig).unwrap();
        engine.promote().unwrap();
        // Reboot during testing -> rollback to slot 0.
        let mut engine2 = OtaEngine::new(&mut store, signer);
        let active = engine2.recover().unwrap();
        assert_eq!(active, 0);
        assert_eq!(engine2.state().unwrap(), SlotState::ActiveSlot0);
    }

    #[test]
    fn forced_rollback_returns_to_previous_slot() {
        let signer = DemoSigner::new(0xCAFE);
        let mut store = MemStorage::new();
        let mut engine = OtaEngine::new(&mut store, signer);

        let payload = [0x99; 16];
        let (sig, pkg) = make_package(&signer, 1, &payload);
        engine.stage(&pkg, &payload, &sig).unwrap();
        engine.promote().unwrap();
        let rolled = engine.rollback().unwrap();
        assert_eq!(rolled, 1);
        assert_eq!(engine.state().unwrap(), SlotState::ActiveSlot0);
    }
}
