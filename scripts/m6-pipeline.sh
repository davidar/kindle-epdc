#!/usr/bin/env bash
# Milestone 6: updates overlapping in time. One `epdc blit` run in pipelined mode (submit as soon as the working
# buffer is free, do not wait for earlier LUTs), three phases:
#   A  a GL16 strip, then immediately a DU strip overlapping it: the second collides, waits for the first LUT and
#      is resubmitted (stock's collision list, minus the "newer wins" drop);
#   B  four non-overlapping strips back to back: all four LUTs run together, the batch finishes in one GL16 time;
#   C  two lines typed one word at a time, 150 ms apart, each word its own rectangle and LUT.
# Strips rendered by render-text.py. CHECK=1 = dry run (bring-up and golden check only, rails never powered).
#   scripts/m6-pipeline.sh
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
D=/mnt/us/kindle-epdc
log=logs/m6-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m6 start $(date +%T) CHECK=${CHECK:-0}"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:$D/ || { echo "scp failed"; exit 1; }
WBF=$(/bin/ls panel/*.wbf 2>/dev/null | head -1); WRF=${WBF%.wbf}.wrf
[ -n "$WBF" ] || { echo "no panel/*.wbf: run scripts/fetch-panel.sh first"; exit 1; }
scp -O -q "$WBF" $H:$D/panel.wbf || { echo "scp wbf failed"; exit 1; }
[ -f "$WRF" ] && { scp -O -q "$WRF" $H:$D/panel.wrf || { echo "scp wrf failed"; exit 1; }; }
mkdir -p images/m6 && rm -f images/m6/*.pgm && R=scripts/render-text.py
OPS=""
add() { OPS="$OPS $D/m6/$(basename "$1")$2"; }  # $1 = local pgm@x,y from render-text.py, $2 = suffix (:mode, +ms)
# A: collision. The GL16 strip at rows 16-64 and a 1-bit strip at rows 40-88 share rows 40-64.
add "$($R "A: GL16 strip, then a DU strip overlapping it" images/m6/a1.pgm --row 16 --size 28 --grey)" ""
add "$($R "A: this one collided and was resubmitted" images/m6/a2.pgm --row 40 --size 28)" ""
# B: four non-overlapping strips, no delay between submits.
add "$($R "B: four strips submitted back to back" images/m6/b1.pgm --row 128 --size 28 --grey)" ""
add "$($R "B: all LUTs run in the same frames" images/m6/b2.pgm --row 184 --size 28)" ""
add "$($R "B: DU and GL16 side by side" images/m6/b3.pgm --row 240 --size 28 --grey)" ""
add "$($R "B: batch done in one GL16 time" images/m6/b4.pgm --row 296 --size 28)" ""
# C: words typed 150 ms apart, two lines.
for w in $($R "C: the quick brown fox jumps" images/m6/c1 --row 400 --size 28 --words); do add "$w" "+150"; done
for w in $($R "over the lazy dog, one LUT each" images/m6/c2 --row 456 --size 28 --words); do add "$w" "+150"; done
echo "ops:$OPS"
ssh $H "mkdir -p $D/m6" && scp -O -q images/m6/*.pgm $H:$D/m6/ || { echo "scp images failed"; exit 1; }
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 '"$D"'/epdc; md5sum '"$D"'/epdc '"$D"'/epdc_uio.ko '"$D"'/panel.wbf | cut -c1-8 | tr "\n" " "; echo
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && n=$((n+1)); done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
echo 3 > /proc/sys/vm/drop_caches
echo "== load shim"; /sbin/insmod '"$D"'/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
if [ -n "'"${CHECK:-}"'" ]; then echo "== dry run: white --check"; '"$D"'/epdc white --check; echo "rc=$?";
else echo "== blit (pipelined)"; '"$D"'/epdc blit '"$OPS"'; echo "rc=$?"; fi
echo "== backstop: rails down regardless"; '"$D"'/epdc pmic down; echo "rc=$?"
echo "== pmic after"; '"$D"'/epdc pmic dump | grep "ENABLE\|PG_GOOD\|EXT_TEMP"
[ -n "'"${CHECK:-}"'" ] || { echo "holding 20 s so the result can be seen before the stock splash"; sleep 20; }
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
echo "m6 done rc=${PIPESTATUS[0]} $(date +%T)"
