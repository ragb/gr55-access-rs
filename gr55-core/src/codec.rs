use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CodecError {
    #[error("expected SysEx Status (F0), got {0:#04x}")]
    NotSysEx(u8),

    #[error("missing EOX (F7)")]
    MissingEox,

    #[error("not a Roland frame: manufacturer byte {0:#04x}")]
    NotRoland(u8),

    #[error("wrong model ID: expected 00 00 53, got {0:02x?}")]
    WrongModelId([u8; 3]),

    #[error("unknown command byte {0:#04x}")]
    UnknownCommand(u8),

    #[error("checksum mismatch: computed {computed:#04x}, declared {declared:#04x}")]
    BadChecksum { computed: u8, declared: u8 },

    #[error("frame too short: {got} bytes (need at least {min})")]
    TooShort { got: usize, min: usize },

    #[error("RQ1 payload must be exactly 4 bytes, got {0}")]
    BadRq1Size(usize),
}
