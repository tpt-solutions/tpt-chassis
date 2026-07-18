// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) TPT Solutions. All rights reserved.
//
// Host-side validation for the AGL bindings: runs the `no_std` core engine
// against the AGL filesystem OTA backend and the SOCKETCAN (loopback) CAN
// backend.

use tpt_chassis_agl::can_socket::AglCanTransceiver;
use tpt_chassis_agl::ota_storage::AglOtaStorage;
use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_core::ota::{DemoSigner, OtaEngine, SignatureScheme, SlotState, UpdatePackage};

fn temp_root() -> std::path::PathBuf {
    // Each call gets a unique subdirectory so tests never share A/B slots.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("tpt-agl-{}-{}", std::process::id(), n));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

#[test]
fn agl_can_socket_loopback() {
    let mut bus = CanBus::new(AglCanTransceiver::new("vcan0"));
    let id = CanId::standard(0x7DF).unwrap();
    let frame = CanFrame::new(id, &[0x02, 0x10, 0x01]).unwrap();
    bus.transmit(frame).unwrap();
    assert!(bus.can_receive());
    let got = bus.receive().unwrap();
    assert_eq!(got.data(), &[0x02, 0x10, 0x01]);
}

#[test]
fn agl_ota_stage_promote_commit_file_backed() {
    let signer = DemoSigner::new(0x1234);
    let mut storage = AglOtaStorage::with_root(temp_root(), 4096);
    let mut engine = OtaEngine::new(&mut storage, signer);

    let payload = [0xCD; 48];
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
    engine.promote().unwrap();
    assert_eq!(engine.commit().unwrap(), 1);
    assert_eq!(engine.state().unwrap(), SlotState::ActiveSlot1);
}

#[test]
fn agl_ota_recover_promotes_pending() {
    let signer = DemoSigner::new(0x1234);
    let mut storage = AglOtaStorage::with_root(temp_root(), 4096);
    let mut engine = OtaEngine::new(&mut storage, signer);

    let payload = [0x77; 24];
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
    // Fresh engine over the same files simulates a reboot after staging.
    let mut engine2 = OtaEngine::new(&mut storage, signer);
    assert_eq!(engine2.recover().unwrap(), 1);
    assert!(matches!(
        engine2.state().unwrap(),
        SlotState::Testing { active: 1, .. }
    ));
}
