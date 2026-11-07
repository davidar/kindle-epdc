#!/usr/bin/env bash
# Milestone 1: unload Amazon's EPDC driver, load our UIO shim (which turns the EPDC clocks on), read the registers
# from userspace (this is also the golden snapshot: the stock driver's remove() powers down but never soft-resets the
# block, so the timing/format registers still hold what Amazon programmed), then put the stock driver back and do a
# full refresh through Amazon's own tool so the panel is seen to be fine afterwards. The stock reload goes through
# /etc/upstart/display (mounts /mnt/wfm, loads with the panel's .wrf proxy); a bare insmod gives the generic waveform.
# Preconditions: Kindle reachable over ssh and held awake with scripts/hold.sh.
# Never reads EPDC registers through /dev/mem (7 Nov 2026 21:00 hang). All copies with scp -O (dropbear).
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
K=/lib/modules/2.6.31-rt11-lab126/kernel/drivers/video/mxc/mxc_epdc_fb.ko
log=logs/m1-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m1 start $(date +%T)"

ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:/mnt/us/kindle-epdc/ || { echo "scp failed"; exit 1; }

ssh -o ConnectTimeout=8 $H '
chmod 755 /mnt/us/kindle-epdc/epdc
echo "== sizes"; /bin/ls -la /mnt/us/kindle-epdc/epdc /mnt/us/kindle-epdc/epdc_uio.ko
echo "== stop the UI jobs (they hold /dev/fb0)"; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 4
/sbin/initctl list | grep -i "framework\|lab126_gui\|powerd" || true
echo "== the X stack is the upstart job x (lxinit); killing Xorg just respawns it and re-triggers framework, so stop the job"
/sbin/stop x >/dev/null 2>&1 || true; sleep 3
/sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
/sbin/initctl list | grep -i "^x \|^framework \|^lab126_gui \|^powerd " || true
echo "== module refcnt"; cat /sys/module/mxc_epdc_fb/refcnt
echo "== fb0 holders (must be none)"; n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && { echo "$(basename $p): $(tr "\0" " " < $p/cmdline)"; n=$((n+1)); }; done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
grep -q "^mxc_epdc_fb " /proc/modules && { echo "still loaded?!"; exit 1; }
echo "== load shim"; /sbin/insmod /mnt/us/kindle-epdc/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed; reloading stock"; /sbin/insmod $K; exit 1; }
dmesg | tail -n 5
echo "== uio"; for u in /sys/class/uio/uio*; do echo "$u name=$(cat $u/name)"; for m in $u/maps/map*; do echo "  $(basename $m) $(cat $m/name 2>/dev/null) addr=$(cat $m/addr) size=$(cat $m/size)"; done; done
/bin/ls -la /dev/uio*
echo "== probe (golden snapshot, clocks on via shim)"; /mnt/us/kindle-epdc/epdc probe
/mnt/us/kindle-epdc/epdc snapshot /mnt/us/kindle-epdc/epdc-regs-stock.bin
echo "== restore stock"; /sbin/rmmod epdc_uio && echo "epdc_uio unloaded"
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
/etc/upstart/display && echo "stock reloaded via the boot display job (panel waveform, splash drawn)"
sleep 2; dmesg | grep -i "init_hw\|25C" | tail -n 2; cat /sys/class/graphics/fb0/name
echo "== full refresh through Amazon tool (panel sanity)"; /usr/sbin/eips -f 2>&1 || echo "eips failed"
sleep 3; echo "== done on device $(date)"
' 2>&1 | grep -v "Welcome to Kindle\|^$"
rc=${PIPESTATUS[0]}
scp -O -q $H:/mnt/us/kindle-epdc/epdc-regs-stock.bin ./epdc-regs-stock.bin 2>/dev/null && /bin/ls -la epdc-regs-stock.bin
echo "m1 done rc=$rc $(date +%T)"
