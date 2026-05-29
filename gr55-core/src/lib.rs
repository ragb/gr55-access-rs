#![doc = include_str!("../README.md")]

pub mod address;
pub mod codec;
pub mod g5l;
pub mod help;
pub mod mfx_params;
pub(crate) mod mfx_tail_serde;
pub mod midi_map;
pub mod mod_params;
pub(crate) mod mod_tail_serde;
pub mod modeling_params;
pub(crate) mod modeling_tail_serde;
pub mod patch;
pub mod pcm_tail_params;

pub(crate) mod pcm_tail_serde;
pub mod pcm_tones;
pub mod sysex;
pub mod system;
#[cfg(test)]
pub(crate) mod test_support;

pub use address::{AddressSpace, PatchSlot, PatchSlotError};
pub use patch::{PatchArea, PatchMode, PatchName};
pub use system::SystemArea;

pub use codec::CodecError;
pub use sysex::{
    decode_rq1_size, encode_rq1_size, parse_frames, parse_frames_unchecked, roland_checksum,
    ChecksumStatus, Frame, FrameIter, FrameIterUnchecked, DEVICE_ID_BROADCAST, DEVICE_ID_DEFAULT,
    EOX, MANUFACTURER_ROLAND, MODEL_ID_GR55, SOX,
};
