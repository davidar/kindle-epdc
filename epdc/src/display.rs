// SPDX-License-Identifier: GPL-2.0-only
//! EPDC bring-up and updates. Milestone 3: the first update, a full-screen INIT (mode 0) clear to white, done the way
//! Amazon's mxc_epdc_fb does it at hardware init (epdc_init_settings + epdc_init_sequence + draw_mode0).
//! Every static register is compared with the golden snapshot taken from the stock driver on 7 Nov 2026 before
//! the rails are allowed up; a mismatch aborts with nothing driven.
use crate::{pmic, regs, uio};

pub const XRES: u32 = 800;
pub const YRES: u32 = 600;
const WBF_DEFAULT: &str = "/mnt/us/kindle-epdc/panel.wbf";

/// Golden register values from epdc-regs-stock.bin (stock driver, idle, 7 Nov 2026 21:11), decoded in README.
pub const GOLDEN: &[(&str, usize, u32)] = &[
    ("FORMAT", regs::FORMAT, 0x0000_0400), ("FIFOCTRL", regs::FIFOCTRL, 0x0064_c864), ("RES", regs::RES, 0x0258_0320),
    ("TCE_CTRL", regs::TCE_CTRL, 0x0004_0002), ("TCE_SDCFG", regs::TCE_SDCFG, 0x0031_4320), ("TCE_GDCFG", regs::TCE_GDCFG, 0x0000_0010),
    ("TCE_HSCAN1", regs::TCE_HSCAN1, 0x000f_000f), ("TCE_HSCAN2", regs::TCE_HSCAN2, 0x00ac_0011), ("TCE_VSCAN", regs::TCE_VSCAN, 0x0012_0404),
    ("TCE_OE", regs::TCE_OE, 0x0a14_0a14), ("TCE_POLARITY", regs::TCE_POLARITY, 0x0000_000e), ("TCE_TIMING1", regs::TCE_TIMING1, 0),
    ("TCE_TIMING2", regs::TCE_TIMING2, 0x01a9_0014), ("TCE_TIMING3", regs::TCE_TIMING3, 0x0000_0011), ("IRQ_MASK", regs::IRQ_MASK, 0x0004_0000),
];

pub struct Waveform { pub data: Vec<u8>, pub trt: Vec<u8>, pub mode_version: u8, pub modes: u8 }

/// Parse the panel's pre-parsed waveform (.wrf = mxcfb_waveform_data_file): 48-byte header, trc+1 temperature
/// range bounds, one checksum byte, then the EPDC's LUT data (what WVADDR points at).
pub fn load_wrf(path: &str) -> Result<Waveform, String> {
    let f = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    if f.len() < 64 { return Err("wrf too short".into()); }
    let trc = f[38] as usize; let mode_version = f[16]; let modes = f[37];
    let trt_entries = trc + 1;
    let data_offs = 48 + trt_entries + 1;
    let trt = f[48..48 + trt_entries].to_vec();
    println!("wrf {}: {} bytes, mode_version {:#04x}, {} modes, {} temp ranges {:?}, data @ {} ({} bytes)",
        path, f.len(), mode_version, modes as u32 + 1, trt_entries, trt, data_offs, f.len() - data_offs);
    if mode_version != 0x04 { return Err(format!("unexpected waveform mode version {mode_version:#04x} (expected 0x04 for this panel)")); }
    if trt_entries != 14 { return Err(format!("unexpected temperature range count {trt_entries}")); }
    Ok(Waveform { data: f[data_offs..].to_vec(), trt, mode_version, modes })
}

/// mxc_epdc_fb_get_temp_index: last range whose lower bound <= temp; DEFAULT_TEMP_INDEX (8) if none.
pub fn temp_index(trt: &[u8], temp_c: i32) -> u32 {
    let mut index: i32 = -1;
    for i in 0..trt.len() - 1 {
        if temp_c >= trt[i] as i32 { index = i as i32; if temp_c < trt[i + 1] as i32 { break; } }
    }
    if index < 0 { 8 } else { index as u32 }
}

/// epdc_init_settings, verbatim order, with the panel timing of E60_V220_WJ.
pub fn init_settings(r: &uio::Map, temp_idx: u32) {
    r.write32(regs::CTRL_SET, regs::CTRL_SFTRST);
    while r.read32(regs::CTRL) & regs::CTRL_CLKGATE == 0 {}
    r.write32(regs::CTRL_CLEAR, regs::CTRL_SFTRST);
    r.write32(regs::CTRL_CLEAR, regs::CTRL_CLKGATE);
    while r.read32(regs::CTRL) & (regs::CTRL_SFTRST | regs::CTRL_CLKGATE) != 0 {}
    // swizzle: NO_SWAP for both (value 0), so nothing to set
    r.write32(regs::FORMAT, regs::FORMAT_TFT_PIXEL_FORMAT_2BIT | regs::FORMAT_BUF_PIXEL_FORMAT_P4N);
    r.write32(regs::FIFOCTRL, (100 << 16) | (200 << 8) | 100);
    r.write32(regs::TEMP, temp_idx);
    r.write32(regs::RES, (YRES << regs::RES_VERTICAL_OFFSET) | XRES);
    r.write32(regs::TCE_CTRL, (4 << regs::TCE_CTRL_VSCAN_HOLDOFF_OFFSET) | regs::TCE_CTRL_PIXELS_PER_SDCLK_4);
    // horizontal: left 17, right 172, hsync 15 (width and line-sync both = hsync_len)
    r.write32(regs::TCE_HSCAN1, (15 << regs::TCE_HSCAN1_LINE_SYNC_WIDTH_OFFSET) | 15);
    r.write32(regs::TCE_HSCAN2, (172 << regs::TCE_HSCAN2_LINE_END_OFFSET) | 17);
    // vertical: upper 4, lower 18, vsync 4
    r.write32(regs::TCE_VSCAN, (18 << regs::TCE_VSCAN_FRAME_END_OFFSET) | (4 << regs::TCE_VSCAN_FRAME_BEGIN_OFFSET) | 4);
    r.write32(regs::TCE_OE, (10 << regs::TCE_OE_SDOED_WIDTH_OFFSET) | (20 << regs::TCE_OE_SDOED_DLY_OFFSET) | (10 << regs::TCE_OE_SDOEZ_WIDTH_OFFSET) | 20);
    r.write32(regs::TCE_TIMING1, 0);
    r.write32(regs::TCE_TIMING2, (425 << regs::TCE_TIMING2_GDCLK_HP_OFFSET) | 20);
    r.write32(regs::TCE_TIMING3, (0 << regs::TCE_TIMING3_GDOE_OFFSET_OFFSET) | 17);
    let num_ce = 1;
    r.write32(regs::TCE_SDCFG, 0x20_0000 | 0x10_0000 | (num_ce << regs::TCE_SDCFG_NUM_CE_OFFSET) | 0x4000 | (XRES / num_ce));
    r.write32(regs::TCE_GDCFG, 0x10); // GDRL
    r.write32(regs::TCE_POLARITY, regs::TCE_POLARITY_SDLE_POL_ACTIVE_HIGH | regs::TCE_POLARITY_SDOE_POL_ACTIVE_HIGH | regs::TCE_POLARITY_GDOE_POL_ACTIVE_HIGH);
    r.write32(regs::IRQ_MASK, regs::IRQ_TCE_UNDERRUN);
}

pub fn check_golden(r: &uio::Map) -> bool {
    let mut ok = true;
    for (name, off, want) in GOLDEN {
        let got = r.read32(*off);
        let m = if got == *want { "ok" } else { ok = false; "MISMATCH" };
        println!("  {:<13} = {:#010x} (golden {:#010x}) {}", name, got, want, m);
    }
    ok
}

/// `epdc white [--check] [wrf]`: load the waveform into DMA, init the EPDC, verify against the golden
/// snapshot; then (unless --check) rails up, one full-screen INIT update with fixed next-pixel 0 (draw_mode0),
/// wait for LUT 0, rails down.
/// The waveform the driver uses: parse the panel's `.wbf` ourselves (crate::wbf), and if a `.wrf` the stock
/// tooling produced sits beside it, refuse to continue unless ours is identical (guardrail 1).
pub fn load_waveform(wbf_path: &str) -> Result<Waveform, String> {
    let d = std::fs::read(wbf_path).map_err(|e| format!("{wbf_path}: {e}"))?;
    let h = crate::wbf::parse_header(&d)?;
    let ours = crate::wbf::convert(&d)?;
    println!("wbf {}: {} bytes, serial {}, type {:#04x}, mode_version {:#04x}, {} modes, {} ranges, {} Hz; converted to {} bytes",
        wbf_path, d.len(), h.serial, h.waveform_type, h.mode_version, h.mc as u32 + 1, h.trc as u32 + 1,
        if h.fpl_rate == 0x85 { 85 } else { h.fpl_rate as u32 }, ours.len());
    let wrf_path = wbf_path.strip_suffix(".wbf").map(|b| format!("{b}.wrf"));
    match wrf_path.as_deref().map(std::fs::read) {
        Some(Ok(theirs)) => {
            if theirs != ours {
                let first = theirs.iter().zip(&ours).position(|(a, b)| a != b).unwrap_or(theirs.len().min(ours.len()));
                return Err(format!("our conversion differs from the cached .wrf (len {} vs {}, first difference at {:#x}); refusing", ours.len(), theirs.len(), first));
            }
            println!("matches the cached .wrf byte for byte ({} bytes)", theirs.len());
        }
        Some(Err(e)) => println!("no cached .wrf to compare against ({e}); using our conversion"),
        None => println!("no .wrf name derivable; using our conversion"),
    }
    let tmp = std::env::temp_dir().join("epdc-ours.wrf");
    std::fs::write(&tmp, &ours).map_err(|e| e.to_string())?;
    load_wrf(tmp.to_str().unwrap())
}

pub struct Panel {
    pub u: uio::Uio,
    pub r: uio::Map,
    pub m_wf: uio::Map,
    pub m_wb: uio::Map,
    pub m_upd: uio::Map,
    pub p: pmic::Pmic,
    pub tidx: u32,
}

/// Everything up to and including the golden check: waveform into DMA, working buffer zeroed, EPDC programmed
/// in the stock order, static registers verified. Rails stay off. Exits the process on any refusal.
pub fn bring_up(wf_path: Option<&str>) -> Panel {
    pmic::require_shim();
    let path = wf_path.unwrap_or(WBF_DEFAULT);
    let wf = match if path.ends_with(".wbf") { load_waveform(path) } else { load_wrf(path) } {
        Ok(w) => w, Err(e) => { eprintln!("{e}"); std::process::exit(6); }
    };
    let u = uio::Uio::open_by_name("epdc").expect("uio epdc");
    let r = u.map_regs().expect("regs");
    let m_wf = u.map(1).expect("map1 waveform");
    let m_wb = u.map(2).expect("map2 working buffer");
    let m_upd = u.map(3).expect("map3 update buffer");
    let wb_size = (XRES * YRES * 2) as usize;
    let upd_size = (XRES * YRES) as usize;
    if wf.data.len() > m_wf.len || wb_size > m_wb.len || upd_size > m_upd.len {
        eprintln!("DMA maps too small: wf {} / {}, wb {} / {}, upd {} / {}", wf.data.len(), m_wf.len, wb_size, m_wb.len, upd_size, m_upd.len);
        std::process::exit(7);
    }
    m_wf.copy_in(0, &wf.data);
    m_wb.fill(0, wb_size, 0);
    m_upd.fill(0, upd_size, 0xff);
    let back = m_wf.read_bytes(0, 8);
    let tail = m_wf.read_bytes(wf.data.len() - 8, 8);
    println!("waveform in DMA @ {:#010x}: head {:02x?} tail {:02x?} (file head {:02x?} tail {:02x?})", m_wf.phys, back, tail, &wf.data[..8], &wf.data[wf.data.len() - 8..]);
    if back != wf.data[..8] || tail != wf.data[wf.data.len() - 8..] { eprintln!("DMA copy mismatch"); std::process::exit(7); }

    let p = pmic::Pmic::open().expect("i2c");
    let temp_c = p.read_reg(0x00).expect("EXT_TEMP") as i32;
    let tidx = temp_index(&wf.trt, temp_c);
    println!("panel temperature {} C -> temp index {}", temp_c, tidx);

    println!("== init_settings");
    init_settings(&r, tidx);
    r.write32(regs::WVADDR, m_wf.phys as u32);
    r.write32(regs::WB_ADDR, m_wb.phys as u32);
    println!("WVADDR {:#010x} WB_ADDR {:#010x} TEMP {} CTRL {:#010x}", r.read32(regs::WVADDR), r.read32(regs::WB_ADDR), r.read32(regs::TEMP), r.read32(regs::CTRL));
    println!("== golden check");
    if !check_golden(&r) { eprintln!("register mismatch against the stock snapshot: refusing to power the panel"); std::process::exit(8); }
    Panel { u, r, m_wf, m_wb, m_upd, p, tidx }
}

fn install_panic_hook() {
    // Backstop: whatever kills this process from here on (panic, with panic=abort the hook still runs), the
    // rails come down first. The session scripts also run `epdc pmic down` unconditionally afterwards.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {info}; powering the display down before aborting");
        if let Ok(p) = pmic::Pmic::open() { pmic::rails_down(&p); }
    }));
}

/// A rectangle in panel-native coordinates (800 wide, 600 tall, origin top-left as the EPDC scans it).
#[derive(Clone, Copy, Debug)]
pub struct Rect { pub x: u32, pub y: u32, pub w: u32, pub h: u32 }

impl Rect {
    pub const FULL: Rect = Rect { x: 0, y: 0, w: XRES, h: YRES };
    /// The EPDC v1 has no stride register: the update buffer is the region's pixels packed row by row, so the
    /// stock driver sizes and positions every region in multiples of 8 pixels (its PxP output is 8x8 blocks,
    /// `pxp_upd_region` in mxc_epdc_fb.c). Same rule here until a run shows the controller needs less.
    pub fn check(&self) -> Result<(), String> {
        if self.w == 0 || self.h == 0 { return Err(format!("{self:?}: empty")); }
        if self.x + self.w > XRES || self.y + self.h > YRES { return Err(format!("{self:?}: outside the {XRES}x{YRES} panel")); }
        if [self.x, self.y, self.w, self.h].iter().any(|v| v % 8 != 0) { return Err(format!("{self:?}: x, y, w, h must be multiples of 8")); }
        Ok(())
    }
}

/// Submit one update covering the whole panel from map3 and wait for its LUT. `fixed_np`: Some(v) uses the
/// EPDC's fixed-next-pixel test mode (the stock INIT update uses Some(0)); None uses the buffer's pixels.
/// Returns true if the LUT completed within the timeout.
pub fn update_full(pn: &Panel, wfm_mode: u32, fixed_np: Option<u8>, timeout_ms: u64) -> bool {
    update_rect(pn, pn.m_upd.phys as u32, Rect::FULL, wfm_mode, true, fixed_np, timeout_ms).is_some()
}

/// Submit one update of `rect` from the packed region buffer at physical address `buf_phys` (rect.w bytes per
/// row, rect.h rows) and wait for its LUT. `full`: UPDATE_MODE_FULL drives every pixel in the rectangle;
/// otherwise the EPDC compares against the working buffer and drives only the pixels that change (PARTIAL).
/// Returns the time the LUT took, or None if it was still active at the timeout.
pub fn update_rect(pn: &Panel, buf_phys: u32, rect: Rect, wfm_mode: u32, full: bool, fixed_np: Option<u8>, timeout_ms: u64) -> Option<std::time::Duration> {
    let r = &pn.r;
    if let Err(e) = rect.check() { eprintln!("refusing update: {e}"); return None; }
    let lut = {
        let nl = r.read32(regs::STATUS_NEXTLUT);
        if nl & regs::STATUS_NEXTLUT_NEXT_LUT_VALID == 0 { eprintln!("no LUT available (STATUS_NEXTLUT {:#x})", nl); return None; }
        nl & regs::STATUS_NEXTLUT_NEXT_LUT_MASK
    };
    r.write32(regs::IRQ_MASK_SET, regs::IRQ_WB_CMPLT | (1 << lut));
    r.write32(regs::UPD_ADDR, buf_phys);
    r.write32(regs::UPD_CORD, (rect.y << regs::UPD_CORD_YCORD_OFFSET) | rect.x);
    r.write32(regs::UPD_SIZE, (rect.h << regs::UPD_SIZE_HEIGHT_OFFSET) | rect.w);
    r.write32(regs::TEMP, pn.tidx);
    let mut ctrl = (lut << regs::UPD_CTRL_LUT_SEL_OFFSET) | (wfm_mode << regs::UPD_CTRL_WAVEFORM_MODE_OFFSET);
    if full { ctrl |= regs::UPD_CTRL_UPDATE_MODE_FULL; }
    match fixed_np {
        Some(v) => { r.write32(regs::UPD_FIXED, regs::UPD_FIXED_FIXNP_EN | ((v as u32) << 8)); ctrl |= regs::UPD_CTRL_USE_FIXED; }
        None => r.write32(regs::UPD_FIXED, 0),
    }
    println!("update: {:?} buf {:#010x} LUT {} mode {} {} fixed_np {:?} -> UPD_CTRL {:#010x}",
        rect, buf_phys, lut, wfm_mode, if full { "FULL" } else { "PARTIAL" }, fixed_np, ctrl);
    let t0 = std::time::Instant::now();
    r.write32(regs::UPD_CTRL, ctrl);
    let mut done = false;
    let mut next_report = 0u128;
    while t0.elapsed().as_millis() < timeout_ms as u128 {
        std::thread::sleep(std::time::Duration::from_millis(2));
        let luts = r.read32(regs::STATUS_LUTS);
        let ms = t0.elapsed().as_millis();
        if ms >= next_report { println!("  t+{:>5} ms STATUS_LUTS {:#06x} IRQ {:#010x} STATUS {:#x} COL {:#x}", ms, luts, r.read32(regs::IRQ), r.read32(regs::STATUS), r.read32(regs::STATUS_COL)); next_report = ms + 250; }
        if luts & (1 << lut) == 0 { done = true; break; }
    }
    let took = t0.elapsed();
    println!("LUT {} {} after {:?}; IRQ {:#010x} COL {:#x}", lut, if done { "complete" } else { "STILL ACTIVE" }, took, r.read32(regs::IRQ), r.read32(regs::STATUS_COL));
    r.write32(regs::IRQ_CLEAR, regs::IRQ_WB_CMPLT | regs::IRQ_LUT_COL | (1 << lut));
    r.write32(regs::IRQ_MASK_CLEAR, regs::IRQ_WB_CMPLT | (1 << lut));
    if !done { std::thread::sleep(std::time::Duration::from_millis(500)); return None; }
    Some(took)
}

/// Taking the panel over from an unknown state (whatever the stock driver last drew): the stock INIT, then a
/// full-screen GC16 to white. The INIT clears the panel but leaves the working buffer at the fixed next pixel,
/// 0 (black), as the stock driver does at boot; stock follows it with a FULL GC16 of the splash, which rewrites
/// every working-buffer pixel. Without that second step, DU's black-to-black entry and PARTIAL mode's
/// unchanged-pixel skip both leave glyphs undriven on a white panel (8 Nov 2026, first m5 run). The white
/// buffer is the first XRES*YRES bytes of map3, which bring_up fills with 0xff.
pub const WHITE_END: usize = ((XRES * YRES) as usize + 4095) & !4095;
pub fn take_over(pn: &Panel) {
    println!("== INIT (unknown panel state)");
    update_full(pn, regs::WFM_INIT, Some(0), 6000);
    println!("== GC16 full-screen white (working buffer to white)");
    pn.m_upd.fill(0, (XRES * YRES) as usize, 0xff);
    update_full(pn, regs::WFM_GC16, None, 8000);
}

/// `epdc white [--check] [wbf]`: bring up, then (unless --check) rails up, the stock INIT update, rails down.
pub fn white(check_only: bool, wf_path: Option<&str>) {
    let pn = bring_up(wf_path);
    if check_only { println!("check only: not powering the rails, no update submitted"); return; }
    install_panic_hook();
    println!("== rails up");
    if let Err(e) = pmic::rails_up(&pn.p) { eprintln!("rails: {e}"); std::process::exit(9); }
    println!("== draw_mode0: full-screen INIT (mode 0, fixed next pixel 0)");
    update_full(&pn, regs::WFM_INIT, Some(0), 6000);
    println!("== rails down");
    pmic::rails_down(&pn.p);
}

/// Read a binary PGM (P5, maxval 255): (width, height, pixels). 0 = black, 255 = white; the EPDC uses the top
/// four bits of each byte in P4N format, the same Y8 layout the stock driver's PxP produces.
pub fn read_pgm(path: &str) -> Result<(u32, u32, Vec<u8>), String> {
    let d = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 && i < d.len() {
        while i < d.len() && (d[i] as char).is_ascii_whitespace() { i += 1; }
        if i < d.len() && d[i] == b'#' { while i < d.len() && d[i] != b'\n' { i += 1; } continue; }
        let s = i;
        while i < d.len() && !(d[i] as char).is_ascii_whitespace() { i += 1; }
        fields.push(String::from_utf8_lossy(&d[s..i]).to_string());
    }
    i += 1; // the single whitespace after maxval
    if fields.len() < 4 || fields[0] != "P5" { return Err(format!("{path}: not a binary PGM (P5)")); }
    let (w, h, maxv): (u32, u32, u32) = (fields[1].parse().map_err(|_| "bad width")?, fields[2].parse().map_err(|_| "bad height")?, fields[3].parse().map_err(|_| "bad maxval")?);
    if maxv != 255 { return Err(format!("{path}: need maxval 255, got {maxv}")); }
    let n = (w * h) as usize;
    if d.len() < i + n { return Err(format!("{path}: PGM truncated")); }
    Ok((w, h, d[i..i + n].to_vec()))
}

/// Copy `px` into the update map at `off` and read the head back (DMA sanity).
fn load_into(m_upd: &uio::Map, off: usize, px: &[u8], what: &str) -> Result<(), String> {
    if off + px.len() > m_upd.len { return Err(format!("{what}: {} bytes at {off:#x} do not fit the update map ({:#x})", px.len(), m_upd.len)); }
    m_upd.copy_in(off, px);
    let n = px.len().min(8);
    let back = m_upd.read_bytes(off, n);
    println!("{what}: {} bytes at map3+{off:#x}, first bytes {:02x?} (dma {:02x?})", px.len(), &px[..n], back);
    if back != px[..n] { return Err("DMA copy mismatch".into()); }
    Ok(())
}

/// `epdc show <image.pgm> [mode] [wbf]`: bring up, rails up, one full-screen update of the image with the given
/// waveform mode (default GC16 = 2), rails down. The first real pixels from userspace.
pub fn show(image: &str, mode: u32, wf_path: Option<&str>, init_first: bool) {
    let pn = bring_up(wf_path);
    let (w, h, px) = match read_pgm(image) { Ok(v) => v, Err(e) => { eprintln!("{e}"); std::process::exit(10); } };
    if w != XRES || h != YRES { eprintln!("{image}: need {XRES}x{YRES}, got {w}x{h}"); std::process::exit(10); }
    install_panic_hook();
    println!("== rails up");
    if let Err(e) = pmic::rails_up(&pn.p) { eprintln!("rails: {e}"); std::process::exit(9); }
    // A GC16 straight onto an unknown panel drives every pixel from the wrong "old" value and the previous
    // image ghosts through (seen 7 Nov 2026 with the splash); see take_over.
    if init_first { take_over(&pn); }
    if let Err(e) = load_into(&pn.m_upd, 0, &px, image) { eprintln!("{e}"); pmic::rails_down(&pn.p); std::process::exit(10); }
    println!("== full-screen update, waveform mode {}", mode);
    update_full(&pn, mode, None, 8000);
    println!("== rails down");
    pmic::rails_down(&pn.p);
}

/// One step of `epdc blit`: a PGM whose size is the rectangle, placed at (x, y) in panel coordinates.
#[derive(Clone, Debug)]
pub struct BlitOp { pub pgm: String, pub x: u32, pub y: u32, pub mode: Option<u32>, pub full: bool }

/// What the stock driver's WAVEFORM_MODE_AUTO does with the PxP histogram of the region: only black and white
/// present -> the two-level DU; any other grey -> GL16 (the framework's page-turn choice; GC16 is the periodic
/// full flash). Antialiased text through DU comes out ragged (8 Nov 2026), so the choice matters per region.
pub fn auto_mode(px: &[u8]) -> (u32, usize) {
    let mut seen = [false; 16];
    for &p in px { seen[(p >> 4) as usize] = true; }
    let levels = seen.iter().filter(|&&b| b).count();
    let bw_only = seen.iter().enumerate().all(|(i, &b)| !b || i == 0 || i == 15);
    (if bw_only { regs::WFM_DU } else { regs::WFM_GL16 }, levels)
}

/// `epdc blit [--no-init] [--wbf path] <pgm>@<x>,<y>[:<mode>|:auto][:full|partial] ...`: bring up, rails up,
/// take over (unless --no-init), then each rectangle update in turn, from a region-sized buffer at the next 4 KiB slot
/// of map3, timing each LUT. Milestone 5: partial updates, the thing a text display is built from.
pub fn blit(ops: &[BlitOp], wf_path: Option<&str>, init_first: bool) {
    // Validate everything before the panel is touched: every PGM readable, every rectangle legal.
    let mut loaded = Vec::new();
    for op in ops {
        let (w, h, px) = match read_pgm(&op.pgm) { Ok(v) => v, Err(e) => { eprintln!("{e}"); std::process::exit(10); } };
        let rect = Rect { x: op.x, y: op.y, w, h };
        if let Err(e) = rect.check() { eprintln!("{}: {e}", op.pgm); std::process::exit(10); }
        loaded.push((rect, px));
    }
    let pn = bring_up(wf_path);
    install_panic_hook();
    println!("== rails up");
    if let Err(e) = pmic::rails_up(&pn.p) { eprintln!("rails: {e}"); std::process::exit(9); }
    if init_first { take_over(&pn); }
    let mut results = Vec::new();
    // Region buffers go in successive 4 KiB-aligned slots after the full-screen white buffer, wrapping when the
    // map is used up: a buffer is never rewritten right after its update, even though update_rect waits for
    // each LUT before returning.
    let mut next_off = WHITE_END;
    for (op, (rect, px)) in ops.iter().zip(&loaded) {
        if next_off + px.len() > pn.m_upd.len { next_off = WHITE_END; }
        let off = next_off;
        next_off += (px.len() + 4095) & !4095;
        if let Err(e) = load_into(&pn.m_upd, off, px, &op.pgm) { eprintln!("{e}"); break; }
        let mode = match op.mode {
            Some(m) => m,
            None => { let (m, levels) = auto_mode(px); println!("auto: {} grey levels in the region -> waveform mode {}", levels, m); m }
        };
        println!("== blit {} -> {:?} mode {} {}", op.pgm, rect, mode, if op.full { "FULL" } else { "PARTIAL" });
        let took = update_rect(&pn, pn.m_upd.phys as u32 + off as u32, *rect, mode, op.full, None, 8000);
        results.push((op.clone(), *rect, mode, took));
        if took.is_none() { eprintln!("update did not complete; stopping the sequence"); break; }
    }
    println!("== rails down");
    pmic::rails_down(&pn.p);
    println!("== summary");
    for (op, rect, mode, took) in &results {
        println!("  {:<40} {:>3},{:>3} {:>3}x{:>3} mode {}{} {:<7} {}", op.pgm, rect.x, rect.y, rect.w, rect.h, mode, if op.mode.is_none() { " (auto)" } else { "" },
            if op.full { "FULL" } else { "PARTIAL" }, took.map(|d| format!("{} ms", d.as_millis())).unwrap_or("TIMEOUT".into()));
    }
}
