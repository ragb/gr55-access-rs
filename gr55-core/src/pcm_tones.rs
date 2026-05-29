//! Static name + category lookup for the 910 PCM tones in the GR-55
//! catalog. The data is generated at build time from `data/midi.xml` —
//! see [`crate::patch::PcmToneIndex`] for the wire encoding.

include!(concat!(env!("OUT_DIR"), "/pcm_tones.rs"));
