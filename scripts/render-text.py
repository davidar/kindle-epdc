#!/usr/bin/env python3
"""Render one line of text as a region PGM for `epdc blit`, in panel-native orientation.

The Kindle Touch panel is 800x600 to the EPDC and is read in portrait (600 wide, 800 tall): a portrait image
rotated 90 degrees clockwise is what the controller scans (milestone 4). A line of portrait text therefore
becomes a vertical strip in native coordinates. Given a portrait row (the top of the line, multiple of 8) and a
line height (multiple of 8), this writes the strip and prints the blit argument `<pgm>@<x>,<y>`.

  scripts/render-text.py "text" out.pgm --row 16 [--height 48] [--size 36] [--font path.ttf] [--invert] [--grey]
"""
import argparse, sys
from PIL import Image, ImageDraw, ImageFont

XRES, YRES = 800, 600  # native

ap = argparse.ArgumentParser()
ap.add_argument("text"); ap.add_argument("out")
ap.add_argument("--row", type=int, required=True, help="portrait row of the top of the line (multiple of 8)")
ap.add_argument("--height", type=int, default=48, help="line height in portrait rows (multiple of 8)")
ap.add_argument("--size", type=int, default=36, help="font pixel size")
ap.add_argument("--font", default="/usr/share/fonts/google-noto-vf/NotoSansMono[wght].ttf")
ap.add_argument("--invert", action="store_true", help="white on black")
ap.add_argument("--grey", action="store_true", help="keep antialiased edges (for GC16/GL16); default thresholds to 1-bit for DU")
a = ap.parse_args()
if a.row % 8 or a.height % 8 or a.row + a.height > XRES:
    sys.exit("row and height must be multiples of 8 and fit in 800 portrait rows")

bg, fg = (0, 255) if a.invert else (255, 0)
portrait = Image.new("L", (YRES, a.height), bg)   # one line, full portrait width
d = ImageDraw.Draw(portrait)
try:
    font = ImageFont.truetype(a.font, a.size)
except OSError:
    font = ImageFont.load_default(size=a.size)
d.text((8, a.height // 2), a.text, fill=fg, font=font, anchor="lm")
if not a.grey:
    # DU is a two-level waveform: an antialiased edge pixel asks it for a grey it cannot draw and the edges come
    # out ragged (seen 8 Nov 2026). Threshold at mid-grey.
    portrait = portrait.point(lambda v: 255 if v >= 128 else 0)
native = portrait.transpose(Image.Transpose.ROTATE_270)  # 90 degrees clockwise: (height x 600)
assert native.size == (a.height, YRES)
native.save(a.out)  # Pillow writes mode L as binary P5 PGM
# Portrait rows [row, row+height) map to native x in [800-row-height, 800-row); portrait x spans all native y.
x = XRES - a.row - a.height
print(f"{a.out}@{x},0")
