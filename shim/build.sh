#!/usr/bin/env bash
# Build epdc_uio.ko as an external module against an Amazon 2.6.31 tree built with the matching toolchain
# (see docs/kernel-tree.md: same container image and output tree as the kernel, so vermagic matches the device).
set -u
cd "$(dirname "$0")"
K=${KINDLE_KERNEL_TREE:-$HOME/git/kindle-touch-kernel}
podman run --rm \
  -v "$K/amazon-src/linux:/src:Z" -v "$K/build/yoshi:/out:Z" -v "$PWD:/mod:Z" \
  localhost/kindle-touch-xgcc46 \
  bash -c 'make -C /src O=/out ARCH=arm CROSS_COMPILE=arm-linux-gnueabi- M=/mod modules 2>&1 | tail -15 && /usr/bin/arm-linux-gnueabi-objdump -s -j .modinfo /mod/epdc_uio.ko 2>/dev/null | tail -8'
/bin/ls -la epdc_uio.ko 2>/dev/null && strings epdc_uio.ko | grep -E '^vermagic=|^depends=|^license='
