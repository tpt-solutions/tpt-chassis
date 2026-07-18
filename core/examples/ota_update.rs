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

//! `ota_update` — exercise the A/B OTA engine lifecycle: stage → promote →
//! commit, plus a power-loss-during-pending recovery and a corrupted-signature
//! rejection. Uses an in-memory storage backend and the demo signer (swap in a
//! real `SignatureScheme` for production). Run with:
//!
//! ```sh
//! cargo run -p tpt-chassis-core --example ota_update
//! ```

use tpt_chassis_core::ota::{
    DemoSigner, OtaEngine, OtaError, SignatureScheme, UpdatePackage, OTA_MAX_PAYLOAD,
    OTA_SIGNATURE_LEN,
};

/// In-memory storage backend (mirrors the one used in the crate tests).
struct InMemoryStorage {
    slots: [[u8; SLOT_CAP]; 2],
    state: tpt_chassis_core::ota::SlotState,
}

const SLOT_CAP: usize = 256;

impl InMemoryStorage {
    fn new() -> Self {
        InMemoryStorage {
            slots: [[0u8; SLOT_CAP]; 2],
            state: tpt_chassis_core::ota::SlotState::ActiveSlot0,
        }
    }
}

impl tpt_chassis_core::ota::OtaStorage for InMemoryStorage {
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

    fn write_state(&mut self, state: tpt_chassis_core::ota::SlotState) -> Result<(), OtaError> {
        self.state = state;
        Ok(())
    }

    fn read_state(&self) -> Result<tpt_chassis_core::ota::SlotState, OtaError> {
        Ok(self.state)
    }
}

/// Builds and signs an update package over a payload.
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

fn main() {
    let signer = DemoSigner::new(0xCAFE);

    // --- Normal lifecycle: stage -> promote -> commit ---
    let mut store = InMemoryStorage::new();
    let mut engine = OtaEngine::new(&mut store, signer);
    let payload = [0xAB; 16];
    let (sig, pkg) = make_package(&signer, 1, &payload);
    engine.stage(&pkg, &payload, &sig).expect("stage ok");
    println!("staged candidate in slot {}", pkg.slot);

    let active = engine.promote().expect("promote ok");
    println!("promoted candidate to active slot {active}");
    let active = engine.commit().expect("commit ok");
    assert_eq!(active, 1);
    println!("committed image in slot {active}");

    // --- Corrupted signature is rejected ---
    let mut engine2 = OtaEngine::new(&mut store, signer);
    let mut bad_sig = [0u8; OTA_SIGNATURE_LEN];
    bad_sig[0] = 0xFF;
    assert_eq!(
        engine2.stage(&pkg, &payload, &bad_sig),
        Err(OtaError::BadSignature)
    );
    println!("rejected update with corrupted signature (BadSignature)");

    // --- Power loss during `Pending`: a fresh engine recovers and promotes ---
    let mut store3 = InMemoryStorage::new();
    let mut engine3 = OtaEngine::new(&mut store3, signer);
    let payload3 = [0x12; 16];
    let (sig3, pkg3) = make_package(&signer, 1, &payload3);
    engine3.stage(&pkg3, &payload3, &sig3).expect("stage ok");
    // `engine3` goes out of scope here, simulating a reboot mid-pending; the
    // staged state persists in `store3`.

    let mut recovered = OtaEngine::new(&mut store3, signer);
    let active = recovered.recover().expect("recover ok");
    println!("power-loss recovery promoted pending candidate to slot {active}");
    assert_eq!(active, 1);

    println!("ota_update: lifecycle + failure injection OK");
}
