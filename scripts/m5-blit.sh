#!/usr/bin/env bash
# Milestone 5: partial updates. Renders the region PGMs into images/m5/, uploads them and runs one `epdc blit` sequence: INIT,
# a full-screen GC16 to white, then text strips alternating antialiased (auto -> GL16) and 1-bit (auto -> DU),
# one strip rewritten in PARTIAL mode, an inverted strip, and a GL16 grey ramp in a rectangle; each LUT timed.
# CHECK=1 = dry run (bring-up and golden check only, rails never powered). Same bring-up, golden check and rails
# sequence as m3/m4; stock restored through the boot display job.
#   scripts/m5-blit.sh
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
D=/mnt/us/kindle-epdc
log=logs/m5-$(date +%Y%m%d-%H%M%S).log
exec > >(tee "$log") 2>&1
echo "m5 start $(date +%T) CHECK=${CHECK:-0}"
ssh -o ConnectTimeout=8 $H 'mount | grep -q " /sys/power/state " && /sbin/initctl list | grep -q "powerd stop/waiting" && echo "hold ok" || { echo "NOT HELD: run scripts/hold.sh first (suspend guard + powerd stopped)"; exit 1; }' || exit 1
scp -O -q shim/epdc_uio.ko epdc/target/armv7-unknown-linux-musleabihf/release/epdc $H:$D/ || { echo "scp failed"; exit 1; }
WBF=$(/bin/ls panel/*.wbf 2>/dev/null | head -1); WRF=${WBF%.wbf}.wrf
[ -n "$WBF" ] || { echo "no panel/*.wbf: run scripts/fetch-panel.sh first"; exit 1; }
scp -O -q "$WBF" $H:$D/panel.wbf || { echo "scp wbf failed"; exit 1; }
[ -f "$WRF" ] && { scp -O -q "$WRF" $H:$D/panel.wrf || { echo "scp wrf failed"; exit 1; }; }
# Render the strips (portrait rows -> native x come from render-text.py) and the grey ramp.
mkdir -p images/m5 && R=scripts/render-text.py && {
  $R "antialiased text: auto picks GL16" images/m5/l1.pgm --row 16 --size 28 --grey &&
  $R "1-bit text: auto picks DU" images/m5/l2.pgm --row 72 --size 28 &&
  $R "antialiased again, GL16, ~600 ms" images/m5/l3.pgm --row 128 --size 28 --grey &&
  $R "1-bit again, DU, ~250 ms" images/m5/l4.pgm --row 184 --size 28 &&
  $R "line 1 rewritten in 1-bit, PARTIAL" images/m5/l1b.pgm --row 16 --size 28 &&
  $R "inverted, antialiased, GL16" images/m5/l5.pgm --row 240 --size 28 --grey --invert &&
  python3 -c "
from PIL import Image
im = Image.new('L', (320, 160))
for x in range(320):
    for y in range(160): im.putpixel((x, y), (x * 16 // 320) * 17)
im.save('images/m5/ramp.pgm')"; } >/dev/null || { echo "rendering failed (needs python3 with Pillow)"; exit 1; }
ssh $H "mkdir -p $D/m5" && scp -O -q images/m5/*.pgm $H:$D/m5/ || { echo "scp images failed"; exit 1; }
# The sequence: <pgm>@<x>,<y>[:<mode>|:auto][:full|partial]; the mode defaults to auto (DU for black-and-white regions, GL16 otherwise). Coordinates from render-text.py; the ramp sits in portrait rows 320-640.
OPS="$D/m5/l1.pgm@736,0 $D/m5/l2.pgm@680,0 $D/m5/l3.pgm@624,0 $D/m5/l4.pgm@568,0 $D/m5/l1b.pgm@736,0:partial $D/m5/l5.pgm@512,0 $D/m5/ramp.pgm@160,144:partial"
ssh -o ConnectTimeout=8 $H '
export PATH=/usr/sbin:/usr/bin:/bin:/sbin
chmod 755 '"$D"'/epdc; md5sum '"$D"'/epdc '"$D"'/epdc_uio.ko '"$D"'/panel.wbf '"$D"'/m5/*.pgm | cut -c1-8 | tr "\n" " "; echo
/sbin/stop x >/dev/null 2>&1 || true; sleep 3; /sbin/stop lab126_gui >/dev/null 2>&1 || true; /sbin/stop framework >/dev/null 2>&1 || true; sleep 2
n=0; for p in /proc/[0-9]*; do /bin/ls -l $p/fd 2>/dev/null | grep -q "/dev/fb0" && n=$((n+1)); done; [ $n -eq 0 ] || { echo "fb0 still open, refusing"; exit 1; }
echo "== unload stock"; /sbin/rmmod mxc_epdc_fb && echo "mxc_epdc_fb unloaded" || { echo "rmmod failed"; exit 1; }
# Both drivers want multi-MiB contiguous DMA buffers (the stock one an order-10 block in the DMA zone); after
# hours of uptime the page cache fragments memory and a probe fails with ENOMEM (seen 8 Nov 2026). Dropping the
# cache before each load is cheap and gives the buddy allocator large blocks back.
echo 3 > /proc/sys/vm/drop_caches
echo "== load shim"; /sbin/insmod '"$D"'/epdc_uio.ko && echo "epdc_uio loaded" || { echo "insmod failed"; /etc/upstart/display; exit 1; }
if [ -n "'"${CHECK:-}"'" ]; then echo "== dry run: white --check"; '"$D"'/epdc white --check; echo "rc=$?";
else echo "== blit"; '"$D"'/epdc blit '"$OPS"'; echo "rc=$?"; fi
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
echo "m5 done rc=${PIPESTATUS[0]} $(date +%T)"
