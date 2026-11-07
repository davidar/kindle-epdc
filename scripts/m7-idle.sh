#!/usr/bin/env bash
# Milestone 7: unattended operation. Two steps on the device, one shim load:
#   1  `epdc gate-test`: map the registers, gate the clocks behind the map's back, touch a register. The shim's
#      fault handler must kill the process with SIGBUS (exit 135) and log why; the device must stay up. Rails are
#      never powered for this step. (Before milestone 7 the same access wedged the CPU.)
#   2  `epdc idle-cycle N --gap MS`: take the panel over, then N times: draw a DU strip, go idle (rails down,
#      CTRL.CLKGATE, register map dropped, clocks gated by the shim), wait, wake (clocks, CLKGATE, rails) and
#      verify the registers survived. Each strip after the first proves the controller and working buffer came
#      through the gating intact.
# CHECK=1 = dry run (bring-up and golden check only). Stock restored through the boot display job afterwards.
#   scripts/m7-idle.sh [N] [GAP_MS]
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
D=/mnt/us/kindle-epdc
N=${1:-6}; GAP=${2:-3000}
log=logs/m7-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m7 start $(date +%T) CHECK=${CHECK:-0} N=$N GAP=$GAP"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:$D/ || { echo "scp failed"; exit 1; }
WBF=$(/bin/ls panel/*.wbf 2>/dev/null | head -1); WRF=${WBF%.wbf}.wrf
[ -n "$WBF" ] || { echo "no panel/*.wbf: run scripts/fetch-panel.sh first"; exit 1; }
scp -O -q "$WBF" $H:$D/panel.wbf || { echo "scp wbf failed"; exit 1; }
[ -f "$WRF" ] && { scp -O -q "$WRF" $H:$D/panel.wrf || { echo "scp wrf failed"; exit 1; }; }
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 '"$D"'/epdc; md5sum '"$D"'/epdc '"$D"'/epdc_uio.ko '"$D"'/panel.wbf | cut -c1-8 | tr "\n" " "; echo
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && n=$((n+1)); done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
echo 3 > /proc/sys/vm/drop_caches
echo "== load shim"; /sbin/insmod '"$D"'/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
dmesg | grep "epdc_uio" | tail -n 1
C=/sys/bus/platform/devices/mxc_epdc_fb/clocks; echo "clocks attr: $(cat $C) ($(stat -c %a $C))"
if [ -n "'"${CHECK:-}"'" ]; then echo "== dry run: white --check"; '"$D"'/epdc white --check; echo "rc=$?"
else
  echo "== 1. gate-test (expect SIGBUS, exit 135)"; '"$D"'/epdc gate-test; rc=$?; echo "rc=$rc"; dmesg | grep "clocks gated" | tail -n 1
  echo "clocks attr after: $(cat $C)"; [ $rc -eq 135 ] && echo "GUARD OK: stale register access killed the process, device alive" || echo "GUARD UNEXPECTED rc=$rc"
  echo "== ungate for the next step"; echo 1 > $C; echo "clocks attr: $(cat $C)"
  echo "== 2. idle-cycle '"$N"' --gap '"$GAP"'"; '"$D"'/epdc idle-cycle '"$N"' --gap '"$GAP"'; echo "rc=$?"
fi
echo "== backstop: rails down regardless"; '"$D"'/epdc pmic down; echo "rc=$?"; echo "clocks attr: $(cat $C)"
echo "== pmic after"; '"$D"'/epdc pmic dump | grep "ENABLE\|PG_GOOD\|EXT_TEMP"
[ -n "'"${CHECK:-}"'" ] || { echo "holding 15 s so the strips can be seen before the stock splash"; sleep 15; }
echo "== restore stock"; /sbin/rmmod epdc_uio && echo "epdc_uio unloaded"
echo 3 > /proc/sys/vm/drop_caches
/etc/upstart/display && echo "stock reloaded via boot display job"; sleep 2; dmesg | grep -i "init_hw\|25C" | tail -n 1
if [ -e /dev/fb0 ]; then cat /sys/class/graphics/fb0/name; else
  echo "RESTORE FAILED: no /dev/fb0 (stock probe ENOMEM?); retrying once after dropping caches"; dmesg | grep "probe of mxc_epdc_fb\|allocation failure" | tail -n 2
  /sbin/rmmod mxc_epdc_fb; echo 3 > /proc/sys/vm/drop_caches; sleep 1; /etc/upstart/display; sleep 2
  [ -e /dev/fb0 ] && cat /sys/class/graphics/fb0/name || echo "RESTORE FAILED AGAIN: reboot the device to recover the stock display"
fi
echo "== done on device $(date)"
' 2>&1 | grep -v "Welcome to Kindle\|^$"
echo "m7 done rc=${PIPESTATUS[0]} $(date +%T)"
