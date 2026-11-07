#!/usr/bin/env bash
# Undo scripts/hold.sh: lift the suspend guard and start powerd again. The milestone scripts restore the stock
# display driver themselves; the UI jobs they stop (`x`, `lab126_gui`, `framework`) come back with `/sbin/start`,
# or reboot the device to return it to the known boot state, which is the recommended end of a session.
H=${KINDLE_HOST:-kindle}
ssh -o ConnectTimeout=8 "$H" '
umount /sys/power/state 2>/dev/null && echo "suspend guard off"
/sbin/start powerd >/dev/null 2>&1 && echo "powerd started"
echo released $(date)
'
