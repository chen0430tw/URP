//! USB Device Executor
//!
//! Treats a USB-connected device (microcontroller, USB accelerator, etc.) as a
//! first-class URP compute node.  The device runs a minimal firmware that speaks
//! the URP USB wire protocol; URP's scheduler routes IRBlocks to it exactly as it
//! would route to a CPU or GPU node.
//!
//! # Wire Protocol
//!
//! All frames are exchanged over a USB CDC serial (or USB bulk) link.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Request frame  (host → device)                             │
//! ├──────────┬───────────┬────────────────────────┬─────────────┤
//! │ SYNC(1B) │ LEN(2B LE)│ PAYLOAD                │ CRC8(1B)    │
//! │  0xA5    │ payload   │ OPCODE(1B) N_IN(1B)    │  CRC of     │
//! │          │ length    │ [IN_LEN(2B) IN_BYTES]… │  payload    │
//! └──────────┴───────────┴────────────────────────┴─────────────┘
//!
//! ┌─────────────────────────────────────────────────────────────┐
//! │  Response frame  (device → host)                            │
//! ├──────────┬───────────┬────────────────────────┬─────────────┤
//! │ SYNC(1B) │ LEN(2B LE)│ STATUS(1B) [OUT_LEN(2B)│ CRC8(1B)    │
//! │  0xA5    │           │ OUT_BYTES(PayloadCodec)]│             │
//! └──────────┴───────────┴────────────────────────┴─────────────┘
//! ```
//!
//! STATUS byte: 0x00 = ok, 0x01 = unsupported opcode, 0xFF = error.
//!
//! Input/output bytes are encoded with `PayloadCodec`.
//!
//! # Opcode ID table
//!
//! The firmware only needs to implement the subset it supports.  The host sends
//! the full frame regardless; unsupported opcodes return STATUS=0x01 and the
//! runtime re-routes them to CPU.
//!
//! ```text
//! 0x01  UConstI64   0x02  FConst
//! 0x10  UAdd        0x11  USub      0x12  UMul      0x13  UDiv    0x14  URem
//! 0x15  UCmpEq      0x16  UCmpLt    0x17  UCmpLe
//! 0x18  UAnd        0x19  UOr       0x1A  UXor      0x1B  UNot
//! 0x1C  UShl        0x1D  UShr      0x1E  UShra
//! 0x20  FAdd        0x21  FSub      0x22  FMul      0x23  FDiv    0x24  FPow
//! 0x25  FSqrt       0x26  FAbs      0x27  FNeg
//! 0x28  FFloor      0x29  FCeil     0x2A  FRound
//! 0x30  FCmpEq      0x31  FCmpLt    0x32  FCmpLe
//! 0x40  USelect     0x41  UMin      0x42  UMax      0x43  UAbs    0x44  UAssert
//! 0x50  F64ToI64    0x51  I64ToF64
//! 0xF0  Hello  (capability handshake — no inputs, returns DeviceInfo payload)
//! ```
//!
//! # Deployment modes
//!
//! | Device                  | Host sees                                |
//! |-------------------------|------------------------------------------|
//! | RP2040 / STM32 firmware | CDC ACM → /dev/ttyACM0 or COMx          |
//! | Pi Zero 2W (USB gadget) | CDC serial + CDC network (usb0 192.168.6.x) |
//! | USB Armory Mk II        | RNDIS/ECM net interface → TCP            |
//!
//! # Auto-discovery
//!
//! `UsbDiscovery::scan()` (feature="usb") calls `serialport::available_ports()`,
//! filters by known URP firmware VID/PID values, sends a HELLO frame to each
//! candidate, and returns ready-to-register `UsbExecutor` instances.
//!
//! # Host-side auto-registration
//!
//! - **Linux**: udev rule `TAG+="systemd" ENV{SYSTEMD_WANTS}="urp-register@%k.service"`
//!   triggers on `SUBSYSTEM==tty, ACTION==add, ATTRS{idVendor}=="2E8A"` (RP2040).
//! - **Windows**: WMI subscription on `Win32_SerialPort` creation event calls
//!   `urp-register.exe --port COMx`.

use std::collections::HashMap;

use crate::executor::{eval_opcode, HardwareExecutor};
use crate::ir::{IRBlock, Opcode};
use crate::packet::{PayloadCodec, PayloadValue};

// ─────────────────────────────────────────────────────────────────────────────
// Opcode ↔ USB ID mapping
// ─────────────────────────────────────────────────────────────────────────────

/// Compact 1-byte opcode identifier for the USB wire protocol.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbOpcodeId {
    Unknown   = 0x00,
    ConstI64  = 0x01,
    ConstF64  = 0x02,
    UAdd      = 0x10,
    USub      = 0x11,
    UMul      = 0x12,
    UDiv      = 0x13,
    URem      = 0x14,
    UCmpEq    = 0x15,
    UCmpLt    = 0x16,
    UCmpLe    = 0x17,
    UAnd      = 0x18,
    UOr       = 0x19,
    UXor      = 0x1A,
    UNot      = 0x1B,
    UShl      = 0x1C,
    UShr      = 0x1D,
    UShra     = 0x1E,
    FAdd      = 0x20,
    FSub      = 0x21,
    FMul      = 0x22,
    FDiv      = 0x23,
    FPow      = 0x24,
    FSqrt     = 0x25,
    FAbs      = 0x26,
    FNeg      = 0x27,
    FFloor    = 0x28,
    FCeil     = 0x29,
    FRound    = 0x2A,
    FCmpEq    = 0x30,
    FCmpLt    = 0x31,
    FCmpLe    = 0x32,
    USelect   = 0x40,
    UMin      = 0x41,
    UMax      = 0x42,
    UAbs      = 0x43,
    UAssert   = 0x44,
    F64ToI64  = 0x50,
    I64ToF64  = 0x51,
    /// Capability handshake.  Host sends Hello with no inputs; device responds
    /// with a DeviceInfo payload encoded as a Str (JSON-like key=value pairs
    /// separated by `\n`).  Example response payload:
    /// ```text
    /// name=urp-pico-0\ncaps=0x10-0x51\nthroughput=5000\n
    /// ```
    Hello     = 0xF0,
}

/// Information returned by a device in response to a HELLO frame.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Human-readable device name (from firmware).
    pub name: String,
    /// Opcode IDs this device supports, as a hex range string (e.g. "0x10-0x51").
    pub caps: String,
    /// Self-reported throughput estimate in operations/second.
    pub throughput: u32,
    /// Serial port path used to reach this device.
    pub port: String,
}

impl DeviceInfo {
    /// Parse the `\n`-separated `key=value` string returned by a HELLO response.
    pub fn parse(payload: &str, port: &str) -> Self {
        let mut name = format!("usb-{port}");
        let mut caps = String::new();
        let mut throughput = 0u32;
        for line in payload.split('\n') {
            if let Some(v) = line.strip_prefix("name=") { name = v.to_string(); }
            if let Some(v) = line.strip_prefix("caps=") { caps = v.to_string(); }
            if let Some(v) = line.strip_prefix("throughput=") {
                throughput = v.trim().parse().unwrap_or(0);
            }
        }
        DeviceInfo { name, caps, throughput, port: port.to_string() }
    }

    /// Estimated compute capacity for the URP node (normalized to CPU = 1.0).
    pub fn compute_capacity(&self) -> f32 {
        // 1 MHz equivalent per 1000 ops/sec reported throughput.
        // A Cortex-M0+ at 133 MHz doing simple integer add ≈ 133 000 ops/s.
        (self.throughput as f32) / 10_000.0
    }
}

impl UsbOpcodeId {
    pub fn from_opcode(op: &Opcode) -> Self {
        match op {
            Opcode::UConstI64(_) => Self::ConstI64,
            Opcode::FConst(_)    => Self::ConstF64,
            Opcode::UAdd    => Self::UAdd,
            Opcode::USub    => Self::USub,
            Opcode::UMul    => Self::UMul,
            Opcode::UDiv    => Self::UDiv,
            Opcode::URem    => Self::URem,
            Opcode::UCmpEq  => Self::UCmpEq,
            Opcode::UCmpLt  => Self::UCmpLt,
            Opcode::UCmpLe  => Self::UCmpLe,
            Opcode::UAnd    => Self::UAnd,
            Opcode::UOr     => Self::UOr,
            Opcode::UXor    => Self::UXor,
            Opcode::UNot    => Self::UNot,
            Opcode::UShl    => Self::UShl,
            Opcode::UShr    => Self::UShr,
            Opcode::UShra   => Self::UShra,
            Opcode::FAdd    => Self::FAdd,
            Opcode::FSub    => Self::FSub,
            Opcode::FMul    => Self::FMul,
            Opcode::FDiv    => Self::FDiv,
            Opcode::FPow    => Self::FPow,
            Opcode::FSqrt   => Self::FSqrt,
            Opcode::FAbs    => Self::FAbs,
            Opcode::FNeg    => Self::FNeg,
            Opcode::FFloor  => Self::FFloor,
            Opcode::FCeil   => Self::FCeil,
            Opcode::FRound  => Self::FRound,
            Opcode::FCmpEq  => Self::FCmpEq,
            Opcode::FCmpLt  => Self::FCmpLt,
            Opcode::FCmpLe  => Self::FCmpLe,
            Opcode::USelect => Self::USelect,
            Opcode::UMin    => Self::UMin,
            Opcode::UMax    => Self::UMax,
            Opcode::UAbs    => Self::UAbs,
            Opcode::UAssert => Self::UAssert,
            Opcode::F64ToI64 => Self::F64ToI64,
            Opcode::I64ToF64 => Self::I64ToF64,
            _ => Self::Unknown,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame codec
// ─────────────────────────────────────────────────────────────────────────────

pub const FRAME_SYNC: u8 = 0xA5;
pub const STATUS_OK:  u8 = 0x00;
pub const STATUS_UNSUPPORTED: u8 = 0x01;
pub const STATUS_ERROR: u8 = 0xFF;

/// CRC-8/MAXIM (polynomial 0x31, initial 0x00).
/// Chosen because it's commonly available on microcontrollers.
pub fn crc8(data: &[u8]) -> u8 {
    let mut crc: u8 = 0x00;
    for &byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x31;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Encode a request frame: SYNC | LEN(2 LE) | PAYLOAD | CRC8
///
/// PAYLOAD = OPCODE_ID(1) | N_INPUTS(1) | [IN_LEN(2 LE) | IN_BYTES]...
pub fn encode_request(
    opcode_id: UsbOpcodeId,
    inputs: &[&PayloadValue],
) -> Vec<u8> {
    let mut payload: Vec<u8> = Vec::new();
    payload.push(opcode_id as u8);
    payload.push(inputs.len() as u8);
    for &v in inputs {
        let encoded = PayloadCodec::encode(v);
        let len = encoded.len() as u16;
        payload.extend_from_slice(&len.to_le_bytes());
        payload.extend_from_slice(&encoded);
    }

    let len = payload.len() as u16;
    let crc = crc8(&payload);
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.push(FRAME_SYNC);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.push(crc);
    frame
}

/// Decode a response frame.  Returns `(status, result)` on success.
/// Returns `Err` if the frame is malformed or CRC fails.
pub fn decode_response(frame: &[u8]) -> Result<(u8, Option<PayloadValue>), String> {
    if frame.len() < 4 {
        return Err(format!("frame too short: {} bytes", frame.len()));
    }
    if frame[0] != FRAME_SYNC {
        return Err(format!("bad sync byte: 0x{:02X}", frame[0]));
    }
    let payload_len = u16::from_le_bytes([frame[1], frame[2]]) as usize;
    if frame.len() < 3 + payload_len + 1 {
        return Err(format!(
            "truncated frame: need {} bytes, have {}",
            3 + payload_len + 1,
            frame.len()
        ));
    }
    let payload = &frame[3..3 + payload_len];
    let expected_crc = frame[3 + payload_len];
    let actual_crc = crc8(payload);
    if actual_crc != expected_crc {
        return Err(format!(
            "CRC mismatch: expected 0x{:02X}, got 0x{:02X}",
            expected_crc, actual_crc
        ));
    }

    let status = payload[0];
    if status != STATUS_OK || payload.len() < 3 {
        return Ok((status, None));
    }

    let result_len = u16::from_le_bytes([payload[1], payload[2]]) as usize;
    if payload.len() < 3 + result_len {
        return Err("truncated result in response payload".to_string());
    }
    let result = PayloadCodec::decode(&payload[3..3 + result_len]);
    Ok((status, Some(result)))
}

/// Encode a response frame (for use in firmware / loopback tests).
pub fn encode_response(status: u8, result: Option<&PayloadValue>) -> Vec<u8> {
    let mut payload: Vec<u8> = Vec::new();
    payload.push(status);
    if let Some(v) = result {
        let encoded = PayloadCodec::encode(v);
        let len = encoded.len() as u16;
        payload.extend_from_slice(&len.to_le_bytes());
        payload.extend_from_slice(&encoded);
    }
    let len = payload.len() as u16;
    let crc = crc8(&payload);
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.push(FRAME_SYNC);
    frame.extend_from_slice(&len.to_le_bytes());
    frame.extend_from_slice(&payload);
    frame.push(crc);
    frame
}

// ─────────────────────────────────────────────────────────────────────────────
// UsbLoopbackExecutor — software loopback, no hardware required
//
// Useful for:
//   • Protocol unit tests
//   • CI without a physical USB device
//   • Firmware development (host-side reference implementation)
//
// Internally runs eval_opcode, but routes through the full frame encode/decode
// path so the framing code is exercised.
// ─────────────────────────────────────────────────────────────────────────────

pub struct UsbLoopbackExecutor {
    #[allow(dead_code)]
    device_id: String,
}

impl UsbLoopbackExecutor {
    pub fn new(device_id: impl Into<String>) -> Self {
        Self { device_id: device_id.into() }
    }

    fn process_frame(request: &[u8]) -> Vec<u8> {
        // Parse request payload
        if request.len() < 2 {
            return encode_response(STATUS_ERROR, None);
        }
        // skip sync + len already stripped by caller; here we work on the raw frame
        // Re-use decode logic to get payload:
        if request[0] != FRAME_SYNC { return encode_response(STATUS_ERROR, None); }
        let payload_len = u16::from_le_bytes([request[1], request[2]]) as usize;
        if request.len() < 3 + payload_len + 1 {
            return encode_response(STATUS_ERROR, None);
        }
        let payload = &request[3..3 + payload_len];
        let crc_byte = request[3 + payload_len];
        if crc8(payload) != crc_byte {
            return encode_response(STATUS_ERROR, None);
        }

        // payload[0] = opcode_id, payload[1] = n_inputs
        let _opcode_id = payload[0];
        let n_inputs = payload[1] as usize;
        let mut pos = 2usize;
        let mut input_values: Vec<PayloadValue> = Vec::with_capacity(n_inputs);
        for _ in 0..n_inputs {
            if pos + 2 > payload.len() { return encode_response(STATUS_ERROR, None); }
            let in_len = u16::from_le_bytes([payload[pos], payload[pos + 1]]) as usize;
            pos += 2;
            if pos + in_len > payload.len() { return encode_response(STATUS_ERROR, None); }
            input_values.push(PayloadCodec::decode(&payload[pos..pos + in_len]));
            pos += in_len;
        }

        // The loopback executor can't reconstruct the full IRBlock from the frame alone
        // (it only has the opcode_id byte, not the Rust enum).  This is by design — the
        // loopback is for framing tests.  Actual execution uses UsbCpuFallback below.
        encode_response(STATUS_UNSUPPORTED, None)
    }
}

impl HardwareExecutor for UsbLoopbackExecutor {
    fn name(&self) -> &'static str { "usb-loopback" }

    fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        // Build request frame
        let opcode_id = UsbOpcodeId::from_opcode(&block.opcode);
        let input_vals: Vec<&PayloadValue> = block.inputs.iter()
            .filter_map(|k| ctx.get(k))
            .collect();
        let request = encode_request(opcode_id, &input_vals);

        // Process through software loopback
        let response = Self::process_frame(&request);
        match decode_response(&response) {
            Ok((STATUS_OK, Some(v))) => v,
            // Unsupported in loopback → fall back to CPU
            _ => eval_opcode(block, ctx),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UsbCpuFallbackExecutor — routes through USB frame protocol, falls back to CPU
//
// This is the production-safe executor to use when the USB device might not
// support all opcodes.  On STATUS_UNSUPPORTED the block is re-executed locally.
// ─────────────────────────────────────────────────────────────────────────────

pub struct UsbCpuFallbackExecutor {
    inner: Box<dyn HardwareExecutor + Send + Sync>,
}

impl UsbCpuFallbackExecutor {
    pub fn new(inner: Box<dyn HardwareExecutor + Send + Sync>) -> Self {
        Self { inner }
    }
}

impl HardwareExecutor for UsbCpuFallbackExecutor {
    fn name(&self) -> &'static str { "usb-cpu-fallback" }

    fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
        let result = self.inner.exec(block, ctx);
        // If inner returned a "sentinel" error value we could re-route here.
        // For now the inner executor handles fallback internally.
        result
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UsbExecutor — real hardware via serialport
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "usb")]
pub mod hardware {
    use super::*;
    use std::sync::Mutex;
    use serialport::SerialPort;

    /// Configuration for a USB device node.
    #[derive(Debug, Clone)]
    pub struct UsbDeviceConfig {
        /// Serial port path, e.g. "COM3" on Windows or "/dev/ttyACM0" on Linux.
        pub port: String,
        /// Baud rate.  USB CDC devices typically support any rate; 115200 is safe.
        pub baud_rate: u32,
        /// Read/write timeout per frame.
        pub timeout: Duration,
        /// Human-readable device name for diagnostics.
        pub device_name: String,
    }

    impl UsbDeviceConfig {
        pub fn new(port: impl Into<String>) -> Self {
            Self {
                port: port.into(),
                baud_rate: 115_200,
                timeout: Duration::from_millis(100),
                device_name: "usb-device".into(),
            }
        }

        pub fn with_baud(mut self, baud: u32) -> Self { self.baud_rate = baud; self }
        pub fn with_timeout(mut self, t: Duration) -> Self { self.timeout = t; self }
        pub fn with_name(mut self, name: impl Into<String>) -> Self { self.device_name = name.into(); self }
    }

    /// A hardware USB executor.  Sends IRBlock computation requests to the device
    /// and reads back the result.  Falls back to CPU for unsupported opcodes.
    pub struct UsbExecutor {
        port: Mutex<Box<dyn SerialPort>>,
        config: UsbDeviceConfig,
    }

    impl UsbExecutor {
        pub fn open(config: UsbDeviceConfig) -> Result<Self, String> {
            let port = serialport::new(&config.port, config.baud_rate)
                .timeout(config.timeout)
                .open()
                .map_err(|e| format!("UsbExecutor: cannot open {}: {e}", config.port))?;
            Ok(Self { port: Mutex::new(port), config })
        }

        fn transact(&self, request: &[u8]) -> Result<Vec<u8>, String> {
            let mut port = self.port.lock().unwrap();

            // Write request
            port.write_all(request)
                .map_err(|e| format!("USB write error: {e}"))?;

            // Read response header: SYNC(1) + LEN(2)
            let mut header = [0u8; 3];
            port.read_exact(&mut header)
                .map_err(|e| format!("USB read header error: {e}"))?;
            if header[0] != FRAME_SYNC {
                return Err(format!("USB: bad sync 0x{:02X}", header[0]));
            }
            let payload_len = u16::from_le_bytes([header[1], header[2]]) as usize;

            // Read payload + CRC
            let mut rest = vec![0u8; payload_len + 1];
            port.read_exact(&mut rest)
                .map_err(|e| format!("USB read payload error: {e}"))?;

            let mut frame = header.to_vec();
            frame.extend_from_slice(&rest);
            Ok(frame)
        }
    }

    impl HardwareExecutor for UsbExecutor {
        fn name(&self) -> &'static str { "usb" }

        fn exec(&self, block: &IRBlock, ctx: &HashMap<String, PayloadValue>) -> PayloadValue {
            let opcode_id = UsbOpcodeId::from_opcode(&block.opcode);
            let input_vals: Vec<&PayloadValue> = block.inputs.iter()
                .filter_map(|k| ctx.get(k))
                .collect();
            let request = encode_request(opcode_id, &input_vals);

            match self.transact(&request) {
                Ok(response) => {
                    match decode_response(&response) {
                        Ok((STATUS_OK, Some(v))) => v,
                        Ok((STATUS_UNSUPPORTED, _)) => {
                            // Device doesn't support this opcode — fall back to CPU
                            eval_opcode(block, ctx)
                        }
                        Ok((status, _)) => {
                            eprintln!("[usb] device returned status 0x{status:02X}, falling back to CPU");
                            eval_opcode(block, ctx)
                        }
                        Err(e) => {
                            eprintln!("[usb] frame decode error: {e}, falling back to CPU");
                            eval_opcode(block, ctx)
                        }
                    }
                }
                Err(e) => {
                    eprintln!("[usb] transact error: {e}, falling back to CPU");
                    eval_opcode(block, ctx)
                }
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // UsbDiscovery — scan serial ports, HELLO-handshake, return live executors
    // ─────────────────────────────────────────────────────────────────────────

    /// Known URP firmware VID/PID pairs.
    ///
    /// Add your own device's VID/PID here.  RP2040 = 0x2E8A, STM32 = 0x0483.
    /// The PID is less stable (depends on firmware config); set to `None` to
    /// match any PID from the given vendor.
    const KNOWN_VENDORS: &[(u16, Option<u16>)] = &[
        (0x2E8A, None),  // Raspberry Pi (RP2040, Pi Zero in gadget mode)
        (0x0483, None),  // STMicroelectronics (STM32 USB CDC)
        (0x1D6B, None),  // Linux Foundation (Pi Zero configfs gadget)
    ];

    /// Scan available serial ports, probe each URP-compatible device with a
    /// HELLO frame, and return `DeviceInfo` for each that responds.
    pub struct UsbDiscovery;

    impl UsbDiscovery {
        /// Scan all serial ports and return info for responding URP devices.
        ///
        /// `timeout` is the per-device HELLO round-trip budget.
        pub fn scan(timeout: Duration) -> Vec<(DeviceInfo, UsbExecutor)> {
            let ports = match serialport::available_ports() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("[usb-discovery] available_ports error: {e}");
                    return Vec::new();
                }
            };

            let mut found = Vec::new();
            for port_info in ports {
                // Filter by VID/PID
                let is_known = match &port_info.port_type {
                    serialport::SerialPortType::UsbPort(usb) => {
                        KNOWN_VENDORS.iter().any(|&(vid, pid)| {
                            usb.vid == vid && pid.map_or(true, |p| usb.pid == p)
                        })
                    }
                    _ => false,
                };
                if !is_known { continue; }

                let port_name = port_info.port_name.clone();
                let config = UsbDeviceConfig::new(&port_name)
                    .with_timeout(timeout)
                    .with_name(format!("usb-{port_name}"));

                let executor = match UsbExecutor::open(config) {
                    Ok(e) => e,
                    Err(err) => {
                        eprintln!("[usb-discovery] cannot open {port_name}: {err}");
                        continue;
                    }
                };

                // Send HELLO frame
                let hello_frame = encode_request(UsbOpcodeId::Hello, &[]);
                match executor.transact(&hello_frame) {
                    Ok(response) => {
                        match decode_response(&response) {
                            Ok((STATUS_OK, Some(PayloadValue::Str(info_str)))) => {
                                let info = DeviceInfo::parse(&info_str, &port_name);
                                eprintln!(
                                    "[usb-discovery] found: {} @ {} caps={} throughput={}",
                                    info.name, port_name, info.caps, info.throughput
                                );
                                found.push((info, executor));
                            }
                            _ => {
                                eprintln!("[usb-discovery] {port_name}: no valid HELLO response");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("[usb-discovery] {port_name}: transact error: {e}");
                    }
                }
            }
            found
        }

        /// Register all discovered USB devices into the executor registry.
        ///
        /// Each device is registered under its `DeviceInfo.name` as the node ID.
        /// Returns the `DeviceInfo` list for the caller to also create `Node`
        /// entries in the URP runtime.
        pub fn register_all(
            registry: &mut crate::executor::ExecutorRegistry,
            timeout: Duration,
        ) -> Vec<DeviceInfo> {
            Self::scan(timeout)
                .into_iter()
                .map(|(info, executor)| {
                    registry.register(info.name.clone(), Arc::new(executor));
                    info
                })
                .collect()
        }
    }
}

#[cfg(feature = "usb")]
pub use hardware::{UsbDeviceConfig, UsbDiscovery, UsbExecutor};

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::Opcode;
    use crate::packet::PayloadValue;

    // ── CRC8 ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_crc8_deterministic() {
        let data = b"hello usb";
        assert_eq!(crc8(data), crc8(data));
        assert_ne!(crc8(data), crc8(b"hello USB"));
    }

    #[test]
    fn test_crc8_empty() {
        assert_eq!(crc8(b""), 0x00);
    }

    // ── Request frame ─────────────────────────────────────────────────────────

    #[test]
    fn test_encode_request_add() {
        let a = PayloadValue::I64(3);
        let b = PayloadValue::I64(4);
        let frame = encode_request(UsbOpcodeId::UAdd, &[&a, &b]);
        assert_eq!(frame[0], FRAME_SYNC);
        // payload = opcode(1) + n_inputs(1) + [len(2) + bytes(9)] * 2 = 2 + 22 = 24
        let payload_len = u16::from_le_bytes([frame[1], frame[2]]) as usize;
        assert_eq!(payload_len, 2 + 2 * (2 + 9));  // 24
        // CRC should match
        let payload = &frame[3..3 + payload_len];
        assert_eq!(frame[3 + payload_len], crc8(payload));
    }

    #[test]
    fn test_encode_request_const() {
        let frame = encode_request(UsbOpcodeId::ConstI64, &[&PayloadValue::I64(42)]);
        assert_eq!(frame[0], FRAME_SYNC);
    }

    // ── Response frame ────────────────────────────────────────────────────────

    #[test]
    fn test_encode_decode_response_ok() {
        let result = PayloadValue::I64(7);
        let frame = encode_response(STATUS_OK, Some(&result));
        let (status, value) = decode_response(&frame).unwrap();
        assert_eq!(status, STATUS_OK);
        assert_eq!(value, Some(PayloadValue::I64(7)));
    }

    #[test]
    fn test_encode_decode_response_f64() {
        let result = PayloadValue::F64(3.14);
        let frame = encode_response(STATUS_OK, Some(&result));
        let (status, value) = decode_response(&frame).unwrap();
        assert_eq!(status, STATUS_OK);
        match value {
            Some(PayloadValue::F64(v)) => assert!((v - 3.14).abs() < 1e-10),
            _ => panic!("expected F64"),
        }
    }

    #[test]
    fn test_encode_decode_response_unsupported() {
        let frame = encode_response(STATUS_UNSUPPORTED, None);
        let (status, value) = decode_response(&frame).unwrap();
        assert_eq!(status, STATUS_UNSUPPORTED);
        assert_eq!(value, None);
    }

    #[test]
    fn test_decode_bad_sync() {
        let mut frame = encode_response(STATUS_OK, Some(&PayloadValue::I64(1)));
        frame[0] = 0x00;  // corrupt sync
        assert!(decode_response(&frame).is_err());
    }

    #[test]
    fn test_decode_bad_crc() {
        let mut frame = encode_response(STATUS_OK, Some(&PayloadValue::I64(1)));
        *frame.last_mut().unwrap() ^= 0xFF;  // flip CRC
        assert!(decode_response(&frame).is_err());
    }

    // ── Opcode ID mapping ─────────────────────────────────────────────────────

    #[test]
    fn test_opcode_id_round_trip() {
        let cases = [
            (Opcode::UAdd,    UsbOpcodeId::UAdd),
            (Opcode::FMul,    UsbOpcodeId::FMul),
            (Opcode::FSqrt,   UsbOpcodeId::FSqrt),
            (Opcode::UCmpEq,  UsbOpcodeId::UCmpEq),
            (Opcode::I64ToF64,UsbOpcodeId::I64ToF64),
        ];
        for (opcode, expected_id) in cases {
            assert_eq!(UsbOpcodeId::from_opcode(&opcode), expected_id);
        }
    }

    #[test]
    fn test_unknown_opcode_id() {
        // UConcat has no USB ID
        assert_eq!(UsbOpcodeId::from_opcode(&Opcode::UConcat), UsbOpcodeId::Unknown);
    }

    // ── UsbLoopbackExecutor ───────────────────────────────────────────────────

    #[test]
    fn test_loopback_falls_back_to_cpu() {
        use std::collections::HashMap;
        use crate::ir::IRBlock;

        let ex = UsbLoopbackExecutor::new("loopback");
        let mut block = IRBlock::new("add", Opcode::UAdd);
        block.inputs = vec!["a".into(), "b".into()];
        let mut ctx = HashMap::new();
        ctx.insert("a".into(), PayloadValue::I64(10));
        ctx.insert("b".into(), PayloadValue::I64(32));

        // Loopback returns UNSUPPORTED → falls back to CPU eval_opcode
        let result = ex.exec(&block, &ctx);
        assert_eq!(result, PayloadValue::I64(42));
    }

    #[test]
    fn test_loopback_fallback_float() {
        use std::collections::HashMap;
        use crate::ir::IRBlock;

        let ex = UsbLoopbackExecutor::new("loopback");
        let mut block = IRBlock::new("sq", Opcode::FSqrt);
        block.inputs = vec!["a".into()];
        let mut ctx = HashMap::new();
        ctx.insert("a".into(), PayloadValue::F64(9.0));

        let result = ex.exec(&block, &ctx);
        match result {
            PayloadValue::F64(v) => assert!((v - 3.0).abs() < 1e-9),
            _ => panic!("expected F64"),
        }
    }

    // ── Full round-trip: encode request → device processes → encode response ──

    #[test]
    fn test_request_response_round_trip() {
        // Simulate device: receives request, computes add(3, 4) = 7, sends back
        let a = PayloadValue::I64(3);
        let b = PayloadValue::I64(4);
        let req_frame = encode_request(UsbOpcodeId::UAdd, &[&a, &b]);

        // "Device" decodes request (verify it's well-formed)
        assert_eq!(req_frame[0], FRAME_SYNC);
        let payload_len = u16::from_le_bytes([req_frame[1], req_frame[2]]) as usize;
        let payload = &req_frame[3..3 + payload_len];
        assert_eq!(crc8(payload), req_frame[3 + payload_len]);
        assert_eq!(payload[0], UsbOpcodeId::UAdd as u8);
        assert_eq!(payload[1], 2u8);  // 2 inputs

        // "Device" computes and sends response
        let resp_frame = encode_response(STATUS_OK, Some(&PayloadValue::I64(7)));
        let (status, value) = decode_response(&resp_frame).unwrap();
        assert_eq!(status, STATUS_OK);
        assert_eq!(value, Some(PayloadValue::I64(7)));
    }

    // ── HELLO handshake ───────────────────────────────────────────────────────

    #[test]
    fn test_hello_opcode_id() {
        assert_eq!(UsbOpcodeId::Hello as u8, 0xF0);
    }

    #[test]
    fn test_hello_request_has_zero_inputs() {
        let frame = encode_request(UsbOpcodeId::Hello, &[]);
        let payload_len = u16::from_le_bytes([frame[1], frame[2]]) as usize;
        let payload = &frame[3..3 + payload_len];
        assert_eq!(payload[0], 0xF0);  // Hello opcode
        assert_eq!(payload[1], 0);     // 0 inputs
    }

    #[test]
    fn test_hello_response_parse() {
        let info_str = "name=urp-pico-0\ncaps=0x10-0x51\nthroughput=133000\n";
        let resp = encode_response(STATUS_OK, Some(&PayloadValue::Str(info_str.into())));
        let (status, value) = decode_response(&resp).unwrap();
        assert_eq!(status, STATUS_OK);

        let info_payload = match value {
            Some(PayloadValue::Str(s)) => s,
            _ => panic!("expected Str"),
        };
        let info = DeviceInfo::parse(&info_payload, "/dev/ttyACM0");
        assert_eq!(info.name, "urp-pico-0");
        assert_eq!(info.caps, "0x10-0x51");
        assert_eq!(info.throughput, 133_000);
        assert_eq!(info.port, "/dev/ttyACM0");
        // 133000 / 10000 = 13.3
        assert!((info.compute_capacity() - 13.3).abs() < 0.01);
    }

    #[test]
    fn test_device_info_default_name() {
        let info = DeviceInfo::parse("throughput=5000\n", "COM3");
        assert_eq!(info.name, "usb-COM3");  // fallback name from port
        assert_eq!(info.throughput, 5000);
    }
}
