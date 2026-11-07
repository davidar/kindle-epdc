// SPDX-License-Identifier: GPL-2.0-only
//! E Ink `.wbf` (compressed waveform file, as stored in the panel's SPI flash) to `.wrf` (the i.MX50 EPDC's
//! in-memory layout, what Amazon's `eu extract` produces and the stock driver loads) converter.
//!
//! This is a Rust port of inkwave (github.com/fread-ink/inkwave, GPLv2; Copyright 2018, 2021 Marc Juul,
//! Copyright 2005-2017 Amazon Technologies, Inc., Copyright 2004-2013 Freescale Semiconductor, Inc.), which is
//! where the format and the conversion algorithm come from. Verified byte for byte against the `.wrf` the Kindle
//! itself generated with `eu` (test `wrf_matches_kindle`).
//!
//! .wbf layout: 48-byte header; temperature table (trc+2 bytes + 1 checksum byte); extra-waveform-info string at
//! `xwia` (len byte, bytes, checksum); mode table (mc+1 pointers: 24-bit LE address + checksum byte); per mode a
//! temperature-range table (trc+1 pointers) each pointing at a run-length-coded waveform block.
//! .wrf layout: same header; trc+2 temperature bytes; then, relative to offset 63 (header + temperatures):
//! mode table (mc+1 × 8 bytes: u32 LE offset + 4 zero bytes); per mode a range table (trc+1 × 8 bytes) followed by
//! per range an 8-byte block (u16 big-endian phase×4 count, 6 zero bytes) and the unpacked waveform: each packed
//! byte expands to 4 bytes holding its four 2-bit drive values, repeated `count` times.

pub const HEADER_LEN: usize = 48;
pub const WRF_TABLE_BASE: usize = 63; // "MYSTERIOUS_OFFSET": header + 15 temperature bytes for trc=13 (48 + trc + 2)

#[derive(Debug, Clone)]
pub struct Header {
    pub checksum: u32,
    pub filesize: u32,
    pub serial: u32,
    pub run_type: u8,
    pub fpl_platform: u8,
    pub fpl_lot: u16,
    pub mode_version: u8,
    pub waveform_version: u8,
    pub waveform_subversion: u8,
    pub waveform_type: u8,
    pub fpl_size: u8,
    pub mfg_code: u8,
    pub tuning_bias: u8,
    pub fpl_rate: u8,
    pub xwia: u32,
    pub wmta: u32,
    pub luts: u8,
    pub mc: u8,
    pub trc: u8,
}

fn u24(b: &[u8]) -> u32 { b[0] as u32 | (b[1] as u32) << 8 | (b[2] as u32) << 16 }
fn u32le(b: &[u8]) -> u32 { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) }

pub fn parse_header(d: &[u8]) -> Result<Header, String> {
    if d.len() < HEADER_LEN { return Err("file shorter than the 48-byte header".into()); }
    Ok(Header {
        checksum: u32le(&d[0..]), filesize: u32le(&d[4..]), serial: u32le(&d[8..]),
        run_type: d[12], fpl_platform: d[13], fpl_lot: u16::from_le_bytes([d[14], d[15]]),
        mode_version: d[16], waveform_version: d[17], waveform_subversion: d[18], waveform_type: d[19],
        fpl_size: d[20], mfg_code: d[21], tuning_bias: d[22], fpl_rate: d[23],
        xwia: u24(&d[28..]), wmta: u24(&d[32..]), luts: d[36], mc: d[37], trc: d[38],
    })
}

/// zlib-style CRC-32 over four zero bytes followed by file[4..filesize] (the checksum field itself is skipped).
pub fn crc32_wbf(d: &[u8], filesize: usize) -> u32 {
    let mut table = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 { c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 }; }
        table[n as usize] = c;
    }
    let mut c = 0xffff_ffffu32;
    for _ in 0..4 { c = table[(c & 0xff) as usize] ^ (c >> 8); }
    for &b in &d[4..filesize] { c = table[((c ^ b as u32) & 0xff) as usize] ^ (c >> 8); }
    c ^ 0xffff_ffff
}

pub fn bits_per_pixel(h: &Header) -> u8 { if h.luts & 0xc == 4 { 5 } else { 4 } }

/// Expand one run-length-coded waveform block (bytes `[start, end)` of the file, last two bytes ignored).
/// Returns (unpacked bytes, state count = phases*4... as inkwave counts it).
fn unpack_waveform(d: &[u8], start: usize, end: usize) -> Result<(Vec<u8>, u16), String> {
    if end < start + 2 { return Err(format!("waveform block at {start:#x} too short")); }
    let w = &d[start..end];
    let len = w.len() - 2;
    if len == 0 { return Err(format!("empty waveform block at {start:#x}")); }
    let mut out = Vec::new();
    let mut state_count: u32 = 0;
    let mut fc_active = false;
    let mut i = 0usize;
    while i < len - 1 {
        if w[i] == 0xfc { fc_active = !fc_active; i += 1; continue; }
        let s = w[i];
        let count: u32 = if fc_active { i += 1; 1 } else if i >= len - 1 { i += 2; 1 } else { let c = w[i + 1] as u32 + 1; i += 2; c };
        state_count += count * 4;
        let u = [s & 3, (s >> 2) & 3, (s >> 4) & 3, (s >> 6) & 3];
        for _ in 0..count { out.extend_from_slice(&u); }
    }
    Ok((out, state_count as u16))
}

/// Convert a `.wbf` image to the `.wrf` the EPDC consumes. Every checksum in the file is verified.
pub fn convert(d: &[u8]) -> Result<Vec<u8>, String> {
    let h = parse_header(d)?;
    if h.filesize as usize != d.len() { return Err(format!("header says {} bytes, file is {}", h.filesize, d.len())); }
    let crc = crc32_wbf(d, d.len());
    if crc != h.checksum { return Err(format!("file checksum {crc:#010x} != header {:#010x}", h.checksum)); }
    if bits_per_pixel(&h) != 4 { return Err("5 bits per pixel waveforms are not handled".into()); }
    let mc = h.mc as usize + 1;
    let trc = h.trc as usize + 1;

    // temperature table: trc+1 ranges = trc+2 temperatures, then a checksum byte
    let tt = &d[HEADER_LEN..HEADER_LEN + trc + 1];
    let tsum = tt.iter().fold(0u8, |a, &b| a.wrapping_add(b));
    if tsum != d[HEADER_LEN + trc + 1] { return Err("temperature table checksum mismatch".into()); }

    // xwia
    let modes_start = if h.xwia != 0 {
        let x = h.xwia as usize;
        let n = d[x] as usize;
        let sum = d[x + 1..x + 1 + n].iter().fold(n as u8, |a, &b| a.wrapping_add(b));
        if sum != d[x + 1 + n] { return Err("xwia checksum mismatch".into()); }
        x + 1 + n + 1
    } else { return Err("no xwia; mode table location unknown".into()); };

    // mode pointers and, per mode, range pointers; checksums on each
    let ptr = |at: usize| -> Result<u32, String> {
        let p = &d[at..at + 4];
        let sum = p[0].wrapping_add(p[1]).wrapping_add(p[2]);
        if sum != p[3] { return Err(format!("pointer checksum mismatch at {at:#x}")); }
        Ok(u24(p))
    };
    let mut mode_ptrs = Vec::with_capacity(mc);
    for m in 0..mc { mode_ptrs.push(ptr(modes_start + 4 * m)? as usize); }
    let mut range_ptrs: Vec<Vec<usize>> = Vec::with_capacity(mc);
    let mut all_addrs: Vec<usize> = Vec::new();
    for &mp in &mode_ptrs {
        let mut v = Vec::with_capacity(trc);
        for r in 0..trc { let a = ptr(mp + 4 * r)? as usize; v.push(a); all_addrs.push(a); }
        range_ptrs.push(v);
    }
    all_addrs.push(d.len());
    all_addrs.sort_unstable();
    all_addrs.dedup();
    let next_after = |a: usize| -> usize { let i = all_addrs.binary_search(&a).unwrap(); all_addrs[i + 1] };

    // write the .wrf
    let mut out: Vec<u8> = Vec::with_capacity(2 << 20);
    out.extend_from_slice(&d[..HEADER_LEN]);
    out.extend_from_slice(tt); // trc+1 (= h.trc + 2) temperature bytes
    debug_assert_eq!(out.len(), HEADER_LEN + h.trc as usize + 2);
    let mode_table_at = out.len();
    out.resize(mode_table_at + 8 * mc, 0);
    let mut mode_offsets = Vec::with_capacity(mc);
    for m in 0..mc {
        mode_offsets.push((out.len() - WRF_TABLE_BASE) as u32);
        let range_table_at = out.len();
        out.resize(range_table_at + 8 * trc, 0);
        let mut range_offsets = Vec::with_capacity(trc);
        for r in 0..trc {
            let a = range_ptrs[m][r];
            let (data, state_count) = unpack_waveform(d, a, next_after(a))?;
            range_offsets.push((out.len() - WRF_TABLE_BASE) as u32);
            let block = out.len();
            out.resize(block + 8, 0);
            out[block..block + 2].copy_from_slice(&state_count.to_be_bytes());
            out.extend_from_slice(&data);
        }
        for (r, off) in range_offsets.iter().enumerate() {
            out[range_table_at + 8 * r..range_table_at + 8 * r + 4].copy_from_slice(&off.to_le_bytes());
        }
    }
    for (m, off) in mode_offsets.iter().enumerate() {
        out[mode_table_at + 8 * m..mode_table_at + 8 * m + 4].copy_from_slice(&off.to_le_bytes());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    const WBF: &str = "../panel/06_05_007a_3c_158e21_03_3c_00000986_85_04.wbf"; // the author's unit; see fetch-panel.sh
    const WRF: &str = "../panel/06_05_007a_3c_158e21_03_3c_00000986_85_04.wrf";

    // Both tests need the panel files fetched into ../panel/ (scripts/fetch-panel.sh); they skip otherwise.
    #[test]
    fn header_reads_this_panel() {
        let Ok(d) = std::fs::read(WBF) else { eprintln!("skipped: {WBF} not present"); return };
        let h = parse_header(&d).unwrap();
        assert_eq!(h.filesize as usize, d.len());
        assert_eq!(h.mode_version, 4);
        assert_eq!(h.mc, 5);
        assert_eq!(h.trc, 13);
        assert_eq!(h.fpl_rate, 0x85);
        assert_eq!(h.waveform_type, 0x15); // WJ
        assert_eq!(bits_per_pixel(&h), 4);
        assert_eq!(crc32_wbf(&d, d.len()), h.checksum);
    }

    #[test]
    fn wrf_matches_kindle() {
        let (Ok(d), Ok(want)) = (std::fs::read(WBF), std::fs::read(WRF)) else { eprintln!("skipped: panel files not present"); return };
        let got = convert(&d).unwrap();
        assert_eq!(got.len(), want.len(), "length differs");
        if got != want {
            let first = got.iter().zip(&want).position(|(a, b)| a != b).unwrap();
            panic!("first difference at byte {first:#x}: ours {:02x?} kindle {:02x?}", &got[first..first + 8], &want[first..first + 8]);
        }
    }
}
