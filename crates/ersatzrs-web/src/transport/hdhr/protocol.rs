//! `HDHomeRun` wire protocol primitives shared between the UDP discovery
//! responder and its tests. See `$ERSATZTV_SRC/hdhr.md` for the packet
//! format reference.

pub const UDP_DISCOVERY_PORT: u16 = 65001;

pub const TYPE_DISCOVER_REQ: u16 = 0x0002;
pub const TYPE_DISCOVER_RPY: u16 = 0x0003;

pub const TAG_DEVICE_TYPE: u8 = 0x01;
pub const TAG_DEVICE_ID: u8 = 0x02;
pub const TAG_TUNER_COUNT: u8 = 0x10;
pub const TAG_LINEUP_URL: u8 = 0x27;
pub const TAG_BASE_URL: u8 = 0x2A;

pub const DEVICE_TYPE_TUNER: u32 = 0x0000_0001;
pub const DEVICE_TYPE_WILDCARD: u32 = 0xFFFF_FFFF;
pub const DEVICE_ID_WILDCARD: u32 = 0xFFFF_FFFF;

/// Minimum packet size: 2 (type) + 2 (length) + 4 (crc) = 8 bytes.
pub const MIN_PACKET_SIZE: usize = 8;
pub const MAX_PACKET_SIZE: usize = 1460;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryRequest {
    pub device_type: u32,
    pub device_id: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("packet too short (need {MIN_PACKET_SIZE}, got {0})")]
    TooShort(usize),
    #[error("packet type {0:#06x} is not a discovery request")]
    WrongType(u16),
    #[error("declared payload length {declared} exceeds remaining {available}")]
    PayloadOverflow { declared: usize, available: usize },
    #[error("crc mismatch: computed {computed:#010x}, expected {expected:#010x}")]
    CrcMismatch { computed: u32, expected: u32 },
    #[error("malformed tlv entry")]
    MalformedTlv,
}

pub fn encode_discovery_reply(
    device_type: u32,
    device_id: u32,
    tuner_count: u8,
    base_url: &str,
    lineup_url: &str,
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(64);
    write_tlv(&mut payload, TAG_DEVICE_TYPE, &device_type.to_be_bytes());
    write_tlv(&mut payload, TAG_DEVICE_ID, &device_id.to_be_bytes());
    write_tlv(&mut payload, TAG_TUNER_COUNT, &[tuner_count]);
    write_tlv(&mut payload, TAG_BASE_URL, base_url.as_bytes());
    write_tlv(&mut payload, TAG_LINEUP_URL, lineup_url.as_bytes());
    encode_packet(TYPE_DISCOVER_RPY, &payload)
}

pub fn encode_packet(packet_type: u16, payload: &[u8]) -> Vec<u8> {
    let payload_len =
        u16::try_from(payload.len()).expect("hdhr payload must be at most u16::MAX bytes");
    let mut buffer = Vec::with_capacity(MIN_PACKET_SIZE + payload.len());
    buffer.extend_from_slice(&packet_type.to_be_bytes());
    buffer.extend_from_slice(&payload_len.to_be_bytes());
    buffer.extend_from_slice(payload);
    let crc = crc32(&buffer);
    buffer.extend_from_slice(&crc.to_le_bytes());
    buffer
}

pub fn decode_discovery_request(packet: &[u8]) -> Result<DiscoveryRequest, ProtocolError> {
    if packet.len() < MIN_PACKET_SIZE {
        return Err(ProtocolError::TooShort(packet.len()));
    }
    let packet_type = u16::from_be_bytes([packet[0], packet[1]]);
    if packet_type != TYPE_DISCOVER_REQ {
        return Err(ProtocolError::WrongType(packet_type));
    }
    let payload_len = usize::from(u16::from_be_bytes([packet[2], packet[3]]));
    if payload_len + MIN_PACKET_SIZE > packet.len() {
        return Err(ProtocolError::PayloadOverflow {
            declared: payload_len,
            available: packet.len() - MIN_PACKET_SIZE,
        });
    }

    let header_and_payload = &packet[..4 + payload_len];
    let expected_crc = u32::from_le_bytes([
        packet[4 + payload_len],
        packet[5 + payload_len],
        packet[6 + payload_len],
        packet[7 + payload_len],
    ]);
    let computed = crc32(header_and_payload);
    if computed != expected_crc {
        return Err(ProtocolError::CrcMismatch {
            computed,
            expected: expected_crc,
        });
    }

    let payload = &packet[4..4 + payload_len];
    let mut device_type = DEVICE_TYPE_WILDCARD;
    let mut device_id = DEVICE_ID_WILDCARD;
    for (tag, value) in read_tlv_sequence(payload)? {
        match tag {
            TAG_DEVICE_TYPE if value.len() == 4 => {
                device_type = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
            }
            TAG_DEVICE_ID if value.len() == 4 => {
                device_id = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
            }
            _ => {} // unknown tags must be ignored per protocol
        }
    }
    Ok(DiscoveryRequest {
        device_type,
        device_id,
    })
}

pub fn should_reply(request: &DiscoveryRequest, our_device_id: u32, supported_type: u32) -> bool {
    let type_match =
        request.device_type == DEVICE_TYPE_WILDCARD || request.device_type == supported_type;
    let id_match = request.device_id == DEVICE_ID_WILDCARD || request.device_id == our_device_id;
    type_match && id_match
}

fn write_tlv(buffer: &mut Vec<u8>, tag: u8, value: &[u8]) {
    buffer.push(tag);
    let len = value.len();
    if len <= 127 {
        #[allow(clippy::cast_possible_truncation)]
        buffer.push(len as u8);
    } else {
        let low = u8::try_from(len & 0x7F).expect("masked with 0x7F fits in u8") | 0x80;
        #[allow(clippy::cast_possible_truncation)]
        let high = (len >> 7) as u8;
        buffer.push(low);
        buffer.push(high);
    }
    buffer.extend_from_slice(value);
}

fn read_tlv_sequence(payload: &[u8]) -> Result<Vec<(u8, &[u8])>, ProtocolError> {
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while cursor < payload.len() {
        if cursor + 2 > payload.len() {
            return Err(ProtocolError::MalformedTlv);
        }
        let tag = payload[cursor];
        cursor += 1;
        let first_length_byte = payload[cursor];
        cursor += 1;
        let length = if first_length_byte & 0x80 == 0 {
            usize::from(first_length_byte)
        } else {
            if cursor >= payload.len() {
                return Err(ProtocolError::MalformedTlv);
            }
            let high = payload[cursor];
            cursor += 1;
            usize::from(first_length_byte & 0x7F) | (usize::from(high) << 7)
        };
        if cursor + length > payload.len() {
            return Err(ProtocolError::MalformedTlv);
        }
        out.push((tag, &payload[cursor..cursor + length]));
        cursor += length;
    }
    Ok(out)
}

pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Derive a 32-bit HDHomeRun-style device id from the first four bytes of the
/// stored UUID. Stable across restarts because the UUID is persisted.
#[must_use]
pub fn derive_device_id_from_uuid(uuid: uuid::Uuid) -> u32 {
    let bytes = uuid.as_bytes();
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_reference_vector() {
        // "123456789" -> 0xCBF43926 per the standard CRC-32/ISO-HDLC test vector.
        let crc = crc32(b"123456789");
        assert_eq!(crc, 0xCBF4_3926);
    }

    #[test]
    fn tlv_short_length_roundtrip() {
        let mut buf = Vec::new();
        write_tlv(&mut buf, TAG_DEVICE_ID, b"abc");
        let decoded = read_tlv_sequence(&buf).expect("decode");
        assert_eq!(decoded, vec![(TAG_DEVICE_ID, b"abc".as_slice())]);
    }

    #[test]
    fn tlv_long_length_roundtrip() {
        let value = vec![0xAB; 300];
        let mut buf = Vec::new();
        write_tlv(&mut buf, TAG_BASE_URL, &value);
        let decoded = read_tlv_sequence(&buf).expect("decode");
        assert_eq!(decoded, vec![(TAG_BASE_URL, value.as_slice())]);
    }

    #[test]
    fn discovery_request_roundtrip() {
        let mut payload = Vec::new();
        write_tlv(
            &mut payload,
            TAG_DEVICE_TYPE,
            &DEVICE_TYPE_WILDCARD.to_be_bytes(),
        );
        write_tlv(
            &mut payload,
            TAG_DEVICE_ID,
            &DEVICE_ID_WILDCARD.to_be_bytes(),
        );
        let packet = encode_packet(TYPE_DISCOVER_REQ, &payload);

        let decoded = decode_discovery_request(&packet).expect("decode");
        assert_eq!(decoded.device_type, DEVICE_TYPE_WILDCARD);
        assert_eq!(decoded.device_id, DEVICE_ID_WILDCARD);
    }

    #[test]
    fn discovery_reply_encodes_expected_header_and_payload() {
        let reply = encode_discovery_reply(
            DEVICE_TYPE_TUNER,
            0x0102_0304,
            1,
            "http://example.test:8080",
            "http://example.test:8080/lineup.json",
        );
        let packet_type = u16::from_be_bytes([reply[0], reply[1]]);
        let payload_len = u16::from_be_bytes([reply[2], reply[3]]);
        assert_eq!(packet_type, TYPE_DISCOVER_RPY);
        assert_eq!(usize::from(payload_len), reply.len() - MIN_PACKET_SIZE);

        let header_and_payload = &reply[..reply.len() - 4];
        let expected_crc = crc32(header_and_payload);
        let actual_crc = u32::from_le_bytes([
            reply[reply.len() - 4],
            reply[reply.len() - 3],
            reply[reply.len() - 2],
            reply[reply.len() - 1],
        ]);
        assert_eq!(expected_crc, actual_crc);
    }

    #[test]
    fn crc_mismatch_is_rejected() {
        let mut payload = Vec::new();
        write_tlv(
            &mut payload,
            TAG_DEVICE_TYPE,
            &DEVICE_TYPE_WILDCARD.to_be_bytes(),
        );
        let mut packet = encode_packet(TYPE_DISCOVER_REQ, &payload);
        *packet.last_mut().unwrap() ^= 0xFF;
        match decode_discovery_request(&packet) {
            Err(ProtocolError::CrcMismatch { .. }) => {}
            other => panic!("expected crc mismatch, got {other:?}"),
        }
    }

    #[test]
    fn should_reply_honors_wildcards() {
        assert!(should_reply(
            &DiscoveryRequest {
                device_type: DEVICE_TYPE_WILDCARD,
                device_id: DEVICE_ID_WILDCARD,
            },
            0xDEAD_BEEF,
            DEVICE_TYPE_TUNER
        ));
        assert!(should_reply(
            &DiscoveryRequest {
                device_type: DEVICE_TYPE_TUNER,
                device_id: 0xDEAD_BEEF,
            },
            0xDEAD_BEEF,
            DEVICE_TYPE_TUNER
        ));
        assert!(!should_reply(
            &DiscoveryRequest {
                device_type: 0x0000_0005,
                device_id: DEVICE_ID_WILDCARD,
            },
            0xDEAD_BEEF,
            DEVICE_TYPE_TUNER
        ));
        assert!(!should_reply(
            &DiscoveryRequest {
                device_type: DEVICE_TYPE_WILDCARD,
                device_id: 0x0000_0001,
            },
            0xDEAD_BEEF,
            DEVICE_TYPE_TUNER
        ));
    }
}
