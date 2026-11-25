# VideoSqueeze font: Regular and Bold into ui/fonts (TrueType via fontTools) and a sample sheet sample.png here.
# Run: python tools/font/make_font.py
import os
import sys

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from PIL import Image, ImageDraw, ImageFont

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import glyphs  # noqa: E402

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "..", "ui", "fonts"))
FAMILY = "VideoSqueeze"
REGULAR_STROKE = 88
STYLES = [("Regular", REGULAR_STROKE, 400, 56), ("Bold", 124, 700, 52)]   # style, stroke, weight, side bearing


def glyph_of(contours, shift):
    pen = TTGlyphPen(None)
    for c in contours:
        pts = [(round(x + shift), round(y)) for x, y in c]
        clean = [p for i, p in enumerate(pts) if p != pts[i - 1]]
        if len(clean) < 3:
            continue
        pen.moveTo(clean[0])
        for p in clean[1:]:
            pen.lineTo(p)
        pen.closePath()
    return pen.glyph()


def build(style, stroke, weight, side):
    items = glyphs.build(stroke, stroke - REGULAR_STROKE)
    order = [".notdef", "space"]
    cmap = {0x20: "space", 0xA0: "space"}
    glyf = {}
    hmtx = {}

    # .notdef — an empty box
    nd = glyphs.Glyph(stroke)
    nd.loop((stroke / 2, stroke / 2), (stroke / 2, glyphs.CAP - stroke / 2), (400 - stroke / 2, glyphs.CAP - stroke / 2), (400 - stroke / 2, stroke / 2))
    glyf[".notdef"] = glyph_of(nd.contours, side)
    hmtx[".notdef"] = (400 + 2 * side, side)
    glyf["space"] = TTGlyphPen(None).glyph()
    hmtx["space"] = (glyphs.SPACE, 0)

    for codes, W, contours in items:
        name = "uni%04X" % codes[0]
        order.append(name)
        for c in codes:
            cmap[c] = name
        glyf[name] = glyph_of(contours, side)
        xs = [round(x + side) for cont in contours for x, _ in cont]
        hmtx[name] = (round(W + 2 * side), min(xs) if xs else 0)

    fb = FontBuilder(1000, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap(cmap)
    fb.setupGlyf(glyf)
    fb.setupHorizontalMetrics(hmtx)
    fb.setupHorizontalHeader(ascent=900, descent=-250)
    fb.setupNameTable({
        "familyName": FAMILY,
        "styleName": style,
        "uniqueFontIdentifier": f"{FAMILY}-{style};1.000",
        "fullName": f"{FAMILY} {style}",
        "psName": f"{FAMILY}-{style}",
        "version": "Version 1.000",
    })
    fb.setupOS2(
        version=4,
        usWeightClass=weight,
        fsSelection=(0x20 if weight >= 700 else 0x40) | 0x80,
        sTypoAscender=900, sTypoDescender=-250, sTypoLineGap=0,
        usWinAscent=950, usWinDescent=280,
        sxHeight=glyphs.XH, sCapHeight=glyphs.CAP,
        achVendID="NONE",
        ulCodePageRange1=(1 << 0) | (1 << 2),
    )
    fb.font["OS/2"].recalcUnicodeRanges(fb.font)
    fb.font["head"].macStyle = 1 if weight >= 700 else 0
    fb.setupPost(keepGlyphNames=False)
    os.makedirs(OUT, exist_ok=True)
    path = os.path.join(OUT, f"{FAMILY}-{style}.ttf")
    fb.save(path)
    return path, len(order)


LINES = [
    "VIDEOSQUEEZE // COMPRESS · CONVERT",
    "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
    "abcdefghijklmnopqrstuvwxyz",
    "0123456789  1920×1080  29.97 FPS  2.4 MBIT/S  −63%",
    "АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ",
    "абвгдеёжзийклмнопрстуфхцчшщъыьэюя",
    ".,:;!?-–—_/\\|()[]+=×÷±#%'\"·•→▲▼«»<>°*…№",
    "Holiday_2024 — Beach (Final Cut).mp4  1:23:45",
]


def sample(paths):
    img = Image.new("RGB", (1500, 1180), (10, 12, 27))
    d = ImageDraw.Draw(img)
    y = 20
    for path, color in zip(paths, [(252, 238, 10), (5, 217, 232)]):
        for size in (44, 26):
            f = ImageFont.truetype(path, size)
            for line in LINES:
                d.text((24, y), line, font=f, fill=color)
                y += int(size * 1.25)
            y += 8
        f = ImageFont.truetype(path, 13)
        d.text((24, y), "SMALL 13 PX: ENCODING 11 / 12 · 48 FPS · 2.1× REALTIME · Holiday clip", font=f, fill=(223, 246, 255))
        y += 30
    img.save(os.path.join(HERE, "sample.png"))


if __name__ == "__main__":
    made = []
    for style, stroke, weight, side in STYLES:
        path, n = build(style, stroke, weight, side)
        print(f"{os.path.basename(path)}: {n} glyphs, {os.path.getsize(path)} bytes")
        made.append(path)
    sample(made)
