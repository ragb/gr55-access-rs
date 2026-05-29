use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// `gr55` — CLI for editing, dumping, and syncing Roland GR-55 patches over MIDI.
#[derive(Debug, Parser)]
#[command(name = "gr55", version)]
pub struct Cli {
    /// MIDI input + output port substring (matched against both directions).
    /// If `--input-port` or `--output-port` are given they take precedence.
    /// Defaults to "Focusrite" to match the developer's USB MIDI setup;
    /// override for any other interface.
    #[arg(long, global = true, default_value = "Focusrite")]
    pub port: String,

    /// MIDI input port substring (overrides `--port`).
    #[arg(long, global = true)]
    pub input_port: Option<String>,

    /// MIDI output port substring (overrides `--port`).
    #[arg(long, global = true)]
    pub output_port: Option<String>,

    /// GR-55 SysEx device ID byte. Default `0x10` matches FloorBoard.
    #[arg(long, global = true, default_value_t = 0x10, value_parser = parse_hex_u8)]
    pub device_id: u8,

    /// Timeout for waiting on device replies (milliseconds).
    #[arg(long, global = true, default_value_t = 1500)]
    pub timeout_ms: u64,

    #[command(subcommand)]
    pub command: Command,
}

fn parse_hex_u8(s: &str) -> Result<u8, String> {
    let stripped = s.strip_prefix("0x").unwrap_or(s);
    u8::from_str_radix(stripped, 16).map_err(|e| format!("not a hex byte: {e}"))
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List available MIDI input/output ports.
    Ports,

    /// Send a Universal Identity Request and print the device's reply.
    Identity,

    /// Read the GR-55's System area and write it as YAML.
    Dump {
        #[command(flatten)]
        target: DumpTarget,
        /// Output YAML file path. Use `-` for stdout.
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Load a YAML file and write the resulting state back to the GR-55.
    Sync {
        #[command(flatten)]
        target: SyncTarget,
        /// Input YAML file path.
        #[arg(short, long)]
        input: PathBuf,
        /// Read the addresses back after writing and confirm byte-for-byte match.
        #[arg(long)]
        verify: bool,
    },

    /// Print a YAML file in a human-readable form (no device needed).
    Show {
        /// Interpret the YAML as a patch (PatchArea); default is system (SystemArea).
        #[arg(long)]
        patch: bool,
        file: PathBuf,
    },

    /// Validate a YAML file against the typed model (no device needed).
    Lint {
        /// Interpret the YAML as a patch (PatchArea); default is system (SystemArea).
        #[arg(long)]
        patch: bool,
        file: PathBuf,
    },

    /// Report differences between two YAML files (no device needed).
    Diff {
        /// Interpret the YAML as patches (PatchArea); default is system.
        #[arg(long)]
        patch: bool,
        a: PathBuf,
        b: PathBuf,
    },

    /// Parse a FloorBoard `.g5l` library file and write one patch slot
    /// as YAML (no device needed).
    ImportG5l {
        /// `.g5l` file to read.
        #[arg(short, long)]
        input: PathBuf,
        /// Slot index inside the library (0-based). Default 0.
        #[arg(short, long, default_value_t = 0)]
        slot: usize,
        /// Output YAML file path. Use `-` for stdout.
        #[arg(short, long)]
        output: PathBuf,
    },
}

#[derive(Debug, clap::Args)]
#[group(required = true, multiple = false)]
pub struct DumpTarget {
    /// Dump the System area (MSB 0x01 and 0x02).
    #[arg(long)]
    pub system: bool,
    /// Dump the live current-patch (TEMP RAM, MSB 0x18).
    #[arg(long = "temp-patch")]
    pub temp_patch: bool,
    /// Dump USER patch slot N (0-296, MSB 0x20 + slot encoding).
    #[arg(long = "user-patch", value_name = "N")]
    pub user_patch: Option<u16>,
}

#[derive(Debug, clap::Args)]
#[group(required = true, multiple = false)]
pub struct SyncTarget {
    /// Push the YAML's System-area fields back to the device.
    #[arg(long)]
    pub system: bool,
    /// Push the YAML to the live current-patch (TEMP RAM).
    #[arg(long = "temp-patch")]
    pub temp_patch: bool,
    /// Overwrite USER patch slot N (0-296).
    #[arg(long = "user-patch", value_name = "N")]
    pub user_patch: Option<u16>,
}
