mod cli;
mod midi;
mod yaml_io;

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use gr55_core::sysex::{Frame, SOX};
use gr55_core::SystemArea;

use crate::cli::{Cli, Command};
use crate::midi::{list_ports, MidiSession};

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let cli = Cli::parse();
    let timeout = Duration::from_millis(cli.timeout_ms);

    match &cli.command {
        Command::Ports => list_ports(),
        Command::Identity => identity(&cli, timeout),
        Command::Dump { target, output } => {
            assert!(target.system, "only --system is wired in v1");
            dump_system(&cli, output, timeout)
        }
        Command::Sync {
            target,
            input,
            verify,
        } => {
            assert!(target.system, "only --system is wired in v1");
            sync_system(&cli, input, *verify, timeout)
        }
        Command::Show { file } => show(file),
        Command::Lint { file } => lint(file),
        Command::Diff { a, b } => diff(a, b),
    }
}

fn session(cli: &Cli) -> Result<MidiSession> {
    let in_substr = cli.input_port.as_deref().unwrap_or(&cli.port);
    let out_substr = cli.output_port.as_deref().unwrap_or(&cli.port);
    MidiSession::open(in_substr, out_substr, cli.device_id)
}

fn identity(cli: &Cli, timeout: Duration) -> Result<()> {
    let mut sess = session(cli)?;
    sess.drain();
    // Universal Identity Request, addressed to all devices.
    let request = [SOX, 0x7E, 0x7F, 0x06, 0x01, 0xF7];
    sess.send_raw(&request)?;

    let buf = sess.collect_raw_for(timeout);
    let Some(reply) = find_identity_reply(&buf) else {
        anyhow::bail!(
            "no Universal Identity Reply received within {} ms; try increasing --timeout-ms or check --port",
            timeout.as_millis()
        );
    };
    print_identity_reply(reply);
    Ok(())
}

fn find_identity_reply(buf: &[u8]) -> Option<&[u8]> {
    // F0 7E xx 06 02 41 ... F7
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == SOX {
            let end = buf[i..].iter().position(|&b| b == 0xF7).map(|p| i + p)?;
            let candidate = &buf[i..=end];
            if candidate.len() >= 7
                && candidate[1] == 0x7E
                && candidate[3] == 0x06
                && candidate[4] == 0x02
                && candidate[5] == 0x41
            {
                return Some(candidate);
            }
            i = end + 1;
        } else {
            i += 1;
        }
    }
    None
}

fn print_identity_reply(reply: &[u8]) {
    // F0 7E [dev] 06 02 41 [family-lo] [family-hi] [number-lo] [number-hi] [4-byte sw-rev] F7
    println!("Identity Reply ({} bytes):", reply.len());
    println!("  Device ID:        0x{:02X}", reply[2]);
    if reply.len() >= 14 {
        println!(
            "  Family code:      {:02X} {:02X}  (LSB first; combined 0x{:04X})",
            reply[6],
            reply[7],
            (u16::from(reply[7]) << 8) | u16::from(reply[6])
        );
        println!("  Family number:    {:02X} {:02X}", reply[8], reply[9]);
        println!(
            "  Software rev:     {:02X} {:02X} {:02X} {:02X}",
            reply[10], reply[11], reply[12], reply[13]
        );
        let expected_family = [0x53, 0x02];
        if reply[6..8] == expected_family {
            println!("  ✓ Family code matches GR-55 (53 02 per FloorBoard).");
        } else {
            println!(
                "  ⚠ Family code does NOT match GR-55 (expected 53 02, got {:02X} {:02X}).",
                reply[6], reply[7]
            );
        }
    } else {
        println!("  (reply shorter than expected; raw: {:02X?})", reply);
    }
}

fn dump_system(cli: &Cli, output: &Path, timeout: Duration) -> Result<()> {
    let mut sess = session(cli)?;
    // The System area spans MSBs 0x01 and 0x02. We issue an RQ1 covering a
    // conservative window at each base; the device responds with whatever
    // exists. Size 0x200 (= 512 bytes) is well above any single page on either
    // MSB so we capture everything in two requests.
    let mut all_frames: Vec<Frame<'static>> = Vec::new();
    for base in [[0x01, 0x00, 0x00, 0x00], [0x02, 0x00, 0x00, 0x00]] {
        let block = sess.read_block(base, 0x200, timeout)?;
        all_frames.extend(block);
    }
    if all_frames.is_empty() {
        anyhow::bail!(
            "no system-area DT1 replies received; try `gr55 identity` first to confirm the port wiring"
        );
    }
    let area = SystemArea::from_frames(&all_frames);
    yaml_io::save_system_area(output, &area)?;
    eprintln!(
        "dumped System area to {} ({} typed fields populated, {} bytes in unknown_bytes)",
        if output == Path::new("-") {
            "<stdout>"
        } else {
            output.file_name().and_then(|n| n.to_str()).unwrap_or("?")
        },
        typed_field_count(&area),
        area.unknown_bytes.len(),
    );
    Ok(())
}

fn sync_system(cli: &Cli, input: &Path, verify: bool, timeout: Duration) -> Result<()> {
    let area = yaml_io::load_system_area(input)?;
    let frames = area
        .to_frames(cli.device_id)
        .context("encoding SystemArea to DT1 frames")?;
    let mut sess = session(cli)?;
    for frame in &frames {
        sess.send_frame(frame)?;
    }
    eprintln!("sent {} DT1 frames", frames.len());
    if verify {
        // Re-dump and compare.
        let mut all: Vec<Frame<'static>> = Vec::new();
        for base in [[0x01, 0x00, 0x00, 0x00], [0x02, 0x00, 0x00, 0x00]] {
            let block = sess.read_block(base, 0x200, timeout)?;
            all.extend(block);
        }
        let back = SystemArea::from_frames(&all);
        if back == area {
            eprintln!("verify: OK (round-trip matches)");
        } else {
            let diffs = field_diffs(&area, &back);
            anyhow::bail!(
                "verify: mismatch ({} differing field(s))\n{}",
                diffs.len(),
                diffs.join("\n")
            );
        }
    }
    Ok(())
}

fn show(file: &Path) -> Result<()> {
    let area = yaml_io::load_system_area(file)?;
    println!("# {}", file.display());
    println!("# typed fields: {}", typed_field_count(&area));
    println!("# unknown bytes: {}", area.unknown_bytes.len());
    println!();
    let yaml = serde_yaml::to_string(&area)?;
    print!("{yaml}");
    Ok(())
}

fn lint(file: &Path) -> Result<()> {
    let area = yaml_io::load_system_area(file)?;
    let _frames = area
        .to_frames(0x10)
        .context("re-encoding to DT1 frames (would fail on a real sync)")?;
    println!(
        "{}: OK ({} typed fields, {} unknown bytes)",
        file.display(),
        typed_field_count(&area),
        area.unknown_bytes.len()
    );
    Ok(())
}

fn diff(a_path: &Path, b_path: &Path) -> Result<()> {
    let a = yaml_io::load_system_area(a_path)?;
    let b = yaml_io::load_system_area(b_path)?;
    if a == b {
        println!(
            "{} and {} are equivalent.",
            a_path.display(),
            b_path.display()
        );
        return Ok(());
    }
    let diffs = field_diffs(&a, &b);
    println!(
        "{} vs {} ({} differing field(s)):",
        a_path.display(),
        b_path.display(),
        diffs.len()
    );
    for line in &diffs {
        println!("  {line}");
    }
    Ok(())
}

/// Number of typed `Option` fields on `SystemArea` that are `Some`.
/// Implemented via YAML serialization since SystemArea fields skip `None`.
fn typed_field_count(area: &SystemArea) -> usize {
    let yaml = serde_yaml::to_string(area).unwrap_or_default();
    // Count top-level keys other than `unknown_bytes`.
    yaml.lines()
        .filter(|line| {
            !line.starts_with(' ')
                && !line.is_empty()
                && !line.starts_with('#')
                && !line.starts_with("unknown_bytes")
                && line.contains(':')
        })
        .count()
}

fn field_diffs(a: &SystemArea, b: &SystemArea) -> Vec<String> {
    let a_yaml = serde_yaml::to_string(a).unwrap_or_default();
    let b_yaml = serde_yaml::to_string(b).unwrap_or_default();
    let a_lines: BTreeSet<&str> = a_yaml.lines().collect();
    let b_lines: BTreeSet<&str> = b_yaml.lines().collect();
    let mut out = Vec::new();
    for line in a_lines.difference(&b_lines) {
        out.push(format!("- {line}"));
    }
    for line in b_lines.difference(&a_lines) {
        out.push(format!("+ {line}"));
    }
    out.sort();
    out
}
