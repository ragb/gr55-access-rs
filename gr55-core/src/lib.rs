#![doc = include_str!("../README.md")]

pub mod address;
pub mod codec;
pub mod midi_map;
pub mod sysex;
pub mod system;

pub use address::{AddressSpace, PatchSlot, PatchSlotError};
pub use system::SystemArea;

pub use codec::CodecError;
pub use sysex::{
    decode_rq1_size, encode_rq1_size, parse_frames, parse_frames_unchecked, roland_checksum,
    ChecksumStatus, Frame, FrameIter, FrameIterUnchecked, DEVICE_ID_BROADCAST, DEVICE_ID_DEFAULT,
    EOX, MANUFACTURER_ROLAND, MODEL_ID_GR55, SOX,
};
