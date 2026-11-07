# The kernel tree the shim is built against

The shim is an out-of-tree module for the stock kernel, so it has to be built against Amazon's source for the
exact firmware with a toolchain whose output the kernel accepts. `vermagic` on the device is
`2.6.31-rt11-lab126 mod_unload ARMv7`; `CONFIG_MODVERSIONS` is off, so a matching vermagic is all that is checked.

## Source

Amazon's GPL release for firmware 5.3.7.3: `Kindle_src_5.3.7.3_2715280002.tar.gz` (240,644,949 bytes,
sha256 `ae0fabcafde606335c01c0fed7dd6f8d20a070bb86a6424f20cd56ef5b1378cf`), from Amazon's source-code page. Inside,
`gplrelease/linux-2.6.31.tar.gz` is the kernel; it extracts flat (no top-level directory). The defconfig is
`imx50_yoshi_defconfig`. The running kernel's config is in `/proc/config.gz` on the device.

## Toolchain

The stock kernel was built with Linaro GCC 4.5.4 (2011.05), whose prebuilt binaries are gone. Ubuntu 12.04's
`gcc-4.6-arm-linux-gnueabi` (4.6.3) from `old-releases.ubuntu.com` builds the tree cleanly and the modules load.
A container keeps it contained:

```
FROM docker.io/library/ubuntu:12.04
RUN printf 'deb http://old-releases.ubuntu.com/ubuntu/ precise main universe\ndeb http://old-releases.ubuntu.com/ubuntu/ precise-updates main universe\n' > /etc/apt/sources.list \
 && apt-get update -qq \
 && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
      gcc-4.6-arm-linux-gnueabi binutils-arm-linux-gnueabi make bc uboot-mkimage u-boot-tools \
      gcc g++ libc6-dev flex bison libncurses5-dev bzip2 xz-utils patch perl python ca-certificates git \
 && ln -sf arm-linux-gnueabi-gcc-4.6 /usr/bin/arm-linux-gnueabi-gcc
ENV ARCH=arm CROSS_COMPILE=arm-linux-gnueabi-
WORKDIR /src
```

Build the image as `kindle-touch-xgcc46`, then with the kernel source at `$K/amazon-src/linux` and an output
directory at `$K/build/yoshi`:

```
podman run --rm -v "$K/amazon-src/linux:/src:Z" -v "$K/build/yoshi:/out:Z" kindle-touch-xgcc46 \
  bash -c 'make O=/out ARCH=arm CROSS_COMPILE=arm-linux-gnueabi- imx50_yoshi_defconfig && \
           make O=/out ARCH=arm CROSS_COMPILE=arm-linux-gnueabi- -j8 modules'
```

That produces the 36 stock modules (including `mxc_epdc_fb.ko` and `eink_fb_waveform.ko`) with the device's
vermagic. `shim/build.sh` then builds `epdc_uio.ko` as an external module against `/out` with
`KINDLE_KERNEL_TREE=$K`.

## On the device

- Nothing is ever written to the root or boot partitions. The shim and the driver live in `/mnt/us/kindle-epdc/`
  (the user store) and are loaded with `/sbin/insmod`. A reboot restores the stock state.
- Copy files with `scp -O`.
- The stock display driver is unloaded with `/sbin/rmmod mxc_epdc_fb` after the UI jobs are stopped, and restored
  with `/etc/upstart/display` (not a bare insmod).
