#!/usr/bin/env python3
"""Build Madrasa's brand SVGs from code (the brand sheet: ../Madrasa_Brand_Identity_Showcase.png).

The emblem is drawn here as geometry; the wordmark and tagline are set in Merriweather Black
and Inter Medium and turned into outlines, so the SVGs look the same everywhere without the
fonts installed (README on GitHub, PDFs, installers).

    python3 -m venv .venv && .venv/bin/pip install fonttools
    npm pack @fontsource/merriweather@5 @fontsource/inter@5   # the two font packages
    .venv/bin/python assets/brand/tools/build_svg.py --fonts <folder with the unpacked packages>

Writes into assets/brand/: madrasa-icon.svg (the app icon; the source of every icon size),
madrasa-mark.svg (the emblem alone), madrasa-logo.svg / madrasa-logo-dark.svg (horizontal,
for light and dark backgrounds) and madrasa-logo-vertical.svg / madrasa-logo-vertical-dark.svg.
Then run assets/brand/generate-icons.sh for the icon files.
"""

from __future__ import annotations

import argparse
import glob
import os

from fontTools.pens.svgPathPen import SVGPathPen
from fontTools.pens.transformPen import TransformPen
from fontTools.ttLib import TTFont

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.dirname(HERE)

# Brand colours (also the Tailwind tokens: apps/desktop/src/styles/tokens.css).
NAVY = "#0B2557"  # wordmark, tile
NAVY_DEEP = "#06204D"
NAVY_TOP = "#1C4F8E"
BLUE = "#0757BA"  # book, primary actions
BLUE_LIGHT = "#8CC8FA"
GOLD = "#F5B13B"
GOLD_DEEP = "#E39A24"
SLATE = "#2F4670"  # tagline on light
WHITE = "#FFFFFF"

# The emblem's bounds on its 1024 grid (x 100-924, y 60-900).
MARK_W, MARK_H = 824, 840


def emblem_defs(prefix: str) -> str:
    return f"""
    <linearGradient id="{prefix}inside" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{NAVY_TOP}"/>
      <stop offset="1" stop-color="{NAVY_DEEP}"/>
    </linearGradient>
    <linearGradient id="{prefix}gold" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{GOLD}"/>
      <stop offset="1" stop-color="{GOLD_DEEP}"/>
    </linearGradient>
    <linearGradient id="{prefix}page" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#1D6FD6"/>
      <stop offset="1" stop-color="#0A3F95"/>
    </linearGradient>
    <linearGradient id="{prefix}paper" x1="0" y1="0" x2="1" y2="0">
      <stop offset="0" stop-color="#BFE0FD"/>
      <stop offset="0.3" stop-color="{WHITE}"/>
      <stop offset="0.7" stop-color="{WHITE}"/>
      <stop offset="1" stop-color="#BFE0FD"/>
    </linearGradient>"""


# The arch's centre line: an ogee dome on two straight sides, open at the bottom (the book
# covers it). Coordinates are on a 1024 grid; the emblem spans about x 120-904, y 70-900.
ARCH = (
    "M 244 730 L 244 478 C 244 372 322 316 410 274 C 462 249 496 228 512 196 "
    "C 528 228 562 249 614 274 C 702 316 780 372 780 478 L 780 730"
)
ARCH_FILL = ARCH + " Z"


def crescent(cx: float, cy: float, r: float) -> str:
    """A crescent opening up and to the right: a disc less a smaller, offset disc."""
    import math

    r2 = r * 0.82
    ox, oy = cx + r * 0.46, cy - r * 0.16  # the cut-out disc's centre
    d = math.hypot(ox - cx, oy - cy)
    a = (r * r - r2 * r2 + d * d) / (2 * d)
    h = math.sqrt(r * r - a * a)
    mx, my = cx + a * (ox - cx) / d, cy + a * (oy - cy) / d
    p1 = (mx + h * (oy - cy) / d, my - h * (ox - cx) / d)
    p2 = (mx - h * (oy - cy) / d, my + h * (ox - cx) / d)
    return (
        f"M {p1[0]:.1f} {p1[1]:.1f} A {r:g} {r:g} 0 1 0 {p2[0]:.1f} {p2[1]:.1f} "
        f"A {r2:g} {r2:g} 0 1 1 {p1[0]:.1f} {p1[1]:.1f} Z"
    )


def band(top_c: float, top_o: float, x_top: float, bot_c: float, bot_o: float, x_bot: float) -> str:
    """One layer of the open book: a V-shaped band from the spine (x 512) out to both edges.
    `top_c`/`bot_c` are its top and bottom at the spine, `top_o`/`bot_o` at the left edge,
    which is at `x_top` (top) and `x_bot` (bottom); the right side mirrors the left."""

    def half(yc: float, yo: float, xo: float, reverse: bool = False) -> tuple[str, str]:
        # A curve from the spine to the edge that leaves the spine steeply and flattens out.
        c1 = (512 - (512 - xo) * 0.28, yc - (yc - yo) * 0.62)
        c2 = (512 - (512 - xo) * 0.62, yo + (yc - yo) * 0.06)
        return (
            f"C {c1[0]:.0f} {c1[1]:.0f} {c2[0]:.0f} {c2[1]:.0f} {xo:.0f} {yo:.0f}",
            f"C {c2[0]:.0f} {c2[1]:.0f} {c1[0]:.0f} {c1[1]:.0f} 512 {yc:.0f}",
        )

    def mirror(x: float) -> float:
        return 1024 - x

    top_out, _ = half(top_c, top_o, x_top)
    _, bot_in = half(bot_c, bot_o, x_bot)
    # Right half: mirror the control points.
    rc1 = (mirror(512 - (512 - x_top) * 0.28), top_c - (top_c - top_o) * 0.62)
    rc2 = (mirror(512 - (512 - x_top) * 0.62), top_o + (top_c - top_o) * 0.06)
    bc1 = (mirror(512 - (512 - x_bot) * 0.28), bot_c - (bot_c - bot_o) * 0.62)
    bc2 = (mirror(512 - (512 - x_bot) * 0.62), bot_o + (bot_c - bot_o) * 0.06)
    return (
        f"M 512 {top_c:.0f} {top_out} L {x_bot:.0f} {bot_o:.0f} {bot_in} "
        f"C {bc1[0]:.0f} {bc1[1]:.0f} {bc2[0]:.0f} {bc2[1]:.0f} {mirror(x_bot):.0f} {bot_o:.0f} "
        f"L {mirror(x_top):.0f} {top_o:.0f} "
        f"C {rc2[0]:.0f} {rc2[1]:.0f} {rc1[0]:.0f} {rc1[1]:.0f} 512 {top_c:.0f} Z"
    )


def emblem(prefix: str, pages: str = "blue") -> str:
    """The emblem on a 1024 grid, as SVG elements. `pages`: "blue" on light backgrounds (the
    logo), "white" on the night-blue tile (the app icon), as on the brand sheet."""
    g = f"url(#{prefix}gold)"
    if pages == "white":
        page_fill, fold_fill, fold_opacity = f"url(#{prefix}paper)", BLUE, "0.6"
    else:
        page_fill, fold_fill, fold_opacity = f"url(#{prefix}page)", BLUE_LIGHT, "0.9"
    return f"""
  <g>
    <!-- crescent finial -->
    <path d="{crescent(500, 116, 54)}" fill="{g}"/>
    <!-- dome: gold band, white rim, night-blue inside -->
    <path d="{ARCH}" fill="none" stroke="{g}" stroke-width="84" stroke-linejoin="round"/>
    <path d="{ARCH_FILL}" fill="url(#{prefix}inside)"/>
    <path d="{ARCH}" fill="none" stroke="{WHITE}" stroke-width="14" stroke-linejoin="round"/>
    <!-- minaret: cap, gallery, lantern with its light, gallery, tapering shaft -->
    <path d="M 512 318 L 512 340" stroke="{WHITE}" stroke-width="8" stroke-linecap="round"/>
    <path d="M 474 398 C 474 362 494 342 512 334 C 530 342 550 362 550 398 Z" fill="{WHITE}"/>
    <rect x="462" y="396" width="100" height="18" rx="5" fill="{WHITE}"/>
    <path d="M 474 414 L 550 414 L 550 486 L 474 486 Z" fill="{WHITE}"/>
    <path d="M 497 478 L 497 444 C 497 432 505 424 512 419 C 519 424 527 432 527 444 L 527 478 Z" fill="{g}"/>
    <rect x="458" y="486" width="108" height="20" rx="5" fill="{WHITE}"/>
    <path d="M 470 506 L 554 506 L 578 760 L 446 760 Z" fill="{WHITE}"/>
    <!-- open book: gold covers' edge, a white line, the pages, a fold of light -->
    <path d="{band(744, 618, 150, 800, 672, 140)}" fill="{g}"/>
    <path d="{band(800, 672, 140, 820, 690, 136)}" fill="{WHITE}"/>
    <path d="{band(820, 690, 136, 900, 800, 106)}" fill="{page_fill}"/>
    <path d="{band(858, 752, 146, 880, 778, 138)}" fill="{fold_fill}" opacity="{fold_opacity}"/>
  </g>"""


def svg(width: float, height: float, body: str, defs: str = "", title: str = "Madrasa") -> str:
    return (
        f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width:g} {height:g}" '
        f'width="{width:g}" height="{height:g}" role="img" aria-label="{title}">\n'
        f"  <title>{title}</title>\n  <defs>{defs}\n  </defs>{body}\n</svg>\n"
    )


def text_path(font: TTFont, text: str, size: float, x: float, baseline: float, fill: str,
              tracking: float = 0.0) -> tuple[str, float]:
    """Outline `text` at `size` px with its left edge at x; returns (<path>, advance width)."""
    upm = font["head"].unitsPerEm
    scale = size / upm
    cmap = font.getBestCmap()
    gs = font.getGlyphSet()
    hmtx = font["hmtx"]
    kern = {}
    # Pair kerning from a simple 'kern' table when present (GPOS kerning is not applied).
    if "kern" in font:
        for table in font["kern"].kernTables:
            kern.update(table.kernTable)
    pen = SVGPathPen(gs, ntos=lambda v: f"{v:.1f}".rstrip("0").rstrip("."))
    cursor = 0.0
    prev = None
    for ch in text:
        name = cmap[ord(ch)]
        if prev is not None:
            cursor += kern.get((prev, name), 0)
        tpen = TransformPen(pen, (scale, 0, 0, -scale, x + cursor * scale, baseline))
        gs[name].draw(tpen)
        cursor += hmtx[name][0] + tracking / scale
        prev = name
    return f'<path d="{pen.getCommands()}" fill="{fill}"/>', cursor * scale


def find(fonts: str, pattern: str) -> TTFont:
    hits = glob.glob(os.path.join(fonts, "**", pattern), recursive=True)
    if not hits:
        raise SystemExit(f"font not found: {pattern} under {fonts}")
    return TTFont(sorted(hits)[0])


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--fonts", required=True, help="folder holding the unpacked font packages")
    args = ap.parse_args()
    serif = find(args.fonts, "merriweather-latin-900-normal.woff")
    sans = find(args.fonts, "inter-latin-500-normal.woff")

    def write(name: str, content: str) -> None:
        with open(os.path.join(OUT, name), "w", encoding="utf-8") as f:
            f.write(content)
        print("wrote", name)

    header = "\n  <!-- Generated by assets/brand/tools/build_svg.py. Edit the script, not this file. -->"

    # App icon: rounded night-blue tile, emblem a little smaller than the tile.
    tile = f"""
    <linearGradient id="tile-fill" x1="0" y1="0" x2="0.35" y2="1">
      <stop offset="0" stop-color="{NAVY_TOP}"/>
      <stop offset="1" stop-color="{NAVY_DEEP}"/>
    </linearGradient>"""
    icon_body = (
        header
        + f'\n  <rect id="tile" x="32" y="32" width="960" height="960" rx="216" fill="url(#tile-fill)"/>'
        + '\n  <g id="emblem" transform="translate(512 512) scale(0.86) translate(-512 -485)">'
        + emblem("i-", pages="white")
        + "\n  </g>"
    )
    write("madrasa-icon.svg", svg(1024, 1024, icon_body, tile + emblem_defs("i-")))

    # Emblem alone, cropped to its bounds (x 100-924, y 60-900).
    mark_body = header + '\n  <g transform="translate(-100 -60)">' + emblem("m-") + "\n  </g>"
    write("madrasa-mark.svg", svg(MARK_W, MARK_H, mark_body, emblem_defs("m-")))

    # Horizontal logo: emblem, then the wordmark and tagline.
    for suffix, word_fill, tag_fill in (("", NAVY, SLATE), ("-dark", WHITE, "#D6E2F5")):
        mark_h = MARK_H
        scale = 260 / mark_h
        word, ww = text_path(serif, "Madrasa", 132, 0, 0, word_fill, tracking=-1)
        tag, tw = text_path(sans, "School Information System", 47.5, 0, 0, tag_fill, tracking=0.2)
        x0 = MARK_W * scale + 40
        width = x0 + max(ww, tw) + 8
        body = (
            header
            + f'\n  <g transform="scale({scale:.5f}) translate(-100 -60)">'
            + emblem("h-")
            + "\n  </g>"
            + f'\n  <g transform="translate({x0:.1f} 162)">{word}</g>'
            + f'\n  <g transform="translate({x0 + 4:.1f} 232)">{tag}</g>'
        )
        write(f"madrasa-logo{suffix}.svg", svg(round(width), 260, body, emblem_defs("h-")))

        # Vertical logo: emblem above, centred text.
        word, ww = text_path(serif, "Madrasa", 150, 0, 0, word_fill, tracking=-1)
        tag, tw = text_path(sans, "School Information System", 50, 0, 0, tag_fill, tracking=0.2)
        width = max(ww, tw, 560) + 40
        ms = 420 / MARK_H
        mx = (width - MARK_W * ms) / 2
        body = (
            header
            + f'\n  <g transform="translate({mx:.1f} 0) scale({ms:.5f}) translate(-100 -60)">'
            + emblem("v-")
            + "\n  </g>"
            + f'\n  <g transform="translate({(width - ww) / 2:.1f} 568)">{word}</g>'
            + f'\n  <g transform="translate({(width - tw) / 2:.1f} 640)">{tag}</g>'
        )
        write(f"madrasa-logo-vertical{suffix}.svg", svg(round(width), 660, body, emblem_defs("v-")))


if __name__ == "__main__":
    main()
