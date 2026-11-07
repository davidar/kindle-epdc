#!/usr/bin/env bash
# Milestone 3: the first picture. CHECK=1 = dry run (waveform into DMA, EPDC init, golden compare; rails never
# powered, nothing submitted). Without CHECK: rails up, one full-screen INIT update (mode 0, fixed next pixel 0),
# wait for LUT 0, rails down. Stock restored through the boot display job either way.
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
log=logs/m3-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m3 start $(date +%T) CHECK=${CHECK:-0}"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:/mnt/us/kindle-epdc/ || { echo "scp failed"; exit 1; }
scp -O -q panel/06_05_007a_3c_158e21_03_3c_00000986_85_04.wbf $H:/mnt/us/kindle-epdc/panel.wbf || { echo "scp wbf failed"; exit 1; }
scp -O -q panel/06_05_007a_3c_158e21_03_3c_00000986_85_04.wrf $H:/mnt/us/kindle-epdc/panel.wrf || { echo "scp wrf failed"; exit 1; }
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 /mnt/us/kindle-epdc/epdc; md5sum /mnt/us/kindle-epdc/epdc /mnt/us/kindle-epdc/epdc_uio.ko /mnt/us/kindle-epdc/panel.wbf /mnt/us/kindle-epdc/panel.wrf | cut -c1-8
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && n=$((n+1)); done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
echo "== load shim"; /sbin/insmod /mnt/us/kindle-epdc/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
dmesg | grep "epdc_uio" | tail -n 2
echo "== white '"${CHECK:+--check}"'"; /mnt/us/kindle-epdc/epdc white '"${CHECK:+--check}"'; echo "rc=$?"
echo "== backstop: rails down regardless of how white ended"; /mnt/us/kindle-epdc/epdc pmic down; echo "rc=$?"
echo "== pmic after"; /mnt/us/kindle-epdc/epdc pmic dump | grep "ENABLE\|PG_GOOD\|EXT_TEMP"
[ -n "'"${CHECK:-}"'" ] || { echo "holding 8 s so the result can be seen before the stock splash"; sleep 8; }
echo "== restore stock"; /sbin/rmmod epdc_uio && echo "epdc_uio unloaded"
/etc/upstart/display && echo "stock reloaded via boot display job"; sleep 2; dmesg | grep -i "init_hw\|25C" | tail -n 1; cat /sys/class/graphics/fb0/name
echo "== done on device $(date)"
' 2>&1 | grep -v "Welcome to Kindle\|^$"
echo "m3 done rc=${PIPESTATUS[0]} $(date +%T)"
