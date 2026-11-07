# kindle-epdc

A userspace driver for the e-paper display controller (EPDC) of the i.MX50 in the 2011 Kindle Touch, with the
kernel's part reduced to a 250-line UIO shim. The driver is Rust, statically linked, and runs on the stock 2.6.31
kernel; the same code should run on a mainline kernel with a device-tree node in place of the shim.

Status (7 November 2026): the driver initialises the controller exactly as Amazon's driver does, parses the panel's
own waveform from the `.wbf` in the panel's flash, powers the display through the Papyrus PMIC's sequencer, and
draws images: a full-screen GC16 update of an 8-bit PGM, text and a grey ramp rendered correctly on a real panel.
It is a test harness so far, not a display server. [docs/hardware.md](docs/hardware.md) has every hardware fact
with its source; [docs/kernel-tree.md](docs/kernel-tree.md) explains how the shim is built against the stock kernel.

## Why

The EPDC does the hard part in hardware: it reads a pixel buffer and a waveform lookup table and generates the
panel's source and gate timing itself. What the kernel driver adds is clocks and pin mux, power sequencing over
I2C, a few physically contiguous buffers, about forty registers per update, and three interrupts. None of that
needs to be in the kernel. Freescale's driver was written in 2009 against a Freescale kernel fork, and it is the
reason nobody has shipped a third-party OS for these Kindles: the display driver is welded to a kernel nothing
modern runs on. A driver that owns the controller from userspace is independent of the kernel version.

As far as I can find, nobody had done this for the EPDC. The neighbours that showed it was possible: `waved` and `swtcon` on the
reMarkable 2 (userspace software TCON, a harder case with no EPDC), IT8951 panels driven over spidev, EPDiy on bare
metal, and the out-of-tree i.MX6 EPDC DRM driver.

## Layout

- `shim/`: `epdc_uio.c`, the kernel side. Binds to the `mxc_epdc_fb` platform device once the stock module is
  unloaded, enables the EPDC clocks, allocates three 2 MiB coherent buffers, and exposes registers, buffers and the
  interrupt through UIO, plus the Papyrus control pins and the clock state through sysfs. `build.sh` builds it
  against the stock kernel tree ([docs/kernel-tree.md](docs/kernel-tree.md)). GPL-2.0.
- `epdc/`: the driver (GPL-2.0-only). `uio.rs` UIO client whose only way to map the register window checks the
  shim's clock state; `regs.rs` the EPDC register map; `pmic.rs` the Papyrus over `/dev/i2c/1` and the rails
  sequence; `wbf.rs` the `.wbf` → `.wrf` waveform converter, with tests; `display.rs` controller init, the golden
  check and updates; `main.rs` the command line.
- `scripts/`: `hold.sh` and `release.sh` keep the Kindle from suspending during a session; `m1-probe.sh`,
  `m2-rails.sh`, `m3-init-white.sh` are the milestone runs, each of which swaps the stock driver out, runs one
  step, and restores stock through the boot job.
- `epdc-regs-stock.bin`: the EPDC register window as Amazon's driver leaves it, the reference every init is
  compared against.

Not in the repository: the panel's waveform files (E Ink's data; read them from your own device) and the device
logs.

## Command line

```
epdc probe                      map registers and buffers, read the EPDC version and timing registers
epdc snapshot [out.bin]         same, and save the 4 KiB register window
epdc pmic [dump|up [ms|stay]|down]   Papyrus registers; rails up (and down again) from userspace
epdc wbf <in.wbf> [out.wrf]     convert a waveform and print its header
epdc white [--check] [wbf]      controller init + golden check (+ one full-screen INIT update unless --check)
epdc show <image.pgm> [mode] [wbf] [--no-init]   draw an 800x600 8-bit PGM with one full-screen update
                                (mode: 0 INIT, 1 DU, 2 GC16, 4 A2, 5 GL16); an INIT comes first unless --no-init
```

Everything but `wbf` needs the shim loaded. `white` refuses to power the panel unless the parsed waveform matches a
cached `.wrf` beside it (when one exists) and every static register matches the golden snapshot.

## Building

```
cd epdc && cargo build --release --target armv7-unknown-linux-musleabihf   # static, no libc on the device needed
cd epdc && cargo test                                                       # waveform converter against a cached .wrf
KINDLE_KERNEL_TREE=/path/to/tree shim/build.sh                              # the shim, in the toolchain container
```

Rust's musl target links with `rust-lld` and `crt-static` (see `epdc/.cargo/config.toml`), which is what lets a
2026 toolchain produce a binary for a 2009 kernel.

## Getting your panel's waveform

The waveform is specific to the panel in front of you and is not in this repository. The Kindle's boot job reads it
from the panel's flash and leaves it in `/var/local/eink/panel/` as `<name>.wbf`, with Amazon's conversion beside
it as `<name>.wrf`. `scripts/fetch-panel.sh` copies both (plus `panel_id`) into `panel/`, generating the `.wrf` on
the device with Amazon's own tool first if it is missing. `epdc wbf panel/<name>.wbf` then shows the header and
proves the converter reproduces the `.wrf`; the milestone scripts upload whatever is in `panel/`.

`epdc-regs-stock.bin` was taken from one Kindle Touch; the values are the board file's constants, so another Touch
should match, and `epdc snapshot` regenerates it from the stock driver's state if yours differs.

## Running a session

The milestone scripts assume an ssh alias for the Kindle (`KINDLE_HOST`, default `kindle`) with root, usbnet or
wifi. The device must be jailbroken for ssh; nothing else is changed on it.

1. `scripts/fetch-panel.sh` once, then `scripts/hold.sh`: stops `powerd` and guards `/sys/power/state`, so the
   device stays up.
2. `scripts/m3-init-white.sh` (or `CHECK=1 scripts/m3-init-white.sh` for the dry run), or
   `scripts/m4-image.sh [image.pgm] [mode]` for an image: stops the UI jobs, unloads `mxc_epdc_fb`, loads the shim,
   runs the driver, powers down, reloads stock through `/etc/upstart/display`. `images/milestone4-800x600.pgm` is
   the test image, in the panel's native landscape orientation (rotate a portrait image by 90° clockwise).
3. `scripts/release.sh`, then reboot the device to return it to its normal state.

## Safety rules, all enforced in code or script

- EPDC registers are touched only through the shim, which turns the clocks on first; a register access with the
  clocks gated stalls the bus and hangs the CPU (learned the hard way; now impossible by construction).
- Writes into the DMA buffers are aligned 32-bit stores; UIO maps them strongly-ordered and `memcpy` faults.
- The waveform is the panel's own, parsed and checksum-verified, and compared with the stock conversion.
- Every static register is compared with the stock driver's values before the rails come up.
- VCOM is never written; it stays at the value the kernel programmed from the panel's data.
- Power sequencing is the PMIC's own, with power-good polling, and the rails come down on every exit path.
- Nothing is written to the device's root or boot partitions; a reboot restores stock.
- The first update after taking the panel over is an INIT: the working buffer is unknown to us until then, and a
  GC16 from a wrong "old" state ghosts the previous image through.

## Acknowledgements

Amazon's GPL kernel source for firmware 5.3.7.3 (the EPDC driver, the Papyrus regulator driver, the board file and
the waveform module) is the reference for every register and sequence here. The waveform converter in `wbf.rs` is a
Rust port of [inkwave](https://github.com/fread-ink/inkwave) (GPLv2; Copyright 2018, 2021 Marc Juul, with code
derived from Amazon Technologies and Freescale Semiconductor GPL sources), checked byte for byte against the output
of Amazon's own conversion tool.

## Licence

GPL-2.0-only throughout: `shim/` because it is a Linux kernel module, `epdc/` because its waveform converter is a
port of inkwave, which is GPLv2. Full text in `shim/COPYING` and `epdc/COPYING`.
