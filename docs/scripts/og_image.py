#!/usr/bin/env python3
"""Rasterise the site logo to the 1200x630 Open Graph image.

Reads docs/source/_static/logo.svg (and the mark it references) and writes
docs/source/_static/og.png. The canvas is the logo's own ink, with the
wordmark centered. Nothing here is drawn by hand.
"""

from __future__ import annotations

import io
import struct
import sys
from pathlib import Path

STATIC = Path(__file__).resolve().parents[1] / "source" / "_static"
LOGO = STATIC / "logo.svg"
MARK = STATIC / "mark.svg"
OUT = STATIC / "og.png"

WIDTH = 1200
HEIGHT = 630
# The logo's viewBox is 820 by 220, and the ink does not fill it.
# Render that viewBox, then center the ink on the canvas.
LOGO_W = 820.0
LOGO_H = 220.0
RENDER_W = 1640
MARGIN = 120


def _inner_svg(text: str) -> str:
    start = text.find("<svg")
    if start < 0:
        raise SystemExit("og_image: not an svg")
    open_end = text.find(">", start)
    close = text.rfind("</svg>")
    if open_end < 0 or close < 0:
        raise SystemExit("og_image: svg has no body")
    return text[open_end + 1 : close]


def logo_svg() -> str:
    logo = LOGO.read_text(encoding="utf-8")
    mark = MARK.read_text(encoding="utf-8")
    image = '<image href="mark.svg"'
    if image not in logo:
        raise SystemExit("og_image: logo.svg no longer references mark.svg")
    nested = (
        '<svg x="18" y="10" width="200" height="200" viewBox="0 0 200 200">'
        + _inner_svg(mark)
        + "</svg>"
    )
    # The image element is one tag. Replace only that, and keep the wordmark.
    logo_body = _inner_svg(logo)
    start = logo_body.find(image)
    end = logo_body.find("/>", start)
    if start < 0 or end < 0:
        raise SystemExit("og_image: could not inline mark.svg")
    logo_body = logo_body[:start] + nested + logo_body[end + 2 :]
    render_h = round(RENDER_W * LOGO_H / LOGO_W)
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{RENDER_W}" height="{render_h}" '
        f'viewBox="0 0 {LOGO_W:.0f} {LOGO_H:.0f}">{logo_body}</svg>'
    )


def _ink_box(image: object) -> tuple[int, int, int, int]:
    pixels = image.load()
    width, height = image.size
    minx, miny, maxx, maxy = width, height, 0, 0
    for y in range(height):
        for x in range(width):
            red, green, blue, alpha = pixels[x, y]
            if alpha < 10:
                continue
            if abs(red - 0x0B) + abs(green - 0x0B) + abs(blue - 0x0A) <= 18:
                continue
            minx = min(minx, x)
            miny = min(miny, y)
            maxx = max(maxx, x)
            maxy = max(maxy, y)
    if maxx < minx:
        raise SystemExit("og_image: logo rendered as empty ink")
    return minx, miny, maxx + 1, maxy + 1


def _png_size(data: bytes) -> tuple[int, int]:
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise SystemExit("og_image: output is not a png")
    return struct.unpack(">II", data[16:24])


def main() -> int:
    import cairosvg
    from PIL import Image

    rendered = cairosvg.svg2png(bytestring=logo_svg().encode("utf-8"))
    logo = Image.open(io.BytesIO(rendered)).convert("RGBA")
    box = _ink_box(logo)
    ink = logo.crop(box)
    room_w = WIDTH - 2 * MARGIN
    room_h = HEIGHT - 2 * MARGIN
    scale = min(room_w / ink.width, room_h / ink.height)
    placed = ink.resize(
        (max(1, round(ink.width * scale)), max(1, round(ink.height * scale))),
        Image.Resampling.LANCZOS,
    )
    canvas = Image.new("RGBA", (WIDTH, HEIGHT), (0x0B, 0x0B, 0x0A, 255))
    canvas.paste(
        placed,
        ((WIDTH - placed.width) // 2, (HEIGHT - placed.height) // 2),
        placed,
    )
    canvas.save(OUT, "PNG")
    png = OUT.read_bytes()
    width, height = _png_size(png)
    if (width, height) != (WIDTH, HEIGHT):
        raise SystemExit(f"og_image: expected {WIDTH}x{HEIGHT}, got {width}x{height}")
    print(f"og_image: wrote {OUT.relative_to(STATIC.parents[1])} ({width}x{height})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
