#!/usr/bin/env bash
# Milestone 2b: with the stock EPDC driver unloaded and our shim loaded, bring the Papyrus display rails up from
# userspace (ENABLE 0xbf, wait for the power-good pin, VCOM pin on with the DVR the kernel left), hold ~1.5 s, take
# them down (VCOM off, ENABLE 0x7f, wait PG_GOOD 0), then restore the stock driver through the boot display job.
# No pixel is driven: the EPDC is not programmed, so the panel sees rails + VCOM with no source/gate activity.
# Preconditions as m1-probe.sh. Never /dev/mem. scp -O.
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
log=logs/m2-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m2 start $(date +%T)"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:/mnt/us/kindle-epdc/ || { echo "scp failed"; exit 1; }
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 /mnt/us/kindle-epdc/epdc; md5sum /mnt/us/kindle-epdc/epdc /mnt/us/kindle-epdc/epdc_uio.ko | cut -c1-8
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
echo "== fb0 holders (must be none)"; n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && { echo "$(basename $p): $(tr "\0" " " < $p/cmdline)"; n=$((n+1)); }; done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== PMIC before (stock driver live)"; /mnt/us/kindle-epdc/epdc pmic dump | grep "ENABLE\|DVR\|PG_GOOD\|EXT_TEMP"
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
echo "== load shim"; /sbin/insmod /mnt/us/kindle-epdc/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
dmesg | tail -n 2; d=/sys/bus/platform/devices/mxc_epdc_fb; echo "clocks=$(cat $d/clocks) wakeup=$(cat $d/wakeup) vcom=$(cat $d/vcom) pwrgood=$(cat $d/pwrgood)"
echo "== rails up / hold 1500 ms / down"; /mnt/us/kindle-epdc/epdc pmic up 1500; echo "rc=$?"
echo "== PMIC after"; /mnt/us/kindle-epdc/epdc pmic dump | grep "ENABLE\|DVR\|PG_GOOD\|EXT_TEMP"; echo "vcom=$(cat $d/vcom) pwrgood=$(cat $d/pwrgood)"
echo "== restore stock"; /sbin/rmmod epdc_uio && echo "epdc_uio unloaded"
/etc/upstart/display && echo "stock reloaded via boot display job"; sleep 2; dmesg | grep -i "init_hw\|25C" | tail -n 1; cat /sys/class/graphics/fb0/name
echo "== done on device $(date)"
' 2>&1 | grep -v "Welcome to Kindle\|^$"
echo "m2 done rc=${PIPESTATUS[0]} $(date +%T)"
