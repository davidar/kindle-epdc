// SPDX-License-Identifier: GPL-2.0-only
mod display;
mod pmic;
mod regs;
mod uio;
mod wbf;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("probe") => probe(),
        Some("snapshot") => snapshot(args.get(2).map(String::as_str)),
        Some("white") => {
            let check = args.iter().any(|a| a == "--check");
            let wrf = args.iter().skip(2).find(|a| !a.starts_with("--")).map(String::as_str);
            display::white(check, wrf)
        }
        Some("wbf") => {
            // epdc wbf <in.wbf> [out.wrf] : convert, or just verify/print the header
            let inp = args.get(2).expect("wbf path");
            let d = std::fs::read(inp).expect("read wbf");
            let h = wbf::parse_header(&d).expect("header");
            println!("{:#?}\nbits per pixel {}", h, wbf::bits_per_pixel(&h));
            let out = wbf::convert(&d).expect("convert");
            println!("converted: {} bytes", out.len());
            if let Some(o) = args.get(3) { std::fs::write(o, &out).expect("write"); println!("wrote {o}"); }
        }
        Some("show") => {
            // epdc show <image.pgm> [mode] [wbf] [--no-init]
            let no_init = args.iter().any(|a| a == "--no-init");
            let pos: Vec<&String> = args.iter().skip(2).filter(|a| !a.starts_with("--")).collect();
            let image = pos.first().expect("image.pgm");
            let mode: u32 = pos.get(1).and_then(|m| m.parse().ok()).unwrap_or(regs::WFM_GC16);
            display::show(image, mode, pos.get(2).map(|s| s.as_str()), !no_init)
        }
        Some("pmic") => match args.get(2).map(String::as_str) {
            None | Some("dump") => pmic::dump(),
            Some("up") => pmic::up(args.get(3).map(String::as_str)),
            Some("down") => pmic::down(),
            _ => { eprintln!("usage: epdc pmic [dump|up [ms|stay]|down]"); std::process::exit(2); }
        },
        _ => {
            eprintln!("usage: epdc probe | snapshot [outfile] | pmic [dump|up|down] | white [--check] [wrf]");
            std::process::exit(2);
        }
    }
}

/// Milestone 1: open the shim, map registers and the DMA buffer, read the EPDC back. Touches nothing.
fn probe() {
    let u = uio::Uio::open_by_name("epdc").expect("open uio 'epdc' (is epdc_uio.ko loaded?)");
    let regs = u.map_regs().expect("map0 regs (clocks on?)");
    println!("uio{} regs phys {:#010x} len {:#x}", u.index, regs.phys, regs.len);
    let mut dmas = Vec::new();
    for i in 1..=3 {
        match u.map(i) {
            Ok(m) => { println!("  dma map{} phys {:#010x} len {:#x} ({} KiB)", i, m.phys, m.len, m.len / 1024); dmas.push(m); }
            Err(e) => println!("  dma map{}: {}", i, e),
        }
    }
    let dma = dmas.first().expect("at least one dma map");
    let ver = regs.read32(regs::VERSION);
    println!("EPDC VERSION {:#010x} (major {} minor {} step {})", ver, ver >> 24, (ver >> 16) & 0xff, ver & 0xffff);
    for (name, off) in regs::DUMP {
        println!("  {:<14} +{:#05x} = {:#010x}", name, off, regs.read32(*off));
    }
    // DMA buffer sanity: write a pattern through the uncached mapping and read it back.
    dma.write32(0, 0xA55A_1234);
    let back = dma.read32(0);
    println!("dma readback {:#010x} {}", back, if back == 0xA55A_1234 { "ok" } else { "MISMATCH" });
}

/// Register snapshot through the shim (clocks on). Never through /dev/mem: with Amazon's driver idle the EPDC
/// clocks are gated and a bus access to the block wedges the CPU (7 Nov 2026 21:00, hard reset needed).
fn snapshot(out: Option<&str>) {
    let u = uio::Uio::open_by_name("epdc").expect("open uio 'epdc' (load epdc_uio.ko first; it enables the clocks)");
    let m = u.map_regs().expect("map0 regs (clocks on?)");
    let ver = m.read32(regs::VERSION);
    println!("EPDC VERSION {:#010x} (major {} minor {} step {})", ver, ver >> 24, (ver >> 16) & 0xff, ver & 0xffff);
    for (name, off) in regs::DUMP {
        println!("  {:<14} +{:#05x} = {:#010x}", name, off, m.read32(*off));
    }
    if let Some(path) = out {
        let mut raw = Vec::with_capacity(4096);
        for off in (0..4096).step_by(4) {
            raw.extend_from_slice(&m.read32(off).to_le_bytes());
        }
        std::fs::write(path, &raw).expect("write snapshot");
        println!("wrote {} ({} bytes)", path, raw.len());
    }
}
