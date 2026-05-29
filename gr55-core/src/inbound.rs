//! Classify a single inbound MIDI message and decode it into the
//! richest typed shape the GR-55 protocol supports.
//!
//! The editor's session callback hands raw bytes in (anything the device
//! emits — short channel messages, full SysEx frames, identity replies)
//! and gets back a tagged union so it can route by `kind` instead of
//! re-implementing the parsing every place it wants to react.
//!
//! Only what the editor actually consumes is modelled. Unmodelled
//! status bytes fall through to [`InboundMessage::Other`] with the
//! first byte preserved for diagnostics; calling code can decide
//! whether to log, ignore, or extend the classifier.

use serde::{Deserialize, Serialize};

use crate::sysex::{Frame, MANUFACTURER_ROLAND, MODEL_ID_GR55};

#[cfg(feature = "tsify")]
use tsify_next::Tsify;

/// Tagged enum describing one decoded inbound message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "tsify", derive(Tsify))]
#[cfg_attr(feature = "tsify", tsify(into_wasm_abi, from_wasm_abi))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboundMessage {
    /// Universal Non-Realtime Identity Reply matching the GR-55's
    /// Roland family code `53 02` + family number `00 00`.
    ///
    /// Reply layout: `F0 7E [dev] 06 02 41 53 02 00 00 [sw_rev_4] F7`.
    IdentityReply {
        /// `dd` byte from the reply — the device's own ID byte.
        device_id: u8,
        /// 4-byte software revision (raw bytes, untyped).
        software_revision: [u8; 4],
    },
    /// Roland DT1 (data set) frame addressed to the GR-55. The
    /// framing + checksum have already been validated.
    Dt1 {
        device_id: u8,
        address: [u8; 4],
        #[cfg_attr(feature = "tsify", tsify(type = "Uint8Array"))]
        data: Vec<u8>,
    },
    /// Roland RQ1 (data request) frame. Unusual as an inbound message
    /// but included for completeness — some setups echo MIDI back.
    Rq1 {
        device_id: u8,
        address: [u8; 4],
        size: u32,
    },
    /// MIDI Program Change. `channel` is 1..=16; `program` is 0..=127.
    /// The GR-55 sends this when a patch is selected on the device
    /// (when the system area's "MIDI/USB → PC Out" is on).
    ProgramChange { channel: u8, program: u8 },
    /// MIDI Control Change.
    ControlChange { channel: u8, cc: u8, value: u8 },
    /// Anything else — non-Roland SysEx, status bytes we don't model,
    /// or framing errors. The first status byte is preserved so callers
    /// can decide what to do.
    Other { status: u8 },
}

/// Inspect one inbound MIDI message and decode it into an
/// [`InboundMessage`].
///
/// Accepts any frame the editor's session callback might deliver —
/// channel messages (2-3 bytes), full SysEx (variable length), or
/// anything else. Returns [`InboundMessage::Other`] when the bytes
/// don't match a shape this classifier knows.
pub fn classify_inbound(bytes: &[u8]) -> InboundMessage {
    if bytes.is_empty() {
        return InboundMessage::Other { status: 0 };
    }
    let status = bytes[0];

    // Channel messages: Program Change (0xCn) and Control Change (0xBn).
    if (status & 0xF0) == 0xC0 && bytes.len() >= 2 {
        return InboundMessage::ProgramChange {
            channel: (status & 0x0F) + 1,
            program: bytes[1] & 0x7F,
        };
    }
    if (status & 0xF0) == 0xB0 && bytes.len() >= 3 {
        return InboundMessage::ControlChange {
            channel: (status & 0x0F) + 1,
            cc: bytes[1] & 0x7F,
            value: bytes[2] & 0x7F,
        };
    }

    // Universal Non-Realtime Identity Reply — Roland (0x41) +
    // family code 53 02 + family number 00 00 identify a GR-55.
    if is_gr55_identity_reply(bytes) {
        return InboundMessage::IdentityReply {
            device_id: bytes[2],
            software_revision: [bytes[10], bytes[11], bytes[12], bytes[13]],
        };
    }

    // Roland SysEx addressed to the GR-55. Frame::try_parse_unchecked
    // requires the Roland manufacturer + GR-55 model ID, so other
    // manufacturers' SysEx (and non-GR-55 Roland devices) fall
    // through to `Other`.
    if status == 0xF0
        && bytes.len() >= 11
        && bytes[1] == MANUFACTURER_ROLAND
        && bytes[3..6] == MODEL_ID_GR55
    {
        if let Ok((frame, _, _)) = Frame::try_parse_unchecked(bytes) {
            return match frame {
                Frame::Dt1 {
                    device_id,
                    address,
                    data,
                } => InboundMessage::Dt1 {
                    device_id,
                    address,
                    data: data.into_owned(),
                },
                Frame::Rq1 {
                    device_id,
                    address,
                    size,
                } => InboundMessage::Rq1 {
                    device_id,
                    address,
                    size,
                },
            };
        }
    }

    InboundMessage::Other { status }
}

fn is_gr55_identity_reply(bytes: &[u8]) -> bool {
    bytes.len() >= 15
        && bytes[0] == 0xF0
        && bytes[bytes.len() - 1] == 0xF7
        && bytes[1] == 0x7E
        && bytes[3] == 0x06
        && bytes[4] == 0x02
        && bytes[5] == MANUFACTURER_ROLAND
        && bytes[6] == 0x53
        && bytes[7] == 0x02
        && bytes[8] == 0x00
        && bytes[9] == 0x00
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_program_change() {
        // 0xC5 = PC on channel 6, program 42.
        let msg = classify_inbound(&[0xC5, 0x2A]);
        assert_eq!(
            msg,
            InboundMessage::ProgramChange {
                channel: 6,
                program: 42
            }
        );
    }

    #[test]
    fn classifies_control_change() {
        // 0xB0 = CC on channel 1, CC 7, value 100.
        let msg = classify_inbound(&[0xB0, 0x07, 0x64]);
        assert_eq!(
            msg,
            InboundMessage::ControlChange {
                channel: 1,
                cc: 7,
                value: 100
            }
        );
    }

    #[test]
    fn classifies_gr55_identity_reply() {
        // Synthesized reply matching the pattern from sysex-notes.md §3.
        let reply = [
            0xF0, 0x7E, 0x10, 0x06, 0x02, 0x41, 0x53, 0x02, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04,
            0xF7,
        ];
        let msg = classify_inbound(&reply);
        assert_eq!(
            msg,
            InboundMessage::IdentityReply {
                device_id: 0x10,
                software_revision: [0x01, 0x02, 0x03, 0x04],
            }
        );
    }

    #[test]
    fn non_gr55_identity_reply_falls_through() {
        // Same shape, different family code → Other.
        let reply = [
            0xF0, 0x7E, 0x10, 0x06, 0x02, 0x41, 0x18, 0x04, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04,
            0xF7,
        ];
        let msg = classify_inbound(&reply);
        match msg {
            InboundMessage::Other { status } => assert_eq!(status, 0xF0),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn classifies_dt1_addressed_to_gr55() {
        // Construct via Frame::encode so the checksum is correct.
        let frame = Frame::Dt1 {
            device_id: 0x10,
            address: [0x18, 0x00, 0x00, 0x00],
            data: std::borrow::Cow::Owned(vec![0x49, 0x6e, 0x69, 0x74]),
        };
        let bytes = frame.encode();
        let msg = classify_inbound(&bytes);
        assert_eq!(
            msg,
            InboundMessage::Dt1 {
                device_id: 0x10,
                address: [0x18, 0x00, 0x00, 0x00],
                data: vec![0x49, 0x6e, 0x69, 0x74],
            }
        );
    }

    #[test]
    fn classifies_rq1_addressed_to_gr55() {
        let frame = Frame::Rq1 {
            device_id: 0x10,
            address: [0x60, 0x00, 0x00, 0x00],
            size: 256,
        };
        let bytes = frame.encode();
        let msg = classify_inbound(&bytes);
        assert_eq!(
            msg,
            InboundMessage::Rq1 {
                device_id: 0x10,
                address: [0x60, 0x00, 0x00, 0x00],
                size: 256,
            }
        );
    }

    #[test]
    fn non_roland_sysex_falls_through() {
        // F0 00 21 24 (Morningstar manufacturer ID) ... F7.
        let bytes = [0xF0, 0x00, 0x21, 0x24, 0x00, 0xF7];
        let msg = classify_inbound(&bytes);
        match msg {
            InboundMessage::Other { status } => assert_eq!(status, 0xF0),
            other => panic!("expected Other, got {other:?}"),
        }
    }

    #[test]
    fn empty_input_is_other() {
        let msg = classify_inbound(&[]);
        assert_eq!(msg, InboundMessage::Other { status: 0 });
    }
}
