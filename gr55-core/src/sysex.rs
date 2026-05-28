use std::borrow::Cow;

use crate::codec::CodecError;

pub const SOX: u8 = 0xF0;
pub const EOX: u8 = 0xF7;
pub const MANUFACTURER_ROLAND: u8 = 0x41;
pub const MODEL_ID_GR55: [u8; 3] = [0x00, 0x00, 0x53];
pub const DEVICE_ID_DEFAULT: u8 = 0x10;
pub const DEVICE_ID_BROADCAST: u8 = 0x7F;

const CMD_DT1: u8 = 0x12;
const CMD_RQ1: u8 = 0x11;

const MIN_FRAME_LEN: usize = 13;
const HEADER_LEN: usize = 7;
const ADDRESS_LEN: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame<'a> {
    Dt1 {
        device_id: u8,
        address: [u8; 4],
        data: Cow<'a, [u8]>,
    },
    Rq1 {
        device_id: u8,
        address: [u8; 4],
        size: u32,
    },
}

impl<'a> Frame<'a> {
    pub fn device_id(&self) -> u8 {
        match *self {
            Frame::Dt1 { device_id, .. } | Frame::Rq1 { device_id, .. } => device_id,
        }
    }

    pub fn address(&self) -> [u8; 4] {
        match *self {
            Frame::Dt1 { address, .. } | Frame::Rq1 { address, .. } => address,
        }
    }

    pub fn into_owned(self) -> Frame<'static> {
        match self {
            Frame::Dt1 {
                device_id,
                address,
                data,
            } => Frame::Dt1 {
                device_id,
                address,
                data: Cow::Owned(data.into_owned()),
            },
            Frame::Rq1 {
                device_id,
                address,
                size,
            } => Frame::Rq1 {
                device_id,
                address,
                size,
            },
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let payload_len = match self {
            Frame::Dt1 { data, .. } => data.len(),
            Frame::Rq1 { .. } => ADDRESS_LEN,
        };
        let mut out = Vec::with_capacity(MIN_FRAME_LEN + payload_len);
        out.push(SOX);
        out.push(MANUFACTURER_ROLAND);
        out.push(self.device_id());
        out.extend_from_slice(&MODEL_ID_GR55);
        match self {
            Frame::Dt1 { address, data, .. } => {
                out.push(CMD_DT1);
                out.extend_from_slice(address);
                out.extend_from_slice(data);
            }
            Frame::Rq1 { address, size, .. } => {
                out.push(CMD_RQ1);
                out.extend_from_slice(address);
                out.extend_from_slice(&encode_rq1_size(*size));
            }
        }
        let checksum = roland_checksum(&out[HEADER_LEN..]);
        out.push(checksum);
        out.push(EOX);
        out
    }

    pub fn try_parse(bytes: &'a [u8]) -> Result<(Self, usize), CodecError> {
        let (frame, consumed, declared, computed) = Self::try_parse_inner(bytes)?;
        if declared != computed {
            return Err(CodecError::BadChecksum { computed, declared });
        }
        Ok((frame, consumed))
    }

    /// Parse a frame without enforcing the embedded checksum. Returns the parsed
    /// frame, bytes consumed, and a [`ChecksumStatus`] describing whether the
    /// declared checksum matched the computed one.
    ///
    /// FloorBoard's saved `.syx` files contain frames with checksum errors that
    /// FloorBoard silently corrects on load — use this when consuming FloorBoard
    /// fixtures. Live device wire data should always pass strict [`Frame::try_parse`].
    pub fn try_parse_unchecked(
        bytes: &'a [u8],
    ) -> Result<(Self, usize, ChecksumStatus), CodecError> {
        let (frame, consumed, declared, computed) = Self::try_parse_inner(bytes)?;
        Ok((frame, consumed, ChecksumStatus { declared, computed }))
    }

    fn try_parse_inner(bytes: &'a [u8]) -> Result<(Self, usize, u8, u8), CodecError> {
        if bytes.len() < MIN_FRAME_LEN {
            return Err(CodecError::TooShort {
                got: bytes.len(),
                min: MIN_FRAME_LEN,
            });
        }
        if bytes[0] != SOX {
            return Err(CodecError::NotSysEx(bytes[0]));
        }
        if bytes[1] != MANUFACTURER_ROLAND {
            return Err(CodecError::NotRoland(bytes[1]));
        }
        let device_id = bytes[2];
        let model_id = [bytes[3], bytes[4], bytes[5]];
        if model_id != MODEL_ID_GR55 {
            return Err(CodecError::WrongModelId(model_id));
        }
        let cmd = bytes[6];
        let address = [bytes[7], bytes[8], bytes[9], bytes[10]];
        let eox_offset = bytes[11..]
            .iter()
            .position(|&b| b == EOX)
            .ok_or(CodecError::MissingEox)?;
        let eox_pos = 11 + eox_offset;
        if eox_pos < 12 {
            return Err(CodecError::TooShort {
                got: bytes.len(),
                min: MIN_FRAME_LEN,
            });
        }
        let declared_checksum = bytes[eox_pos - 1];
        let payload = &bytes[11..eox_pos - 1];

        let mut checksum_bytes = Vec::with_capacity(ADDRESS_LEN + payload.len());
        checksum_bytes.extend_from_slice(&address);
        checksum_bytes.extend_from_slice(payload);
        let computed_checksum = roland_checksum(&checksum_bytes);

        let frame = match cmd {
            CMD_DT1 => Frame::Dt1 {
                device_id,
                address,
                data: Cow::Borrowed(payload),
            },
            CMD_RQ1 => {
                if payload.len() != ADDRESS_LEN {
                    return Err(CodecError::BadRq1Size(payload.len()));
                }
                let size = decode_rq1_size([payload[0], payload[1], payload[2], payload[3]]);
                Frame::Rq1 {
                    device_id,
                    address,
                    size,
                }
            }
            other => return Err(CodecError::UnknownCommand(other)),
        };
        Ok((frame, eox_pos + 1, declared_checksum, computed_checksum))
    }
}

pub fn roland_checksum(addr_plus_data: &[u8]) -> u8 {
    let sum: u32 = addr_plus_data.iter().map(|&b| b as u32).sum();
    ((128 - (sum % 128)) % 128) as u8
}

pub fn encode_rq1_size(size: u32) -> [u8; 4] {
    [
        ((size >> 21) & 0x7F) as u8,
        ((size >> 14) & 0x7F) as u8,
        ((size >> 7) & 0x7F) as u8,
        (size & 0x7F) as u8,
    ]
}

pub fn decode_rq1_size(bytes: [u8; 4]) -> u32 {
    ((bytes[0] as u32) << 21)
        | ((bytes[1] as u32) << 14)
        | ((bytes[2] as u32) << 7)
        | (bytes[3] as u32)
}

pub fn parse_frames(bytes: &[u8]) -> FrameIter<'_> {
    FrameIter {
        bytes,
        strict: true,
    }
}

/// Like [`parse_frames`] but tolerates frames whose embedded checksum is wrong
/// (as found in FloorBoard's saved `.syx` files). Each yielded item carries a
/// [`ChecksumStatus`] indicating whether the original frame's checksum matched.
pub fn parse_frames_unchecked(bytes: &[u8]) -> FrameIterUnchecked<'_> {
    FrameIterUnchecked { bytes }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChecksumStatus {
    pub declared: u8,
    pub computed: u8,
}

impl ChecksumStatus {
    pub fn is_valid(&self) -> bool {
        self.declared == self.computed
    }
}

pub struct FrameIter<'a> {
    bytes: &'a [u8],
    strict: bool,
}

impl<'a> Iterator for FrameIter<'a> {
    type Item = Result<Frame<'a>, CodecError>;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.bytes.iter().position(|&b| b == SOX)?;
        self.bytes = &self.bytes[start..];
        let result = if self.strict {
            Frame::try_parse(self.bytes)
        } else {
            Frame::try_parse_unchecked(self.bytes).map(|(f, c, _)| (f, c))
        };
        match result {
            Ok((frame, consumed)) => {
                self.bytes = &self.bytes[consumed..];
                Some(Ok(frame))
            }
            Err(e) => {
                self.bytes = &self.bytes[1..];
                Some(Err(e))
            }
        }
    }
}

pub struct FrameIterUnchecked<'a> {
    bytes: &'a [u8],
}

impl<'a> Iterator for FrameIterUnchecked<'a> {
    type Item = Result<(Frame<'a>, ChecksumStatus), CodecError>;

    fn next(&mut self) -> Option<Self::Item> {
        let start = self.bytes.iter().position(|&b| b == SOX)?;
        self.bytes = &self.bytes[start..];
        match Frame::try_parse_unchecked(self.bytes) {
            Ok((frame, consumed, status)) => {
                self.bytes = &self.bytes[consumed..];
                Some(Ok((frame, status)))
            }
            Err(e) => {
                self.bytes = &self.bytes[1..];
                Some(Err(e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_zero_sum_is_zero() {
        assert_eq!(roland_checksum(&[0x00]), 0x00);
        assert_eq!(roland_checksum(&[]), 0x00);
    }

    #[test]
    fn checksum_matches_floorboard_system_first_frame() {
        // floorboard_system_area.syx first frame: address 01 00 00 00, data 00 39, declared chk 46.
        // sum = 0x01 + 0x39 = 0x3A = 58; (128 - 58) % 128 = 70 = 0x46.
        let payload = [0x01, 0x00, 0x00, 0x00, 0x00, 0x39];
        assert_eq!(roland_checksum(&payload), 0x46);
    }

    #[test]
    fn rq1_size_roundtrip() {
        for &size in &[0u32, 1, 0x7F, 0x80, 0x3FFF, 0x4000, 0x1FF_FFFF, 0x0FFF_FFFF] {
            assert_eq!(
                decode_rq1_size(encode_rq1_size(size)),
                size,
                "size {size:#x}"
            );
        }
    }

    #[test]
    fn dt1_roundtrip() {
        let original = Frame::Dt1 {
            device_id: 0x10,
            address: [0x18, 0x00, 0x00, 0x00],
            data: Cow::Owned(b"Init Patch      ".to_vec()),
        };
        let bytes = original.encode();
        let (parsed, consumed) = Frame::try_parse(&bytes).unwrap();
        assert_eq!(parsed, original);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn rq1_roundtrip() {
        let original = Frame::Rq1 {
            device_id: 0x10,
            address: [0x60, 0x00, 0x00, 0x00],
            size: 0x4000,
        };
        let bytes = original.encode();
        let (parsed, consumed) = Frame::try_parse(&bytes).unwrap();
        assert_eq!(parsed, original);
        assert_eq!(consumed, bytes.len());
    }

    fn load_floorboard_fixture(bytes: &[u8]) -> Vec<(Frame<'_>, ChecksumStatus)> {
        parse_frames_unchecked(bytes)
            .collect::<Result<Vec<_>, _>>()
            .expect("FloorBoard fixture should parse structurally (checksums may be wrong)")
    }

    #[test]
    fn parse_floorboard_system_syx() {
        let bytes: &[u8] = include_bytes!("../tests/fixtures/floorboard_system_area.syx");
        let frames = load_floorboard_fixture(bytes);
        assert!(
            !frames.is_empty(),
            "system.syx should contain at least one frame"
        );

        let (first, _) = &frames[0];
        match first {
            Frame::Dt1 {
                device_id, address, ..
            } => {
                assert_eq!(*device_id, 0x10);
                assert_eq!(*address, [0x01, 0x00, 0x00, 0x00]);
            }
            Frame::Rq1 { .. } => panic!("expected DT1 in system.syx, found RQ1"),
        }

        // Re-encoding any frame must produce a frame that survives strict round-trip.
        for (i, (frame, _)) in frames.iter().enumerate() {
            let encoded = frame.encode();
            let (reparsed, _) = Frame::try_parse(&encoded)
                .unwrap_or_else(|e| panic!("frame {i} strict re-parse failed: {e}"));
            assert_eq!(&reparsed, frame, "frame {i} round-trip mismatch");
        }
    }

    #[test]
    fn floorboard_system_syx_has_some_bad_checksums() {
        // FloorBoard saves .syx files with checksum errors that it corrects on load
        // (SysxIO.cpp:144). Document the quirk so future fixture changes are caught.
        let bytes: &[u8] = include_bytes!("../tests/fixtures/floorboard_system_area.syx");
        let frames = load_floorboard_fixture(bytes);
        let bad = frames.iter().filter(|(_, s)| !s.is_valid()).count();
        assert!(
            bad > 0,
            "expected at least one bad-checksum frame in FloorBoard system.syx"
        );
    }

    #[test]
    fn parse_floorboard_default_patch_syx() {
        let bytes: &[u8] = include_bytes!("../tests/fixtures/floorboard_default_patch.syx");
        let frames = load_floorboard_fixture(bytes);
        assert!(
            frames.len() > 1,
            "default.syx should contain multiple frames (got {})",
            frames.len()
        );

        let (first, _) = &frames[0];
        match first {
            Frame::Dt1 {
                device_id,
                address,
                data,
            } => {
                assert_eq!(*device_id, 0x10);
                assert_eq!(*address, [0x18, 0x00, 0x00, 0x00]);
                // Per FloorBoard midi.xml <Structure>: the patch starts with a
                // Guitar/Bass-Mode byte (data[0] = 00=Guitar / 01=Bass), then 16
                // bytes of name characters starting at sub-address 01.
                assert_eq!(
                    data[0], 0x00,
                    "first patch byte should be Guitar/Bass Mode = 0 (Guitar)"
                );
                assert_eq!(
                    &data[1..17],
                    b"Init Patch      ",
                    "bytes 1..17 should be the 16-char patch name field"
                );
            }
            Frame::Rq1 { .. } => panic!("expected DT1 in default.syx, found RQ1"),
        }

        for (i, (frame, _)) in frames.iter().enumerate() {
            assert!(
                matches!(frame, Frame::Dt1 { .. }),
                "frame {i} should be DT1"
            );
            let encoded = frame.encode();
            let (reparsed, _) = Frame::try_parse(&encoded)
                .unwrap_or_else(|e| panic!("frame {i} strict re-parse failed: {e}"));
            assert_eq!(&reparsed, frame, "frame {i} round-trip mismatch");
        }
    }

    #[test]
    fn parse_floorboard_ez_tone_library_syx() {
        let bytes: &[u8] = include_bytes!("../tests/fixtures/floorboard_ez_tone_library.syx");
        let frames = load_floorboard_fixture(bytes);
        assert!(frames.len() > 10, "EZ-Tone library should hold many frames");
        for (frame, _) in &frames {
            assert_eq!(frame.device_id(), 0x10);
        }
    }

    #[test]
    fn try_parse_rejects_wrong_model_id() {
        // BOSS RE-202 model ID (5 bytes); GR-55 expects 3 bytes 00 00 53.
        let bytes = [
            SOX,
            MANUFACTURER_ROLAND,
            0x10,
            0x00,
            0x00,
            0x18,
            0x12,
            0x00,
            0x00,
            0x00,
            0x00,
            0x00,
            EOX,
        ];
        let err = Frame::try_parse(&bytes).unwrap_err();
        assert!(matches!(err, CodecError::WrongModelId(_)), "got {err:?}");
    }

    #[test]
    fn try_parse_rejects_bad_checksum() {
        let mut bytes = Frame::Dt1 {
            device_id: 0x10,
            address: [0x01, 0x00, 0x00, 0x00],
            data: Cow::Borrowed(&[0x00, 0x39]),
        }
        .encode();
        // Corrupt the checksum byte (second-to-last).
        let chk_idx = bytes.len() - 2;
        bytes[chk_idx] = bytes[chk_idx].wrapping_add(1) & 0x7F;
        let err = Frame::try_parse(&bytes).unwrap_err();
        assert!(matches!(err, CodecError::BadChecksum { .. }), "got {err:?}");
    }
}
