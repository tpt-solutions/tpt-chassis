// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) TPT Solutions. All rights reserved.
//
// Host-side validation gate for the Zephyr bindings. Runs the same `no_std`
// core engine against the Zephyr `OtaStorage` backend (RAM fallback) so the
// integration is exercised in CI on the host as well as via the thumbv7em
// no_std build.

#![cfg(feature = "std")]

use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId, CanTransceiver};
use tpt_chassis_core::ota::{
    DemoSigner, OtaEngine, OtaStorage, SignatureScheme, SlotState, UpdatePackage,
};
use tpt_chassis_zephyr::ota_storage::ZephyrOtaStorage;

/// A trivial loopback CAN transceiver used to validate the `CanBus` surface on
/// the Zephyr target config without a real FlexCAN peripheral.
struct LoopbackTransceiver {
    tx: CanFrame,
    has: bool,
}

impl CanTransceiver for LoopbackTransceiver {
    fn send(&mut self, frame: CanFrame) -> Result<(), tpt_chassis_core::bus::BusError> {
        self.tx = frame;
        self.has = true;
        Ok(())
    }
    fn recv(&mut self) -> Result<CanFrame, tpt_chassis_core::bus::BusError> {
        if self.has {
            self.has = false;
            Ok(self.tx)
        } else {
            Err(tpt_chassis_core::bus::BusError::RxQueueEmpty)
        }
    }
    fn has_received(&self) -> bool {
        self.has
    }
    fn can_send(&self) -> bool {
        true
    }
}

#[test]
fn zephyr_can_bus_loopback() {
    let mut bus = CanBus::new(LoopbackTransceiver {
        tx: CanFrame::new(CanId::standard(0).unwrap(), &[]).unwrap(),
        has: false,
    });
    let id = CanId::standard(0x321).unwrap();
    let frame = CanFrame::new(id, &[0xAA, 0xBB]).unwrap();
    bus.transmit(frame).unwrap();
    assert!(bus.can_receive());
    let got = bus.receive().unwrap();
    assert_eq!(got.data(), &[0xAA, 0xBB]);
    assert_eq!(got.id().raw(), 0x321);
}

#[test]
fn zephyr_ota_stage_promote_commit() {
    let signer = DemoSigner::new(0xBEEF);
    let mut storage = ZephyrOtaStorage::new();
    let mut engine = OtaEngine::new(&mut storage, signer);

    let payload = [0x5A; 32];
    let pkg = UpdatePackage {
        slot: 1,
        payload_len: payload.len() as u32,
    };
    let mut signed = [0u8; UpdatePackage::HEADER_LEN + tpt_chassis_core::ota::OTA_MAX_PAYLOAD];
    pkg.encode_header(&mut signed[..UpdatePackage::HEADER_LEN]);
    signed[UpdatePackage::HEADER_LEN..UpdatePackage::HEADER_LEN + payload.len()]
        .copy_from_slice(&payload);
    let mut sig = [0u8; tpt_chassis_core::ota::OTA_SIGNATURE_LEN];
    signer.sign(
        &signed[..UpdatePackage::HEADER_LEN + payload.len()],
        &mut sig,
    );

    engine.stage(&pkg, &payload, &sig).unwrap();
    assert!(matches!(
        engine.state().unwrap(),
        SlotState::Pending { candidate: 1 }
    ));
    let active = engine.promote().unwrap();
    assert_eq!(active, 1);
    let active = engine.commit().unwrap();
    assert_eq!(active, 1);
    assert_eq!(engine.state().unwrap(), SlotState::ActiveSlot1);
}

#[test]
fn zephyr_ota_power_loss_rolls_back() {
    let signer = DemoSigner::new(0xBEEF);
    let mut storage = ZephyrOtaStorage::new();
    let mut engine = OtaEngine::new(&mut storage, signer);

    let payload = [0x12; 16];
    let pkg = UpdatePackage {
        slot: 1,
        payload_len: payload.len() as u32,
    };
    let mut signed = [0u8; UpdatePackage::HEADER_LEN + tpt_chassis_core::ota::OTA_MAX_PAYLOAD];
    pkg.encode_header(&mut signed[..UpdatePackage::HEADER_LEN]);
    signed[UpdatePackage::HEADER_LEN..UpdatePackage::HEADER_LEN + payload.len()]
        .copy_from_slice(&payload);
    let mut sig = [0u8; tpt_chassis_core::ota::OTA_SIGNATURE_LEN];
    signer.sign(
        &signed[..UpdatePackage::HEADER_LEN + payload.len()],
        &mut sig,
    );

    engine.stage(&pkg, &payload, &sig).unwrap();
    engine.promote().unwrap();
    // Simulate reboot: fresh engine over the same storage.
    let mut engine2 = OtaEngine::new(&mut storage, signer);
    let active = engine2.recover().unwrap();
    assert_eq!(active, 0);
    assert_eq!(engine2.state().unwrap(), SlotState::ActiveSlot0);
}
