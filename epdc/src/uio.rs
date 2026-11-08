// SPDX-License-Identifier: GPL-2.0-only
//! Minimal UIO client: open /dev/uioN, mmap its maps by index, wait for / re-enable the interrupt.
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;

pub struct Map {
    pub ptr: *mut u8,
    pub len: usize,
    pub phys: u64,
}

pub struct Uio {
    pub file: File,
    pub index: u32,
}

fn sysfs(index: u32, map: u32, attr: &str) -> io::Result<u64> {
    let p = format!("/sys/class/uio/uio{index}/maps/map{map}/{attr}");
    let s = std::fs::read_to_string(&p)?;
    let s = s.trim();
    let v = if let Some(h) = s.strip_prefix("0x") { u64::from_str_radix(h, 16) } else { s.parse() };
    v.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{p}: {s}: {e}")))
}

impl Uio {
    /// Open the UIO device whose name attribute matches `name` (e.g. "epdc").
    pub fn open_by_name(name: &str) -> io::Result<Uio> {
        for i in 0..8 {
            let p = format!("/sys/class/uio/uio{i}/name");
            if let Ok(n) = std::fs::read_to_string(&p) {
                if n.trim() == name {
                    let file = OpenOptions::new().read(true).write(true).open(format!("/dev/uio{i}"))?;
                    return Ok(Uio { file, index: i });
                }
            }
        }
        Err(io::Error::new(io::ErrorKind::NotFound, format!("no uio device named {name}")))
    }

    /// Ask the shim to gate (false) or ungate (true) the EPDC clocks. Gating with a register map still alive
    /// is a bug; the shim zaps the pages so the next access gets SIGBUS instead of stalling the bus.
    pub fn set_clocks(&self, on: bool) -> io::Result<()> {
        std::fs::write(format!("/sys/class/uio/uio{}/device/clocks", self.index), if on { "1" } else { "0" })
    }

    /// The shim's clock state: /sys/class/uio/uioN/device/clocks is "1" while epdc_axi + epdc_pix are enabled.
    /// A register access with the clocks gated stalls the bus and wedges the CPU, so every path to map0 goes
    /// through `map_regs`, which checks this first. There is deliberately no other way to map the registers.
    pub fn clocks_on(&self) -> bool {
        std::fs::read_to_string(format!("/sys/class/uio/uio{}/device/clocks", self.index))
            .map(|s| s.trim() == "1")
            .unwrap_or(false)
    }

    /// Map the EPDC register window (map0), refusing unless the shim reports the clocks on.
    pub fn map_regs(&self) -> io::Result<Map> {
        if !self.clocks_on() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "EPDC clocks are not reported on by the shim; refusing to map registers (a gated access hangs the CPU)",
            ));
        }
        self.map(0)
    }

    /// mmap map number `map` (UIO addresses maps at offset map * page_size). Use `map_regs` for map0.
    pub fn map(&self, map: u32) -> io::Result<Map> {
        let len = sysfs(self.index, map, "size")? as usize;
        let phys = sysfs(self.index, map, "addr")?;
        let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as i64;
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                self.file.as_raw_fd(),
                (map as i64 * page) as libc::off_t,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        Ok(Map { ptr: ptr as *mut u8, len, phys })
    }

    /// Wait up to `timeout_ms` for the next interrupt (poll on the UIO fd, then read the event count). Returns
    /// Some(count) on an interrupt, None on timeout. The shim's handler has masked the line at the interrupt
    /// controller by then; the caller services the EPDC's IRQ register and calls `irq_enable(true)`.
    pub fn wait_irq_timeout(&self, timeout_ms: i32) -> io::Result<Option<u32>> {
        let mut pfd = libc::pollfd { fd: self.file.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        let n = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
        if n < 0 { return Err(io::Error::last_os_error()); }
        if n == 0 { return Ok(None); }
        let mut b = [0u8; 4];
        (&self.file).read_exact(&mut b)?;
        Ok(Some(u32::from_ne_bytes(b)))
    }

    /// Re-enable (true) or mask (false) the EPDC interrupt line at the interrupt controller.
    pub fn irq_enable(&self, on: bool) -> io::Result<()> {
        (&self.file).write_all(&(on as u32).to_ne_bytes())
    }
}

impl Drop for Map {
    /// Unmap on drop, so a map that is no longer referenced cannot be reached by a stale pointer. The register
    /// map in particular must be gone before the clocks are gated (display::Panel::idle drops it first; the
    /// shim also tears the pages down and faults any later access with SIGBUS).
    fn drop(&mut self) {
        unsafe { libc::munmap(self.ptr as *mut libc::c_void, self.len); }
    }
}

impl Map {
    #[inline]
    pub fn read32(&self, off: usize) -> u32 {
        debug_assert!(off + 4 <= self.len);
        unsafe { std::ptr::read_volatile(self.ptr.add(off) as *const u32) }
    }
    #[inline]
    pub fn write32(&self, off: usize, v: u32) {
        debug_assert!(off + 4 <= self.len);
        unsafe { std::ptr::write_volatile(self.ptr.add(off) as *mut u32, v) }
    }
    pub fn as_slice_mut(&self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.len) }
    }
    /// Copy into the uncached DMA mapping with aligned 32-bit volatile stores. UIO maps the buffers
    /// strongly-ordered; memcpy's NEON/multi-register stores to such memory fault (SIGILL seen 7 Nov 2026).
    pub fn copy_in(&self, off: usize, src: &[u8]) {
        assert!(off % 4 == 0 && off + src.len() <= self.len);
        let words = src.len() / 4;
        for i in 0..words {
            let v = u32::from_le_bytes([src[4 * i], src[4 * i + 1], src[4 * i + 2], src[4 * i + 3]]);
            unsafe { std::ptr::write_volatile(self.ptr.add(off + 4 * i) as *mut u32, v) }
        }
        for i in words * 4..src.len() {
            unsafe { std::ptr::write_volatile(self.ptr.add(off + i), src[i]) }
        }
    }
    /// Fill `len` bytes at `off` with `byte`, 32-bit stores.
    pub fn fill(&self, off: usize, len: usize, byte: u8) {
        assert!(off % 4 == 0 && len % 4 == 0 && off + len <= self.len);
        let v = u32::from_ne_bytes([byte; 4]);
        for i in (0..len).step_by(4) {
            unsafe { std::ptr::write_volatile(self.ptr.add(off + i) as *mut u32, v) }
        }
    }
    pub fn read_bytes(&self, off: usize, n: usize) -> Vec<u8> {
        (0..n).map(|i| unsafe { std::ptr::read_volatile(self.ptr.add(off + i)) }).collect()
    }
}
