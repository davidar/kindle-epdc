// SPDX-License-Identifier: GPL-2.0-only
//! i.MX50 EPDC (v1) register offsets, from Amazon's drivers/video/mxc/epdc_regs.h.
//! The *_SET / *_CLEAR / *_TOGGLE aliases at +4/+8/+C are the usual i.MX write-1-to-{set,clear,toggle} views.
#![allow(dead_code)]
pub const CTRL: usize = 0x000;
pub const CTRL_SET: usize = 0x004;
pub const CTRL_CLEAR: usize = 0x008;
pub const WVADDR: usize = 0x020;
pub const WB_ADDR: usize = 0x030;
pub const RES: usize = 0x040;
pub const FORMAT: usize = 0x050;
pub const FIFOCTRL: usize = 0x0A0;
pub const UPD_ADDR: usize = 0x100;
pub const UPD_CORD: usize = 0x120;
pub const UPD_SIZE: usize = 0x140;
pub const UPD_CTRL: usize = 0x160;
pub const UPD_FIXED: usize = 0x180;
pub const TEMP: usize = 0x1A0;
pub const TCE_CTRL: usize = 0x200;
pub const TCE_SDCFG: usize = 0x220;
pub const TCE_GDCFG: usize = 0x240;
pub const TCE_HSCAN1: usize = 0x260;
pub const TCE_HSCAN2: usize = 0x280;
pub const TCE_VSCAN: usize = 0x2A0;
pub const TCE_OE: usize = 0x2C0;
pub const TCE_POLARITY: usize = 0x2E0;
pub const TCE_TIMING1: usize = 0x300;
pub const TCE_TIMING2: usize = 0x310;
pub const TCE_TIMING3: usize = 0x320;
pub const IRQ_MASK: usize = 0x400;
pub const IRQ_MASK_SET: usize = 0x404;
pub const IRQ_MASK_CLEAR: usize = 0x408;
pub const IRQ: usize = 0x420;
pub const IRQ_SET: usize = 0x424;
pub const IRQ_CLEAR: usize = 0x428;
pub const STATUS_LUTS: usize = 0x440;
pub const STATUS_NEXTLUT: usize = 0x460;
pub const STATUS_COL: usize = 0x480;
pub const STATUS: usize = 0x4A0;
pub const DEBUG: usize = 0x500;
pub const DEBUG_LUT0: usize = 0x540;
pub const GPIO: usize = 0x700;
pub const VERSION: usize = 0x7F0;

/// Registers worth dumping in `probe`, with names.
pub const DUMP: &[(&str, usize)] = &[
    ("CTRL", CTRL), ("WVADDR", WVADDR), ("WB_ADDR", WB_ADDR), ("RES", RES), ("FORMAT", FORMAT),
    ("FIFOCTRL", FIFOCTRL), ("UPD_ADDR", UPD_ADDR), ("UPD_CORD", UPD_CORD), ("UPD_SIZE", UPD_SIZE),
    ("UPD_CTRL", UPD_CTRL), ("UPD_FIXED", UPD_FIXED), ("TEMP", TEMP), ("TCE_CTRL", TCE_CTRL),
    ("TCE_SDCFG", TCE_SDCFG), ("TCE_GDCFG", TCE_GDCFG), ("TCE_HSCAN1", TCE_HSCAN1), ("TCE_HSCAN2", TCE_HSCAN2),
    ("TCE_VSCAN", TCE_VSCAN), ("TCE_OE", TCE_OE), ("TCE_POLARITY", TCE_POLARITY), ("TCE_TIMING1", TCE_TIMING1),
    ("TCE_TIMING2", TCE_TIMING2), ("TCE_TIMING3", TCE_TIMING3), ("IRQ_MASK", IRQ_MASK), ("IRQ", IRQ),
    ("STATUS_LUTS", STATUS_LUTS), ("STATUS_NEXTLUT", STATUS_NEXTLUT), ("STATUS_COL", STATUS_COL),
    ("STATUS", STATUS), ("GPIO", GPIO), ("VERSION", VERSION),
];

// Bitfields (same header). Only the ones the driver will need are named; the rest can be added as they come up.
pub const CTRL_SFTRST: u32 = 0x8000_0000;
pub const CTRL_CLKGATE: u32 = 0x4000_0000;
pub const CTRL_SRAM_POWERDOWN: u32 = 0x100;
pub const CTRL_UPD_DATA_SWIZZLE_MASK: u32 = 0xC0;
pub const CTRL_LUT_DATA_SWIZZLE_MASK: u32 = 0x30;
pub const CTRL_BURST_LEN_8_8: u32 = 0x1;
pub const RES_VERTICAL_OFFSET: u32 = 16;
pub const FORMAT_BUF_PIXEL_SCALE_ROUND: u32 = 0x100_0000;
pub const FORMAT_DEFAULT_TFT_PIXEL_OFFSET: u32 = 16;
pub const FORMAT_BUF_PIXEL_FORMAT_P4N: u32 = 0x400; // 8 bpp grey in memory, 4 bits used
pub const FORMAT_TFT_PIXEL_FORMAT_2BIT: u32 = 0x0;
pub const FORMAT_TFT_PIXEL_FORMAT_4BIT: u32 = 0x2;
pub const UPD_CORD_YCORD_OFFSET: u32 = 16;
pub const UPD_SIZE_HEIGHT_OFFSET: u32 = 16;
pub const UPD_CTRL_USE_FIXED: u32 = 0x8000_0000;
pub const UPD_CTRL_LUT_SEL_OFFSET: u32 = 16;
pub const UPD_CTRL_WAVEFORM_MODE_OFFSET: u32 = 8;
pub const UPD_CTRL_UPDATE_MODE_FULL: u32 = 0x1;
pub const UPD_FIXED_FIXNP_EN: u32 = 0x8000_0000;
pub const UPD_FIXED_FIXCP_EN: u32 = 0x4000_0000;
pub const TCE_CTRL_VSCAN_HOLDOFF_OFFSET: u32 = 16;
pub const TCE_CTRL_VCOM_MODE_MANUAL: u32 = 0x000;
pub const TCE_CTRL_VCOM_MODE_AUTO: u32 = 0x200;
pub const TCE_CTRL_DDR_MODE_ENABLE: u32 = 0x100;
pub const TCE_CTRL_LVDS_MODE_ENABLE: u32 = 0x40;
pub const TCE_CTRL_SCAN_DIR_0_UP: u32 = 0x10;
pub const TCE_CTRL_DUAL_SCAN_ENABLE: u32 = 0x8;
pub const TCE_CTRL_SDDO_WIDTH_16BIT: u32 = 0x4;
pub const TCE_CTRL_PIXELS_PER_SDCLK_4: u32 = 2;
pub const TCE_SDCFG_NUM_CE_OFFSET: u32 = 16;
pub const TCE_SDCFG_SDDO_INVERT_ENABLE: u32 = 0x2000;
pub const TCE_SDCFG_GDOE_MODE_DELAYED_GDCLK: u32 = 0x2;
pub const TCE_SDCFG_GDSP_MODE_FRAME_SYNC: u32 = 0x1;
pub const TCE_HSCAN1_LINE_SYNC_WIDTH_OFFSET: u32 = 16;
pub const TCE_HSCAN2_LINE_END_OFFSET: u32 = 16;
pub const TCE_VSCAN_FRAME_END_OFFSET: u32 = 16;
pub const TCE_VSCAN_FRAME_BEGIN_OFFSET: u32 = 8;
pub const TCE_OE_SDOED_WIDTH_OFFSET: u32 = 24;
pub const TCE_OE_SDOED_DLY_OFFSET: u32 = 16;
pub const TCE_OE_SDOEZ_WIDTH_OFFSET: u32 = 8;
pub const TCE_POLARITY_GDSP_POL_ACTIVE_HIGH: u32 = 0x10;
pub const TCE_POLARITY_GDOE_POL_ACTIVE_HIGH: u32 = 0x8;
pub const TCE_POLARITY_SDOE_POL_ACTIVE_HIGH: u32 = 0x4;
pub const TCE_POLARITY_SDLE_POL_ACTIVE_HIGH: u32 = 0x2;
pub const TCE_POLARITY_SDCE_POL_ACTIVE_HIGH: u32 = 0x1;
pub const TCE_TIMING1_SDCLK_SHIFT_NONE: u32 = 0;
pub const TCE_TIMING2_GDCLK_HP_OFFSET: u32 = 16;
pub const TCE_TIMING3_GDOE_OFFSET_OFFSET: u32 = 16;
pub const IRQ_WB_CMPLT: u32 = 0x1_0000;
pub const IRQ_LUT_COL: u32 = 0x2_0000;
pub const IRQ_TCE_UNDERRUN: u32 = 0x4_0000;
pub const IRQ_FRAME_END: u32 = 0x8_0000;
pub const IRQ_BUS_ERROR: u32 = 0x10_0000;
pub const IRQ_TCE_IDLE: u32 = 0x20_0000;
pub const IRQ_LUT_MASK: u32 = 0xFFFF; // bits 0..15: per-LUT completion
pub const STATUS_NEXTLUT_NEXT_LUT_VALID: u32 = 0x100;
pub const STATUS_NEXTLUT_NEXT_LUT_MASK: u32 = 0xF;
pub const STATUS_LUTS_BUSY: u32 = 0x2;
pub const STATUS_WB_BUSY: u32 = 0x1;
pub const GPIO_PWRCOM: u32 = 0x40;
pub const GPIO_PWRCTRL_OFFSET: u32 = 2;
pub const GPIO_BDR_MASK: u32 = 0x3;

// Waveform modes (include/linux/mxcfb.h); the real LUT indices come from the panel's waveform header.
pub const WFM_INIT: u32 = 0;
pub const WFM_DU: u32 = 1;
pub const WFM_GC16: u32 = 2;
pub const WFM_GC16_FAST: u32 = 3;
pub const WFM_A2: u32 = 4;
pub const WFM_GL16: u32 = 5;
pub const WFM_GL16_FAST: u32 = 6;
pub const WFM_DU4: u32 = 7;
