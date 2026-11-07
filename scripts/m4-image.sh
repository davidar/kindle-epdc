#!/usr/bin/env bash
# Milestone 4: the first real pixels. Uploads a PGM (800x600, P5, 8-bit, panel-native landscape orientation) and
# shows it with one full-screen update in the given waveform mode (default GC16 = 2), through the same bring-up,
# golden check and rails sequence as m3. Stock restored through the boot display job afterwards.
#   scripts/m4-image.sh [image.pgm] [mode]
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
IMG=${1:-images/milestone4-800x600.pgm}
MODE=${2:-2}
log=logs/m4-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m4 start $(date +%T) image=$IMG mode=$MODE"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:/mnt/us/kindle-epdc/ || { echo "scp failed"; exit 1; }
scp -O -q "$IMG" $H:/mnt/us/kindle-epdc/image.pgm || { echo "scp image failed"; exit 1; }
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 /mnt/us/kindle-epdc/epdc; md5sum /mnt/us/kindle-epdc/epdc /mnt/us/kindle-epdc/epdc_uio.ko /mnt/us/kindle-epdc/panel.wbf /mnt/us/kindle-epdc/image.pgm | cut -c1-8
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && n=$((n+1)); done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
echo "== load shim"; /sbin/insmod /mnt/us/kindle-epdc/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
echo "== show"; /mnt/us/kindle-epdc/epdc show /mnt/us/kindle-epdc/image.pgm '"$MODE"'; echo "rc=$?"
echo "== backstop: rails down regardless"; /mnt/us/kindle-epdc/epdc pmic down; echo "rc=$?"
echo "== pmic after"; /mnt/us/kindle-epdc/epdc pmic dump | grep "ENABLE\|PG_GOOD\|EXT_TEMP"
echo "holding 15 s so the image can be seen before the stock splash"; sleep 15
echo "== restore stock"; /sbin/rmmod epdc_uio && echo "epdc_uio unloaded"
/etc/upstart/display && echo "stock reloaded via boot display job"; sleep 2; dmesg | grep -i "init_hw\|25C" | tail -n 1; cat /sys/class/graphics/fb0/name
echo "== done on device $(date)"
' 2>&1 | grep -v "Welcome to Kindle\|^$"
echo "m4 done rc=${PIPESTATUS[0]} $(date +%T)"
