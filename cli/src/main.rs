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

//! `tpt-cli` — developer tooling for TPT Chassis.
//!
//! Subcommands:
//! - `new <name>`        — scaffold a new ECU crate from the starter template.
//! - `sim`               — bring up the in-memory CAN network and exchange frames.
//! - `ota sign` / `stage` — assemble and sign an update package (demo signer).
//! - `diag`              — a minimal UDS tester-present / session client over sim CAN.

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use tpt_chassis_core::bus::VehicleBus;
use tpt_chassis_core::can::{CanBus, CanFrame, CanId};
use tpt_chassis_core::ota::{DemoSigner, SignatureScheme, UpdatePackage, OTA_SIGNATURE_LEN};
use tpt_chassis_sim::can::SimCanNetwork;

#[derive(Parser)]
#[command(name = "tpt-cli", about = "Developer tooling for TPT Chassis")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Scaffold a new ECU crate from the starter template.
    New {
        /// Name (and directory) of the new crate.
        name: String,
    },
    /// Run the in-memory CAN simulator and exchange a couple of frames.
    Sim,
    /// OTA package assembly and signing.
    Ota {
        #[command(subcommand)]
        cmd: OtaCmd,
    },
    /// Minimal UDS diagnostics client over the simulator.
    Diag,
    /// Generate safe-Rust Signal accessors from a signals.toml database.
    Signals {
        /// Path to the signals.toml database.
        db: PathBuf,
        /// Optional output file (defaults to stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum OtaCmd {
    /// Sign a payload file, printing the package header + signature (hex).
    Sign {
        /// Path to the payload binary to sign.
        payload: PathBuf,
        /// 32-bit demo signing key (hex).
        #[arg(long, default_value_t = 0xCAFE)]
        key: u32,
        /// Target slot (0 or 1).
        #[arg(long, default_value_t = 1)]
        slot: u8,
    },
    /// Stage a signed package: write `<payload>.pkg` next to the payload.
    Stage {
        /// Path to the payload binary.
        payload: PathBuf,
        #[arg(long, default_value_t = 0xCAFE)]
        key: u32,
        #[arg(long, default_value_t = 1)]
        slot: u8,
    },
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::New { name } => cmd_new(&name),
        Commands::Sim => cmd_sim(),
        Commands::Ota { cmd } => match cmd {
            OtaCmd::Sign { payload, key, slot } => cmd_ota_sign(&payload, key, slot),
            OtaCmd::Stage { payload, key, slot } => cmd_ota_stage(&payload, key, slot),
        },
        Commands::Diag => cmd_diag(),
        Commands::Signals { db, out } => cmd_signals(db, out),
    }
}

fn cmd_new(name: &str) {
    let dir = PathBuf::from(name);
    std::fs::create_dir_all(dir.join("src")).expect("create crate dir");
    let cargo = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\nlicense = \"MIT OR Apache-2.0\"\n\n[dependencies]\ntpt-chassis-core = {{ path = \"../core\", version = \"0.1.0\" }}\ntpt-chassis-sim = {{ path = \"../sim\", version = \"0.1.0\" }}\n"
    );
    std::fs::write(dir.join("Cargo.toml"), cargo).expect("write Cargo.toml");
    std::fs::write(
        dir.join("src/main.rs"),
        "fn main() {\n    println!(\"TODO: build your ECU on TPT Chassis\");\n}\n",
    )
    .expect("write main.rs");
    println!("scaffolded new ECU crate in ./{name}");
}

fn cmd_sim() {
    let net = SimCanNetwork::new();
    let mut tx = CanBus::new(net.node());
    let mut rx = CanBus::new(net.node());
    let id = CanId::standard(0x100).expect("valid id");
    let frame = CanFrame::new(id, &[0xDE, 0xAD, 0xBE, 0xEF]).expect("valid frame");
    tx.transmit(frame).expect("transmit");
    let received = rx.receive().expect("receive");
    println!(
        "sim: sent id=0x{:X}, received id=0x{:X} data={:?}",
        id.raw(),
        received.id().raw(),
        received.data()
    );
}

fn cmd_ota_sign(payload: &PathBuf, key: u32, slot: u8) {
    let bytes = std::fs::read(payload).expect("read payload");
    let signer = DemoSigner::new(key);
    let pkg = UpdatePackage {
        slot,
        payload_len: bytes.len() as u32,
    };
    let mut header = [0u8; UpdatePackage::HEADER_LEN];
    pkg.encode_header(&mut header);
    let mut signed = Vec::from(&header[..]);
    signed.extend_from_slice(&bytes);
    let mut sig = [0u8; OTA_SIGNATURE_LEN];
    signer.sign(&signed, &mut sig);
    println!(
        "ota sign: slot={slot} payload_len={} signature={}",
        bytes.len(),
        hex(&sig)
    );
}

fn cmd_ota_stage(payload: &PathBuf, key: u32, slot: u8) {
    let bytes = std::fs::read(payload).expect("read payload");
    let signer = DemoSigner::new(key);
    let pkg = UpdatePackage {
        slot,
        payload_len: bytes.len() as u32,
    };
    let mut header = [0u8; UpdatePackage::HEADER_LEN];
    pkg.encode_header(&mut header);
    let mut signed = Vec::from(&header[..]);
    signed.extend_from_slice(&bytes);
    let mut sig = [0u8; OTA_SIGNATURE_LEN];
    signer.sign(&signed, &mut sig);

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend_from_slice(&bytes);
    out.extend_from_slice(&sig);
    let pkg_path = payload.with_extension("pkg");
    std::fs::write(&pkg_path, &out).expect("write package");
    println!(
        "ota stage: wrote {} ({} bytes)",
        pkg_path.display(),
        out.len()
    );
}

fn cmd_diag() {
    let net = SimCanNetwork::new();
    let mut tester = CanBus::new(net.node());
    let mut ecu = CanBus::new(net.node());

    // Bring up a minimal UDS server-backed ECU loop in-process.
    use tpt_chassis_core::uds::{UdsDataProvider, UdsServer};
    struct Prov {
        store: [u8; 32],
    }
    impl UdsDataProvider for Prov {
        fn did_read_len(&self, did: u16) -> Option<usize> {
            if did == 0x0001 {
                Some(2)
            } else {
                None
            }
        }
        fn did_read(&self, _did: u16, out: &mut [u8]) {
            out.copy_from_slice(&self.store[..out.len()]);
        }
        fn did_write(&mut self, _did: u16, data: &[u8]) -> bool {
            self.store[..data.len()].copy_from_slice(data);
            true
        }
    }
    let mut server = UdsServer::new(Prov { store: [0u8; 32] });

    let ecu_id = CanId::standard(0x7E0).unwrap();
    let resp_id = CanId::standard(0x7E8).unwrap();
    let reqs: &[&[u8]] = &[&[0x10, 0x03], &[0x22, 0x00, 0x01], &[0x3E, 0x00]];
    for req in reqs {
        tester
            .transmit(CanFrame::new(ecu_id, req).unwrap())
            .unwrap();
        let rx = ecu.receive().unwrap();
        let mut resp = [0u8; 8];
        let n = server.handle(rx.data(), &mut resp).unwrap();
        ecu.transmit(CanFrame::new(resp_id, &resp[..n]).unwrap())
            .unwrap();
        let got = tester.receive().unwrap();
        println!("diag: req={:?} -> resp={:?}", req, &got.data()[..n]);
    }
}

fn cmd_signals(db: PathBuf, out: Option<PathBuf>) {
    let text = std::fs::read_to_string(db).expect("read signals.toml");
    let table: toml::Value = toml::from_str(&text).expect("parse signals.toml");

    let signals = table
        .get("signal")
        .and_then(|v| v.as_array())
        .expect("expected a [[signal]] array");

    let mut code = String::new();
    code.push_str("// Generated by `tpt-cli signals` from a signals.toml database.\n");
    code.push_str("// Do not edit by hand.\n\n");
    code.push_str("use tpt_chassis_core::autosar::Signal;\n\n");

    for s in signals.iter() {
        let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("signal");
        let start = s.get("start_bit").and_then(|v| v.as_integer()).unwrap_or(0) as u8;
        let length = s
            .get("length_bits")
            .and_then(|v| v.as_integer())
            .unwrap_or(0) as u8;
        let can_id = s.get("can_id").and_then(|v| v.as_integer()).unwrap_or(0) as u32;
        // `Signal::new` validates the layout; a bad DB fails the build loudly.
        code.push_str(&format!(
            "/// `{name}`: CAN 0x{can_id:X}, start bit {start}, {length} bits.\n"
        ));
        code.push_str(&format!(
            "pub const {name}: Signal = match Signal::new({start}, {length}) {{\n    Some(s) => s,\n    None => panic!(\"invalid signal layout for {name}\"),\n}};\n\n"
        ));
    }

    match out {
        Some(path) => {
            std::fs::write(&path, &code).expect("write generated code");
            println!(
                "signals: generated {} accessors -> {}",
                signals.len(),
                path.display()
            );
        }
        None => {
            println!("{code}");
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02X}"));
    }
    s
}
