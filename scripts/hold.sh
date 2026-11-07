#!/usr/bin/env bash
# Hold the Kindle awake for a driver session.
#
# On 5.3.7 firmware the stock power daemon (the `powerd` upstart job) suspends the device on its own schedule:
# about a minute after a button wake and within a few minutes of boot, regardless of the UI's screensaver
# setting, and /sys/power/wake_lock does not exist on this kernel. A suspended Kindle drops its USB gadget and
# answers nothing, and a button wake does not re-enumerate USB (only a cable replug or a reboot does).
# So: stop powerd for the session, and bind-mount /dev/null over /sys/power/state so that any remaining
# "echo mem > /sys/power/state" from anything else goes nowhere. Both are undone by scripts/release.sh or by a
# reboot. Side effect while held: the power button does nothing, since powerd services it.
H=${KINDLE_HOST:-kindle}
ssh -o ConnectTimeout=8 "$H" '
/sbin/stop powerd >/dev/null 2>&1 && echo "powerd stopped" || echo "powerd already stopped"
if ! mount | grep -q " /sys/power/state "; then mount --bind /dev/null /sys/power/state && echo "suspend guard on"; else echo "suspend guard already on"; fi
echo held $(date)
'
