// SPDX-License-Identifier: GPL-2.0-only
//! Papyrus display PMIC (TI TPS65180/65181 family on this Touch; Amazon's driver calls it "papyrus") on I2C bus 1,
//! address 0x48, reached through /dev/i2c/1 with the I2C_RDWR ioctl. Register names follow Amazon's
//! drivers/regulator/papyrus-regulator.c. Reads only, so far.
use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;

pub const BUS: &str = "/dev/i2c/1";
pub const ADDR: u16 = 0x48;

const I2C_RDWR: u32 = 0x0707;
const I2C_M_RD: u16 = 0x0001;

#[repr(C)]
struct I2cMsg { addr: u16, flags: u16, len: u16, buf: *mut u8 }
#[repr(C)]
struct I2cRdwrIoctlData { msgs: *mut I2cMsg, nmsgs: u32 }

pub struct Pmic { file: File }

/// (reg, name, safe-to-read): STATUS and FAULT may be clear-on-read, so the dump skips them.
pub const REGS: &[(u8, &str, bool)] = &[
    (0x00, "EXT_TEMP", true), (0x01, "ENABLE", true), (0x02, "CONFIG", true), (0x03, "reg03", true),
    (0x04, "DVR (VCOM)", true), (0x05, "INT_TEMP", true), (0x06, "STATUS", false), (0x07, "PRODUCT_ID", true),
    (0x08, "reg08", true), (0x09, "reg09", true), (0x0A, "FAULT", false), (0x0B, "HVINP", true),
    (0x0C, "TMST_CONFIG", true), (0x0D, "TIMING1", true), (0x0E, "TIMING2", true), (0x0F, "PG_GOOD", true),
    (0x10, "PRODUCT_REV", true), (0x11, "TIMING4", true), (0x12, "TIMING5", true), (0x13, "TIMING6", true),
    (0x14, "TIMING7", true), (0x15, "TIMING8", true),
];

impl Pmic {
    pub fn open() -> io::Result<Pmic> {
        Ok(Pmic { file: OpenOptions::new().read(true).write(true).open(BUS)? })
    }

    /// SMBus-style "read byte data": write the register number, repeated-start, read one byte.
    pub fn read_reg(&self, reg: u8) -> io::Result<u8> {
        let mut w = [reg];
        let mut r = [0u8];
        let mut msgs = [
            I2cMsg { addr: ADDR, flags: 0, len: 1, buf: w.as_mut_ptr() },
            I2cMsg { addr: ADDR, flags: I2C_M_RD, len: 1, buf: r.as_mut_ptr() },
        ];
        let mut d = I2cRdwrIoctlData { msgs: msgs.as_mut_ptr(), nmsgs: 2 };
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_RDWR as _, &mut d as *mut _) };
        if rc < 0 { return Err(io::Error::last_os_error()); }
        Ok(r[0])
    }
}

/// Milestone 2a: read-only dump. Reads happen while Amazon's regulator driver still owns the chip; i2c-dev and
/// the kernel client share the adapter lock, so the bus is safe, and no register is written.
pub fn dump() {
    let p = Pmic::open().expect("open /dev/i2c/1");
    for (reg, name, safe) in REGS {
        if !safe { println!("  {:#04x} {:<12} (skipped: may clear on read)", reg, name); continue; }
        match p.read_reg(*reg) {
            Ok(v) => println!("  {:#04x} {:<12} = {:#04x} ({:3})", reg, name, v, v),
            Err(e) => println!("  {:#04x} {:<12} read failed: {}", reg, name, e),
        }
    }
}

// ---- Milestone 2b: rails up / down, following papyrus-regulator.c exactly ----
//   up:   wakeup pin already 1 (the kernel driver keeps it high); ENABLE <- 0xbf (PAPYRUS_ENABLE_REGS);
//         wait for the power-good pin (PWRSTAT) high (driver: up to max_wait ms, 1 ms steps);
//         VCOM: program DVR only if it differs from the wanted value ("programming VCOM excessively degrades
//         persistence"), then VCOM pin <- 1.
//   down: VCOM pin <- 0; ENABLE <- 0x7f (PAPYRUS_STANDBY_REGS); wait until PG_GOOD register reads 0.
// Only allowed with the stock EPDC driver unloaded and our shim loaded (it provides the pins through sysfs).
pub const ENABLE_REGS: u8 = 0xbf;
pub const STANDBY_REGS: u8 = 0x7f;
pub const REG_ENABLE: u8 = 0x01;
pub const REG_DVR: u8 = 0x04;
pub const REG_PG_GOOD: u8 = 0x0f;

fn shim_attr(name: &str) -> io::Result<String> {
    for i in 0..8 {
        let n = std::fs::read_to_string(format!("/sys/class/uio/uio{i}/name")).unwrap_or_default();
        if n.trim() == "epdc" {
            return std::fs::read_to_string(format!("/sys/class/uio/uio{i}/device/{name}")).map(|s| s.trim().to_string());
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "shim not loaded"))
}
fn shim_set(name: &str, v: bool) -> io::Result<()> {
    for i in 0..8 {
        let n = std::fs::read_to_string(format!("/sys/class/uio/uio{i}/name")).unwrap_or_default();
        if n.trim() == "epdc" {
            return std::fs::write(format!("/sys/class/uio/uio{i}/device/{name}"), if v { "1" } else { "0" });
        }
    }
    Err(io::Error::new(io::ErrorKind::NotFound, "shim not loaded"))
}

impl Pmic {
    pub fn write_reg(&self, reg: u8, val: u8) -> io::Result<()> {
        let mut w = [reg, val];
        let mut msgs = [I2cMsg { addr: ADDR, flags: 0, len: 2, buf: w.as_mut_ptr() }];
        let mut d = I2cRdwrIoctlData { msgs: msgs.as_mut_ptr(), nmsgs: 1 };
        let rc = unsafe { libc::ioctl(self.file.as_raw_fd(), I2C_RDWR as _, &mut d as *mut _) };
        if rc < 0 { return Err(io::Error::last_os_error()); }
        Ok(())
    }
}

pub const REG_EXT_TEMP: u8 = 0x00;
pub const REG_TMST_CONFIG: u8 = 0x0c;
const TMST_READ_THERM: u8 = 0x80;
const TMST_CONV_END: u8 = 0x20;

/// Fresh panel temperature in °C: start a conversion and wait for it, as the stock papyrus driver's
/// papyrus_tmst_get_temp does (it runs one every 60 s and the EPDC driver uses the cached value). EXT_TEMP is a
/// signed byte. Guardrail 5: this is the one PMIC register written besides ENABLE; DVR (VCOM) is never touched.
pub fn read_temp(p: &Pmic) -> Result<i32, String> {
    p.write_reg(REG_TMST_CONFIG, TMST_READ_THERM).map_err(|e| format!("TMST_CONFIG write: {e}"))?;
    for _ in 0..25 {
        let v = p.read_reg(REG_TMST_CONFIG).map_err(|e| format!("TMST_CONFIG read: {e}"))?;
        if v & TMST_CONV_END != 0 {
            return Ok(p.read_reg(REG_EXT_TEMP).map_err(|e| format!("EXT_TEMP read: {e}"))? as i8 as i32);
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err("temperature conversion did not finish in 500 ms".into())
}

pub fn require_shim() {
    match shim_attr("clocks") {
        Ok(v) if v == "1" => {}
        other => { eprintln!("refusing: shim not loaded / clocks not on ({other:?}); unload mxc_epdc_fb and insmod epdc_uio.ko first"); std::process::exit(3); }
    }
}

/// Rails up + VCOM, exactly as the stock epdc_powerup/papyrus driver. Returns Err if power-good never came
/// (rails are put back to standby before returning).
pub fn rails_up(p: &Pmic) -> Result<(), String> {
    shim_set("wakeup", true).map_err(|e| e.to_string())?;
    if p.read_reg(0x10).is_err() { return Err("Papyrus not answering on I2C".into()); }
    let dvr_before = p.read_reg(REG_DVR).map_err(|e| e.to_string())?;
    p.write_reg(REG_ENABLE, ENABLE_REGS).map_err(|e| format!("ENABLE write: {e}"))?;
    let mut good = false;
    for i in 0..500u32 {
        if shim_attr("pwrgood").unwrap_or_default() == "1" { println!("power good after {} ms (pin)", i); good = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let pg = p.read_reg(REG_PG_GOOD).unwrap_or(0);
    println!("PG_GOOD reg = {:#04x}", pg);
    if !good { down_inner(p); return Err("power good never came".into()); }
    let dvr = p.read_reg(REG_DVR).unwrap_or(0);
    if dvr != dvr_before { down_inner(p); return Err(format!("DVR changed across power-up ({dvr_before:#04x} -> {dvr:#04x})")); }
    shim_set("vcom", true).map_err(|e| e.to_string())?;
    let u = crate::uio::Uio::open_by_name("epdc").map_err(|e| e.to_string())?;
    let r = u.map_regs().map_err(|e| e.to_string())?;
    r.write32(crate::regs::CTRL_CLEAR, crate::regs::CTRL_CLKGATE);
    r.write32(crate::regs::GPIO, r.read32(crate::regs::GPIO) | crate::regs::GPIO_PWRCOM);
    println!("VCOM on (DVR {:#04x}), EPDC_GPIO = {:#010x}", dvr, r.read32(crate::regs::GPIO));
    Ok(())
}
pub fn rails_down(p: &Pmic) { down_inner(p) }

/// `epdc pmic up [hold_ms]`: rails up, VCOM on, hold, then (unless hold_ms is "stay") rails down again.
pub fn up(hold_ms: Option<&str>) {
    require_shim();
    let p = Pmic::open().expect("open /dev/i2c/1");
    let t0 = std::time::Instant::now();
    println!("wakeup pin = {}  pwrgood pin = {}  PG_GOOD reg = {:#04x}  ENABLE = {:#04x}  DVR = {:#04x}",
        shim_attr("wakeup").unwrap(), shim_attr("pwrgood").unwrap(),
        p.read_reg(REG_PG_GOOD).unwrap(), p.read_reg(REG_ENABLE).unwrap(), p.read_reg(REG_DVR).unwrap());
    // Output GPIOs read back 0 on i.MX5 unless the pad has SION set, so the wakeup/vcom readbacks mean nothing;
    // the chip answering on I2C is the real "awake" test (asleep = WAKEUP low = no I2C). Assert wakeup anyway.
    shim_set("wakeup", true).unwrap();
    if p.read_reg(0x10).is_err() { eprintln!("Papyrus not answering on I2C; refusing"); std::process::exit(4); }
    let dvr_before = p.read_reg(REG_DVR).unwrap();
    p.write_reg(REG_ENABLE, ENABLE_REGS).expect("ENABLE write");
    let mut good = false;
    for i in 0..500u32 {
        if shim_attr("pwrgood").unwrap() == "1" { println!("power good after {} ms (pin)", i); good = true; break; }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let pg = p.read_reg(REG_PG_GOOD).unwrap();
    println!("PG_GOOD reg = {:#04x}, pwrgood pin = {}, {:?}", pg, shim_attr("pwrgood").unwrap(), t0.elapsed());
    if !good { eprintln!("power good never came; going back to standby"); down_inner(&p); std::process::exit(5); }
    // VCOM: the value the kernel left in DVR is the panel's; we do not change it (0xbf = -2.06 V on this unit).
    let dvr = p.read_reg(REG_DVR).unwrap();
    println!("DVR after power-up = {:#04x} (before {:#04x}){}", dvr, dvr_before, if dvr != dvr_before { "  <-- CHANGED, not enabling VCOM" } else { "" });
    if dvr == dvr_before {
        shim_set("vcom", true).unwrap();
        // the board muxes PWRCOM to the EPDC (ALT0), so the stock driver also sets the EPDC_GPIO PWRCOM bit
        let u = crate::uio::Uio::open_by_name("epdc").unwrap();
        let r = u.map_regs().unwrap();
        // stock epdc_powerup: clear the block's internal clock gate first, or register writes are dropped
        r.write32(crate::regs::CTRL_CLEAR, crate::regs::CTRL_CLKGATE);
        r.write32(crate::regs::GPIO, r.read32(crate::regs::GPIO) | crate::regs::GPIO_PWRCOM);
        println!("VCOM pin = {}  EPDC_GPIO = {:#010x}", shim_attr("vcom").unwrap(), r.read32(crate::regs::GPIO));
    }
    match hold_ms {
        Some("stay") => { println!("leaving rails up (run `epdc pmic down`)"); return; }
        Some(ms) => std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap_or(1000))),
        None => std::thread::sleep(std::time::Duration::from_millis(1000)),
    }
    down_inner(&p);
}

pub fn down() {
    // The backstop runs after anything, including an idle driver that left the clocks gated: ungate first,
    // then insist on the shim being there like every other command.
    if let Ok(u) = crate::uio::Uio::open_by_name("epdc") { if !u.clocks_on() { let _ = u.set_clocks(true); } }
    require_shim();
    let p = Pmic::open().expect("open /dev/i2c/1");
    down_inner(&p);
}

fn down_inner(p: &Pmic) {
    if let Ok(u) = crate::uio::Uio::open_by_name("epdc") {
        // The backstop must work from any state, including clocks gated by an idle driver that died.
        if !u.clocks_on() { let _ = u.set_clocks(true); println!("clocks were gated; ungated for the power-down"); }
        if let Ok(r) = u.map_regs() {
            r.write32(crate::regs::GPIO, r.read32(crate::regs::GPIO) & !crate::regs::GPIO_PWRCOM);
            println!("EPDC_GPIO = {:#010x}", r.read32(crate::regs::GPIO));
            // stock epdc_powerdown: re-gate the block's internal clock after the rails are off (done below, after PG=0)
        }
    }
    let _ = shim_set("vcom", false);
    println!("VCOM pin = {}", shim_attr("vcom").unwrap_or_default());
    p.write_reg(REG_ENABLE, STANDBY_REGS).expect("ENABLE standby write");
    for i in 0..500u32 {
        let pg = p.read_reg(REG_PG_GOOD).unwrap_or(0xff);
        if pg == 0 {
            println!("rails down after {} ms (PG_GOOD reg 0, pwrgood pin {})", i, shim_attr("pwrgood").unwrap_or_default());
            if let Ok(u) = crate::uio::Uio::open_by_name("epdc") { if let Ok(r) = u.map_regs() { r.write32(crate::regs::CTRL_SET, crate::regs::CTRL_CLKGATE); println!("CTRL = {:#010x} (clkgate set again)", r.read32(crate::regs::CTRL)); } }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    eprintln!("power-not-good timed out; PG_GOOD = {:#04x}", p.read_reg(REG_PG_GOOD).unwrap_or(0xff));
}
