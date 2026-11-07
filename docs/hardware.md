# Hardware facts: Kindle Touch (2011, "yoshi"), firmware 5.3.7.3

Everything here comes from Amazon's GPL source release for firmware 5.3.7.3 (`Kindle_src_5.3.7.3_2715280002.tar.gz`,
kernel `2.6.31-rt11-lab126`, board `imx50_yoshi`) or was read off the device. File references are into that tree.

## SoC and display controller

| Fact | Value | Source |
|---|---|---|
| SoC | Freescale i.MX50 (Cortex-A8, 800 MHz), 256 MB RAM | `/proc/cpuinfo`, `/proc/meminfo` |
| EPDC registers | `0x41010000`, 4 KiB (`DEBUG_BASE_ADDR 0x40000000 + 0x01010000`) | `arch/arm/plat-mxc/include/mach/mx5x.h`, `arch/arm/mach-mx5/devices.c` |
| EPDC interrupt | 27 (`MXC_INT_EPDC`), named `fb_dma` in `/proc/interrupts` | `mx5x.h` |
| EPDC version register | `0x01000000` (v1.0.0, the i.MX50 revision; 16 LUTs) | read through the shim |
| Register map | `drivers/video/mxc/epdc_regs.h` (v1; `epdc_regs_v2.h` is later silicon) | |
| Clocks | `epdc_axi`, `epdc_pix` (pixel clock set to 32 MHz) | `drivers/video/mxc/mxc_epdc_fb.c` |
| Clock gating | the stock driver disables both clocks and sets `CTRL.CLKGATE` whenever the display powers down; a bus access to the gated block hangs the CPU | `epdc_powerdown()` |
| Pin mux | `gpio_epdc_pins_enable(1)` (SDCE0, GDSP); PWRCOM is muxed to the EPDC (ALT0) | `arch/arm/mach-mx5/mx50_yoshi_gpio.c` |

## Panel and timing

| Fact | Value | Source |
|---|---|---|
| Panel | E Ink 6", 800×600, 85 Hz, V220 film, waveform type WJ (ED060SCG family) | waveform header |
| Mode | `E60_V220_WJ`: pixclock 32 MHz, margins left 17 / right 172 / upper 4 / lower 18, hsync 15, vsync 4 | `arch/arm/mach-mx5/mx50_yoshi.c` |
| EPDC timing | vscan_holdoff 4, sdoed width 10 / delay 20, sdoez width 10 / delay 20, gdclk_hp_offs 425, gdsp_offs 20, gdoe_offs 0, gdclk_offs 17, num_ce 1 | `mx50_yoshi.c` |
| Golden registers | `../epdc-regs-stock.bin`: the full 4 KiB window as the stock driver leaves it (FORMAT 0x400 = P4N buffer, 2-bit TFT; FIFOCTRL 0x0064c864; RES 0x02580320; TCE_CTRL 0x40002; SDCFG 0x314320; GDCFG 0x10; HSCAN1 0xf000f; HSCAN2 0xac0011; VSCAN 0x120404; OE 0x0a140a14; POLARITY 0xe; TIMING2 0x01a90014; TIMING3 0x11; IRQ_MASK 0x40000) | read through the shim |
| Stock framebuffer | 600×800 portrait, 8 bpp, `rotate=1`, rotated by the PXP into the EPDC's native 800×600 | `/sys/class/graphics/fb0` |

## Display power: "Papyrus" (TI TPS65180 family)

| Fact | Value | Source |
|---|---|---|
| Bus / address | I2C bus 1 (`/dev/i2c/1` on this firmware), `0x48` | `mx50_yoshi.c` |
| Driver | `drivers/regulator/papyrus-regulator.c`, built in; regulators DISPLAY, VCOM, TMST | config |
| Control pins | GPIO2_27 PWRCOM (VCOM enable), GPIO2_28 PWRSTAT (power good, input), GPIO2_29 PWRCTRL0 (wakeup), GPIO2_30 PWRCTRL1 (interrupt) | `mx50_pins.h` |
| Registers | 0x00 EXT_TEMP, 0x01 ENABLE, 0x02 CONFIG, 0x04 DVR (VCOM), 0x05 INT_TEMP, 0x06 STATUS, 0x07 PRODUCT_ID, 0x0A FAULT, 0x0B HVINP, 0x0C TMST_CONFIG, 0x0D/0x0E TIMING1/2, 0x0F PG_GOOD, 0x10 PRODUCT_REV, 0x11–0x15 TIMING4–8 | `papyrus-regulator.c` |
| Power up | ENABLE ← 0xbf; wait for PWRSTAT high (≈5 ms, PG_GOOD reads 0xfa); VCOM: DVR unchanged, PWRCOM pin high and `EPDC_GPIO.PWRCOM` set (after clearing `CTRL.CLKGATE`) | `papyrus_wake()`, `epdc_powerup_vcom()` |
| Power down | PWRCOM clear, ENABLE ← 0x7f, wait PG_GOOD == 0 (≈6 ms), `CTRL.CLKGATE` set | `papyrus_standby()`, `epdc_powerdown()` |
| VCOM | DVR 0xbf on this unit = −2.06 V (driver table: DVR × 2.75 V / 255); programmed by the kernel from the panel data, never chosen by us | `papyrus_vcom_set_voltage()` |
| Temperature | EXT_TEMP in °C; a fresh conversion is triggered with TMST_CONFIG ← 0x80 | `papyrus_tmst_get_temp()` |
| Output GPIO readback | reads 0 on i.MX5 unless the pad has SION set, so PWRCOM/PWRCTRL0 cannot be read back; the chip answering on I2C is the "awake" test | observed |

## Waveform

| Fact | Value | Source |
|---|---|---|
| Where it lives | the panel's SPI flash (SPI bus 2, cs 0, `panel_flash_spi`); the boot job `/etc/upstart/display` caches `/var/local/eink/panel/<name>.wbf` and the converted `<name>.wrf` | `drivers/video/eink/waveform/eink_panel.c`, boot script |
| Conversion | `.wbf` → `.wrf` is done by the closed userland tool `/usr/sbin/eu extract` (deterministic); the kernel loads the `.wrf` | boot script, `mxc_epdc_fb_lab126.c` |
| This panel's file | `06_05_007a_3c_158e21_03_3c_00000986_85_04.wbf`: 112,175 bytes, serial 2438, mode version 0x04, 6 modes, 14 temperature ranges (0,3,6,9,12,15,18,21,24,27,30,33,38,43 °C), 85 Hz; `.wrf` 1,172,655 bytes | header |
| Modes (mode version 4) | INIT 0, DU 1, GC16 2, GC16_FAST → 2, A2 4, GL16 5, GL16_FAST → 5, DU4 → 2 | `eink_panel.c` mode tables, `include/linux/einkwf.h` |
| `.wrf` layout | 48-byte header, trc+2 temperature bytes, then at offset 63: mode table (8 bytes per mode), per mode a range table (8 bytes per range), per range an 8-byte block (big-endian u16 count) and the unpacked waveform (4 bytes per packed byte, one 2-bit drive value each) | inkwave, `wbf.rs` |
| In the EPDC | `WVADDR` = physical address of the data after the temperature bytes; `TEMP` = range index; the update's waveform mode selects the mode | `epdc_init_sequence()` |

## Boot-time facts worth knowing

- The UI is the upstart job `x` (lxinit → Xorg, awesome, blanket); killing its processes respawns them and `started x`
  restarts `framework`. Stop the jobs (`x`, `lab126_gui`, `framework`) to free `/dev/fb0`.
- `/sbin` is not on the ssh PATH (`/sbin/insmod`, `/sbin/rmmod`, `/sbin/stop`, `/sbin/start`, `/sbin/initctl`).
- `scp` to the device must be `scp -O`: dropbear has no sftp and plain scp silently leaves 0-byte files.
- A bare `insmod mxc_epdc_fb.ko` loads the built-in generic 25 °C waveform ("Using 25C-only waveform!"); the proper
  reload is `/etc/upstart/display`, which mounts the waveform store, loads the module with
  `default_panel_hw_init=1 default_update_mode=1` so it reads the cached `.wrf`, draws the splash and unmounts.
- The stock module's `remove()` powers down but never soft-resets the EPDC, so after unloading it the timing registers
  still hold what it programmed; that is how the golden snapshot was taken.
