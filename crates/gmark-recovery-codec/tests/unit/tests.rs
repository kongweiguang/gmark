// @author kongweiguang

use super::*;

#[test]
fn frame_round_trips_and_advances_exactly() {
    let base_payload = r#"{"source":"中"}"#.as_bytes();
    let first = encode_record_payload(RecordKind::Base, base_payload).unwrap();
    let second = encode_record_payload(RecordKind::Edit, br#"{"start":0}"#).unwrap();
    let bytes = [first.as_slice(), second.as_slice()].concat();
    let decoded_first = decode_record(&bytes, 0).unwrap().unwrap();
    assert_eq!(decoded_first.kind, RecordKind::Base);
    assert_eq!(decoded_first.payload, base_payload);
    let decoded_second = decode_record(&bytes, decoded_first.next).unwrap().unwrap();
    assert_eq!(decoded_second.kind, RecordKind::Edit);
    assert_eq!(decoded_second.next, bytes.len());
}

#[test]
fn every_truncated_prefix_is_recoverable_as_an_incomplete_tail() {
    let frame = encode_record_payload(RecordKind::Base, b"payload").unwrap();
    for end in 0..frame.len() {
        assert_eq!(decode_record(&frame[..end], 0).unwrap(), None);
    }
    assert!(decode_record(&frame, 0).unwrap().is_some());
}

#[test]
fn crc_corruption_is_an_incomplete_tail() {
    let mut frame = encode_record_payload(RecordKind::Edit, b"payload").unwrap();
    *frame.last_mut().unwrap() ^= 0x80;
    assert_eq!(decode_record(&frame, 0).unwrap(), None);
}

#[test]
fn rejects_unknown_versions_kinds_flags_and_oversized_declarations() {
    let frame = encode_record_payload(RecordKind::Base, b"").unwrap();
    let mut version = frame.clone();
    version[4..6].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        decode_record(&version, 0),
        Err(CodecError::UnsupportedVersion(2))
    );
    let mut kind = frame.clone();
    kind[6] = 99;
    assert_eq!(
        decode_record(&kind, 0),
        Err(CodecError::UnknownRecordKind(99))
    );
    let mut flags = frame.clone();
    flags[7] = 1;
    assert_eq!(
        decode_record(&flags, 0),
        Err(CodecError::UnsupportedFlags(1))
    );
    let mut oversized = frame;
    oversized[8..16].copy_from_slice(&(MAX_RECORD_BYTES as u64 + 1).to_le_bytes());
    assert_eq!(
        decode_record(&oversized, 0),
        Err(CodecError::PayloadTooLarge)
    );
}
