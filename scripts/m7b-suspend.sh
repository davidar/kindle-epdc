#!/usr/bin/env bash
# Milestone 7, suspend half. Loads the shim and runs `epdc suspend-test` DETACHED on the device (the USB link
# drops across a suspend, which would kill anything attached to the ssh session), with the suspend guard lifted
# for the duration. The test: draw, ask for suspend with the clocks on (the shim must refuse), go idle, arm the
# PMIC RTC alarm, suspend for real, wake on the alarm, wake the panel, draw again. This script then waits for
# the device to answer again, remounts the guard, collects the log and restores stock.
# If the device does not come back on the alarm: a cable replug re-enumerates USB (a button wake does not); the
# restore block can then be run by hand.
#   scripts/m7b-suspend.sh [ALARM_S]
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
D=/mnt/us/kindle-epdc
ALARM=${1:-30}
log=logs/m7b-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m7b start $(date +%T) ALARM=$ALARM"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:$D/ || { echo "scp failed"; exit 1; }
WBF=$(/bin/ls panel/*.wbf 2>/dev/null | head -1); WRF=${WBF%.wbf}.wrf
[ -n "$WBF" ] || { echo "no panel/*.wbf: run scripts/fetch-panel.sh first"; exit 1; }
scp -O -q "$WBF" $H:$D/panel.wbf || { echo "scp wbf failed"; exit 1; }
[ -f "$WRF" ] && { scp -O -q "$WRF" $H:$D/panel.wrf || { echo "scp wrf failed"; exit 1; }; }
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 '"$D"'/epdc; md5sum '"$D"'/epdc '"$D"'/epdc_uio.ko | cut -c1-8 | tr "\n" " "; echo
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && n=$((n+1)); done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
echo 3 > /proc/sys/vm/drop_caches
echo "== load shim"; /sbin/insmod '"$D"'/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
echo "== lift the suspend guard, launch detached"; umount /sys/power/state && echo "guard lifted"; cat /sys/power/state
rm -f '"$D"'/suspend-test.log
# no nohup on this busybox: a new session plus an ignored SIGHUP does the same job
setsid sh -c "trap \"\" HUP; exec '"$D"'/epdc suspend-test --alarm '"$ALARM"'" > '"$D"'/suspend-test.log 2>&1 < /dev/null &
sleep 2; echo "launched: $(pgrep -f "epdc suspend-test" | tr "\n" " ")"; head -n 3 '"$D"'/suspend-test.log
' 2>&1 | grep -v "Welcome to Kindle\|^$"
echo "== waiting for the test (the link should drop for ~$ALARM s and come back)"
t0=$(date +%s); back=0
for i in $(seq 1 40); do
  sleep 5
  if out=$(ssh -o ConnectTimeout=4 -o BatchMode=yes $H 'tail -n 1 /mnt/us/kindle-epdc/suspend-test.log 2>/dev/null; uptime' 2>/dev/null); then
    echo "t+$(( $(date +%s) - t0 ))s: $(echo "$out" | tr '\n' ' ' | cut -c1-160)"
    echo "$out" | grep -q "suspend-test done" && { back=1; break; }
  else
    echo "t+$(( $(date +%s) - t0 ))s: no answer (suspended?)"
  fi
done
[ $back -eq 1 ] || echo "TEST DID NOT REPORT DONE within $(( $(date +%s) - t0 )) s; if the device is dark, replug the USB cable, then run the restore steps by hand"
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
echo "== test log"; cat /mnt/us/kindle-epdc/suspend-test.log | grep -v "^  t+\|golden\|^  [A-Z_0-9]* *= 0x"
echo "== guard back"; mount | grep -q " /sys/power/state " || mount --bind /dev/null /sys/power/state; mount | grep -c " /sys/power/state "
echo "== kernel log"; dmesg | grep -i "refusing suspend\|PM: \|Restarting tasks\|Freezing\|epdc_uio\|rtc\|wakeup\|alarm" | tail -n 25
echo "== backstop: rails down regardless"; /mnt/us/kindle-epdc/epdc pmic down; echo "rc=$?"; echo "clocks attr: $(cat /sys/bus/platform/devices/mxc_epdc_fb/clocks)"
echo "== pmic after"; /mnt/us/kindle-epdc/epdc pmic dump | grep "ENABLE\|PG_GOOD\|EXT_TEMP"
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
echo "m7b done rc=${PIPESTATUS[0]} $(date +%T)"
