use bytes::{BufMut, Bytes, BytesMut};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

use crate::ir::MergeMode;

#[repr(C)]
#[derive(Debug, Clone, Copy, FromBytes, IntoBytes, KnownLayout, Immutable)]
pub struct PacketHeader {
    pub opcode_id: u16,
    pub merge_mode: u16,
    pub src_len: u16,
    pub dst_len: u16,
    pub payload_len: u32,
}

#[derive(Debug, Clone)]
pub struct URPPacket {
    raw: Bytes,
}

impl URPPacket {
    pub fn build(opcode_id: u16, merge_mode: MergeMode, src_block: &str, dst_block: &str, payload: &[u8]) -> Self {
        let src = src_block.as_bytes();
        let dst = dst_block.as_bytes();
        let header = PacketHeader {
            opcode_id,
            merge_mode: merge_mode as u16,
            src_len: src.len() as u16,
            dst_len: dst.len() as u16,
            payload_len: payload.len() as u32,
        };

        let mut buf = BytesMut::with_capacity(
            core::mem::size_of::<PacketHeader>() + src.len() + dst.len() + payload.len()
        );
        buf.put_slice(header.as_bytes());
        buf.put_slice(src);
        buf.put_slice(dst);
        buf.put_slice(payload);

        Self { raw: buf.freeze() }
    }

    pub fn header(&self) -> PacketHeader {
        let hsize = core::mem::size_of::<PacketHeader>();
        *PacketHeader::ref_from_bytes(&self.raw[..hsize]).expect("invalid packet header")
    }

    pub fn payload(&self) -> &[u8] {
        let h = self.header();
        let base = core::mem::size_of::<PacketHeader>() + h.src_len as usize + h.dst_len as usize;
        let end = base + h.payload_len as usize;
        &self.raw[base..end]
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.raw.to_vec()
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < core::mem::size_of::<PacketHeader>() {
            return Err("Packet too short".to_string());
        }
        Ok(Self { raw: Bytes::copy_from_slice(bytes) })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PayloadValue {
    I64(i64),
    F64(f64),
    Str(String),
    List(Vec<PayloadValue>),
}

pub struct PayloadCodec;

impl PayloadCodec {
    const TYPE_I64: u8 = 1;
    const TYPE_STR: u8 = 2;
    const TYPE_F64: u8 = 4;

    pub fn encode(value: &PayloadValue) -> Vec<u8> {
        match value {
            PayloadValue::I64(v) => {
                let mut out = Vec::with_capacity(9);
                out.push(Self::TYPE_I64);
                out.extend_from_slice(&v.to_le_bytes());
                out
            }
            PayloadValue::F64(v) => {
                let mut out = Vec::with_capacity(9);
                out.push(Self::TYPE_F64);
                out.extend_from_slice(&v.to_le_bytes());
                out
            }
            PayloadValue::Str(s) => {
                let b = s.as_bytes();
                let mut out = Vec::with_capacity(1 + 4 + b.len());
                out.push(Self::TYPE_STR);
                out.extend_from_slice(&(b.len() as u32).to_le_bytes());
                out.extend_from_slice(b);
                out
            }
            PayloadValue::List(items) => {
                // Encode as: TYPE_LIST | u32(count) | encode(item)...
                let mut out = vec![3u8]; // TYPE_LIST = 3
                out.extend_from_slice(&(items.len() as u32).to_le_bytes());
                for item in items {
                    let encoded = Self::encode(item);
                    out.extend_from_slice(&(encoded.len() as u32).to_le_bytes());
                    out.extend_from_slice(&encoded);
                }
                out
            }
        }
    }

    pub fn decode(bytes: &[u8]) -> PayloadValue {
        match bytes[0] {
            Self::TYPE_I64 => {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&bytes[1..9]);
                PayloadValue::I64(i64::from_le_bytes(arr))
            }
            Self::TYPE_F64 => {
                let mut arr = [0u8; 8];
                arr.copy_from_slice(&bytes[1..9]);
                PayloadValue::F64(f64::from_le_bytes(arr))
            }
            Self::TYPE_STR => {
                let mut len = [0u8; 4];
                len.copy_from_slice(&bytes[1..5]);
                let n = u32::from_le_bytes(len) as usize;
                PayloadValue::Str(String::from_utf8(bytes[5..5+n].to_vec()).expect("invalid utf8"))
            }
            3 => {
                // TYPE_LIST: u32(count) | (u32(len) | bytes)...
                let mut count_arr = [0u8; 4];
                count_arr.copy_from_slice(&bytes[1..5]);
                let count = u32::from_le_bytes(count_arr) as usize;
                let mut items = Vec::with_capacity(count);
                let mut pos = 5usize;
                for _ in 0..count {
                    let mut len_arr = [0u8; 4];
                    len_arr.copy_from_slice(&bytes[pos..pos+4]);
                    let item_len = u32::from_le_bytes(len_arr) as usize;
                    pos += 4;
                    items.push(Self::decode(&bytes[pos..pos+item_len]));
                    pos += item_len;
                }
                PayloadValue::List(items)
            }
            _ => panic!("unknown payload type"),
        }
    }
}
