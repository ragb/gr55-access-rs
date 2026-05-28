use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use gr55_core::sysex::{encode_rq1_size, parse_frames_unchecked, Frame};
use midir::{MidiInput, MidiInputConnection, MidiInputPort, MidiOutput, MidiOutputConnection};

/// Wraps an open midir input + output pair against the same GR-55 device.
/// Drains its receive channel before each request to avoid stale bytes.
pub struct MidiSession {
    out_conn: MidiOutputConnection,
    in_rx: Receiver<Vec<u8>>,
    _in_conn: MidiInputConnection<()>,
    pub device_id: u8,
}

impl MidiSession {
    pub fn open(input_substring: &str, output_substring: &str, device_id: u8) -> Result<Self> {
        let midi_in = MidiInput::new("gr55-in").context("opening MidiInput")?;
        let in_port = pick_input_port(&midi_in, input_substring)?;
        let midi_out = MidiOutput::new("gr55-out").context("opening MidiOutput")?;
        let out_port = pick_output_port(&midi_out, output_substring)?;

        let (in_tx, in_rx) = mpsc::channel();
        let in_conn = midi_in
            .connect(
                &in_port,
                "gr55-in-conn",
                move |_stamp, bytes, _| {
                    let _ = in_tx.send(bytes.to_vec());
                },
                (),
            )
            .map_err(|e| anyhow!("MIDI input connect failed: {e}"))?;
        let out_conn = midi_out
            .connect(&out_port, "gr55-out-conn")
            .map_err(|e| anyhow!("MIDI output connect failed: {e}"))?;

        Ok(MidiSession {
            out_conn,
            in_rx,
            _in_conn: in_conn,
            device_id,
        })
    }

    /// Discard any bytes already sitting in the input channel.
    pub fn drain(&self) {
        while self.in_rx.try_recv().is_ok() {}
    }

    pub fn send_raw(&mut self, bytes: &[u8]) -> Result<()> {
        self.out_conn
            .send(bytes)
            .map_err(|e| anyhow!("MIDI send failed: {e}"))
    }

    pub fn send_frame(&mut self, frame: &Frame<'_>) -> Result<()> {
        self.send_raw(&frame.encode())
    }

    /// Send an RQ1 read request and collect any DT1 replies within `timeout`.
    /// Filters out non-GR-55 frames and frames whose address doesn't fall in
    /// the [start..end) range.
    pub fn read_block(
        &mut self,
        address: [u8; 4],
        size: u32,
        timeout: Duration,
    ) -> Result<Vec<Frame<'static>>> {
        self.drain();
        let rq1 = Frame::Rq1 {
            device_id: self.device_id,
            address,
            size,
        };
        let _ = encode_rq1_size(size); // touched: documents the wire encoding
        self.send_frame(&rq1)?;
        Ok(self.collect_for(timeout))
    }

    /// Pull bytes from the channel until `timeout` elapses with no new data,
    /// concatenate them, and parse complete frames.
    pub fn collect_for(&self, timeout: Duration) -> Vec<Frame<'static>> {
        let buf = self.collect_raw_for(timeout);
        parse_frames_unchecked(&buf)
            .filter_map(|r| r.ok())
            .map(|(frame, _)| frame.into_owned())
            .collect()
    }

    /// Pull raw byte chunks from the channel for `timeout`, concatenated.
    /// Used by Universal SysEx (non-GR-55-framed) handlers such as the
    /// identity-reply reader.
    pub fn collect_raw_for(&self, timeout: Duration) -> Vec<u8> {
        let mut buf = Vec::new();
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if let Ok(chunk) = self
                .in_rx
                .recv_timeout(remaining.min(Duration::from_millis(100)))
            {
                buf.extend(chunk);
            }
        }
        buf
    }
}

fn pick_input_port(midi: &MidiInput, needle: &str) -> Result<MidiInputPort> {
    for port in midi.ports() {
        if midi
            .port_name(&port)
            .map(|name| name.contains(needle))
            .unwrap_or(false)
        {
            return Ok(port);
        }
    }
    Err(anyhow!(
        "no MIDI input port whose name contains {needle:?}; run `gr55 ports` to list available ports"
    ))
}

fn pick_output_port(midi: &MidiOutput, needle: &str) -> Result<midir::MidiOutputPort> {
    for port in midi.ports() {
        if midi
            .port_name(&port)
            .map(|name| name.contains(needle))
            .unwrap_or(false)
        {
            return Ok(port);
        }
    }
    Err(anyhow!(
        "no MIDI output port whose name contains {needle:?}; run `gr55 ports` to list available ports"
    ))
}

pub fn list_ports() -> Result<()> {
    let midi_in = MidiInput::new("gr55-list-in").context("opening MidiInput")?;
    let midi_out = MidiOutput::new("gr55-list-out").context("opening MidiOutput")?;

    println!("Input ports:");
    if midi_in.ports().is_empty() {
        println!("  (none)");
    }
    for port in midi_in.ports().iter() {
        let name = midi_in
            .port_name(port)
            .unwrap_or_else(|_| "(unreadable name)".to_string());
        println!("  {name}");
    }

    println!("Output ports:");
    if midi_out.ports().is_empty() {
        println!("  (none)");
    }
    for port in midi_out.ports().iter() {
        let name = midi_out
            .port_name(port)
            .unwrap_or_else(|_| "(unreadable name)".to_string());
        println!("  {name}");
    }
    Ok(())
}
