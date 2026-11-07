# kindle-epdc: notes for an AI assistant working in this repository

Read `README.md` first (what this is, layout, commands) and `docs/hardware.md` (every hardware fact with its source).
`docs/kernel-tree.md` covers building the shim against the stock kernel.

## Build and test

- Driver: `cd epdc && cargo build --release --target armv7-unknown-linux-musleabihf` (static musl binary for the
  Kindle; `rust-lld`, crt-static, see `epdc/.cargo/config.toml`). `cargo test` runs on the host; the converter
  tests need the panel files in `panel/` (`scripts/fetch-panel.sh`) and skip otherwise.
- Shim: `KINDLE_KERNEL_TREE=/path shim/build.sh`, inside the toolchain container described in `docs/kernel-tree.md`.
  Check `vermagic=2.6.31-rt11-lab126 mod_unload ARMv7` in the output.
- Device access is over ssh (`KINDLE_HOST`, default `kindle`). Copy files with `scp -O` (dropbear, no sftp).

## Rules for touching the device

These are enforced in code or scripts; keep them that way when changing things.

1. EPDC registers are reached only through `Uio::map_regs()`, which refuses unless the shim reports the clocks
   on. Never add a `/dev/mem` path. A gated access stalls the bus and the device needs a hard reset.
2. Writes into the DMA maps use `Map::copy_in` / `Map::fill` (aligned 32-bit volatile stores). The maps are
   strongly-ordered; `memcpy` into them faults.
3. The waveform comes from the panel's own `.wbf`, parsed by `wbf.rs`; a mismatch with a cached `.wrf` aborts.
   Never write to the panel's SPI flash.
4. Before the rails come up, `check_golden` must pass (static registers equal to `epdc-regs-stock.bin`).
5. VCOM (the Papyrus DVR register) is never written. Temperature is read from the PMIC before updates.
6. Rails come down on every exit path (failed step, panic hook, and the scripts' unconditional `pmic down`).
7. Nothing is written to the device's root or boot partitions. Our files live in `/mnt/us/kindle-epdc/`.
   Stock is restored with `/etc/upstart/display`, never a bare insmod. End a session by rebooting the device.
8. The first update after taking the panel over is an INIT (the working buffer is unknown until then).

## Working on the Kindle

- Hold it awake first (`scripts/hold.sh`: stops `powerd`, guards `/sys/power/state`); a suspended Kindle drops its
  USB gadget and a button wake does not bring it back without a cable replug.
- The UI is the upstart job `x` plus `lab126_gui` and `framework`; stop the jobs (killing processes respawns them).
  `/sbin` is not on the ssh PATH.
- One milestone script per run; each swaps the stock driver out, does one thing, powers down, restores stock.
- When a run needs a human to look at the panel, say so and end the turn before running it; text written in the
  same turn as the tool call arrives after the fact.

## Style

Neutral, factual prose in anything committed. No personal names, hostnames, home paths or device IP addresses in
the repository; those belong in the gitignored `CLAUDE.local.md`. Small follow-up edits are amended into the
commit they belong to rather than stacked as new commits. The repository owner reviews before anything is pushed.
