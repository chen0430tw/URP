#[cfg(test)]
mod tests {
    use crate::packet::{PayloadCodec, PayloadValue, URPPacket};
    use crate::ir::{MergeMode, Opcode};

    #[test]
    fn test_build_i64_packet() {
        let packet = URPPacket::build(
            1,  // opcode_id for UConstI64
            MergeMode::List,
            "test_block",
            "",
            &[],
        );
        let header = packet.header();
        assert_eq!(header.opcode_id, 1);
        assert_eq!(packet.merge_mode, MergeMode::List);
    }

    #[test]
    fn test_build_string_packet() {
        let packet = URPPacket::build(
            2,  // opcode_id for UConstStr
            MergeMode::Concat,
            "test_block",
            "",
            &[],
        );
        let header = packet.header();
        assert_eq!(header.opcode_id, 2);
        assert_eq!(packet.merge_mode, MergeMode::Concat);
    }

    #[test]
    fn test_i64_codec_roundtrip() {
        let value = PayloadValue::I64(12345);
        let codec = PayloadCodec::I64Codec;

        let encoded = codec.encode(&value).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(value, decoded);
    }

    #[test]
    fn test_string_codec_roundtrip() {
        let value = PayloadValue::String("test string".to_string());
        let codec = PayloadCodec::StringCodec;

        let encoded = codec.encode(&value).unwrap();
        let decoded = codec.decode(&encoded).unwrap();

        assert_eq!(value, decoded);
    }

    #[test]
    fn test_packet_to_bytes() {
        let packet = URPPacket::build(
            1,  // opcode_id
            MergeMode::Sum,
            "block_test",
            "",
            &[],
        );

        let bytes = packet.to_bytes();
        assert!(!bytes.is_empty());

        let decoded = URPPacket::from_bytes(&bytes);
        assert!(decoded.is_ok());

        let decoded_packet = decoded.unwrap();
        let header = decoded_packet.header();
        assert_eq!(header.merge_mode, MergeMode::Sum as u16);
    }

    #[test]
    fn test_empty_payload() {
        let packet = URPPacket::build(
            1,
            MergeMode::List,
            "empty_test",
            "",
            &[],
        );

        let bytes = packet.to_bytes();
        let decoded = URPPacket::from_bytes(&bytes).unwrap();
        let header = decoded.header();
        assert_eq!(header.src_len, "empty_test".len() as u16);
    }

    #[test]
    fn test_large_payload() {
        let large_payload = vec![0x42u8; 10000];
        let packet = URPPacket::build(
            2,
            MergeMode::Concat,
            "large_test",
            "",
            &large_payload,
        );

        let bytes = packet.to_bytes();
        let decoded = URPPacket::from_bytes(&bytes).unwrap();

        let payload = decoded.payload();
        assert_eq!(payload.len(), 10000);
    }

    #[test]
    fn test_merge_mode_serialization() {
        let modes = vec![
            MergeMode::List,
            MergeMode::Sum,
            MergeMode::Concat,
            MergeMode::ReduceMax,
        ];

        for mode in modes {
            let packet = URPPacket::build(
                1,
                mode,
                "test",
                "",
                &[],
            );

            let bytes = packet.to_bytes();
            let decoded = URPPacket::from_bytes(&bytes).unwrap();
            let header = decoded.header();

            assert_eq!(header.merge_mode, mode as u16);
        }
    }
}
