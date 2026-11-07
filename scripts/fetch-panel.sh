#!/usr/bin/env bash
# Copy this Kindle's panel files into panel/ (gitignored: the waveform is E Ink's data for that one panel):
#   /var/local/eink/panel/<name>.wbf   the waveform as read from the panel's SPI flash by the boot job
#   /var/local/eink/panel/<name>.wrf   the same converted by Amazon's `eu extract` (the driver checks its own
#                                      conversion against this; it is regenerated on the device if missing)
#   panel_id, version                  the panel's identity
# Needs root ssh to the Kindle (KINDLE_HOST, default `kindle`). Nothing is written to the device.
set -u
cd "$(dirname "$0")/.."
H=${KINDLE_HOST:-kindle}
mkdir -p panel
ssh -o ConnectTimeout=8 "$H" '/bin/ls -la /var/local/eink/panel/; cat /var/local/eink/panel/panel_id; echo; if ! /bin/ls /var/local/eink/panel/*.wrf >/dev/null 2>&1; then w=$(/bin/ls /var/local/eink/panel/*.wbf | head -1); /usr/sbin/eu extract -o "${w%.wbf}.wrf" "$w" && echo "generated ${w%.wbf}.wrf"; fi' || exit 1
scp -O -q "$H:/var/local/eink/panel/*" panel/ || { echo "scp failed"; exit 1; }
chmod 644 panel/* 2>/dev/null
/bin/ls -la panel/
echo "panel files fetched; the milestone scripts and 'epdc wbf' use panel/*.wbf (and compare with panel/*.wrf)"
