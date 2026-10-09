#!/usr/bin/env python3
"""Gives Chakra Petch a slashed zero, so 0 and O differ as they do on a
terminal (the font has no such alternate). Run on the fonts from Google
Fonts (ofl/chakrapetch); writes over the copies in the client's assets.

usage: scripts/slash-zero.py ChakraPetch-Regular.ttf ChakraPetch-SemiBold.ttf

The SIL Open Font License allows changed versions; Chakra Petch reserves
no font name. Needs fonttools (pip install fonttools).
"""
import pathlib
import sys

from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

root = pathlib.Path(__file__).resolve().parent.parent
fonts = root / "crates/nullnet-client/assets/fonts"
if len(sys.argv) < 2:
    sys.exit(__doc__)

for source in sys.argv[1:]:
    font = TTFont(source)
    glyf = font["glyf"]
    zero = glyf["zero"]
    zero.recalcBounds(glyf)
    left, bottom, right, top = zero.xMin, zero.yMin, zero.xMax, zero.yMax
    width, height = right - left, top - bottom
    # The bar runs corner to corner inside the counter and just into the
    # ring, as thick as a little under a stem.
    thick = width * 0.16
    x0, x1 = left + width * 0.2, right - width * 0.2
    y0, y1 = bottom + height * 0.16, top - height * 0.16

    pen = TTGlyphPen(font.getGlyphSet())
    font.getGlyphSet()["zero"].draw(pen)
    # Clockwise, as TrueType's outer contours run, so it fills.
    pen.moveTo((x0, y0))
    pen.lineTo((x1 - thick, y1))
    pen.lineTo((x1, y1))
    pen.lineTo((x0 + thick, y0))
    pen.closePath()
    glyf["zero"] = pen.glyph()
    out = fonts / pathlib.Path(source).name
    font.save(out)
    print(f"{pathlib.Path(source).name}: slashed zero -> {out.relative_to(root)}")
