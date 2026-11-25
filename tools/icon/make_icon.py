# VideoSqueeze icon and NSIS installer pictures.
# Icon: dark plate with cut corners and a yellow edge; a play triangle squeezed between two arrowheads,
# drawn with colour dispersion (pink and cyan copies shifted aside, yellow on top). Every size is drawn
# anew at its own scale (4x oversampling) so 16 and 32 px stay crisp.
# Run: python tools/icon/make_icon.py  (from any folder)
import os
from PIL import Image, ImageDraw, ImageFont, ImageChops

HERE = os.path.dirname(os.path.abspath(__file__))
ICONS = os.path.normpath(os.path.join(HERE, "..", "..", "icons"))
os.makedirs(ICONS, exist_ok=True)

YELLOW = (252, 238, 10)
PINK = (255, 42, 109)
CYAN = (5, 217, 232)
NAVY = (11, 13, 32)
NAVY2 = (24, 20, 70)

SS = 4  # oversampling factor


def plate_points(s, cut_big, cut_small, inset=0.0):
    i = inset
    return [
        (cut_big + i * 0.41, i), (s - cut_small - i * 0.41, i), (s - i, cut_small + i * 0.41),
        (s - i, s - cut_big - i * 0.41), (s - cut_big - i * 0.41, s - i), (cut_small + i * 0.41, s - i),
        (i, s - cut_small - i * 0.41), (i, cut_big + i * 0.41),
    ]


def squeeze_shapes(s):
    # left arrowhead pointing right, play triangle, right arrowhead pointing left
    left = [(0.12, 0.30), (0.27, 0.50), (0.12, 0.70)]
    right = [(0.88, 0.30), (0.73, 0.50), (0.88, 0.70)]
    play = [(0.38, 0.30), (0.38, 0.70), (0.63, 0.50)]
    sc = lambda pts: [(x * s, y * s) for x, y in pts]
    return sc(left), sc(right), sc(play)


def draw_icon(size):
    s = size * SS
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    big, small = s * 0.22, s * 0.09
    border = max(s * 0.045, SS * 1.2)
    d.polygon(plate_points(s, big, small), fill=YELLOW)
    d.polygon(plate_points(s, big, small, border), fill=NAVY)
    # gradient band at the bottom
    grad = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    gd = ImageDraw.Draw(grad)
    for y in range(int(s * 0.55), s):
        a = int(110 * (y - s * 0.55) / (s * 0.45))
        gd.line([(0, y), (s, y)], fill=NAVY2 + (a,))
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).polygon(plate_points(s, big, small, border), fill=255)
    img.paste(grad, (0, 0), ImageChops.multiply(mask, grad.split()[3]))
    # tab at the top right
    d.polygon([(s * 0.60, border), (s * 0.80, border), (s * 0.76, border + s * 0.06), (s * 0.64, border + s * 0.06)], fill=YELLOW)

    width = max(int(s * 0.07), SS * 2)
    shift = max(s * 0.03, SS * 0.9)
    left, right, play = squeeze_shapes(s)

    def paint(d, dx, fill_play):
        d.line([(x + dx, y) for x, y in left], fill=255, width=width, joint="curve")
        d.line([(x + dx, y) for x, y in right], fill=255, width=width, joint="curve")
        d.polygon([(x + dx, y) for x, y in play], fill=255 if fill_play else 0)

    # pink and cyan copies add up, the yellow core goes on top
    layer = Image.new("RGB", (s, s), (0, 0, 0))
    for color, dx in ((PINK, -shift), (CYAN, shift)):
        m = Image.new("L", (s, s), 0)
        paint(ImageDraw.Draw(m), dx, True)
        part = Image.new("RGB", (s, s), (0, 0, 0))
        part.paste(color, (0, 0, s, s), m)
        layer = ImageChops.add(layer, part)
    lmask = layer.convert("L").point(lambda v: 255 if v > 8 else 0)
    img.paste(layer, (0, 0), ImageChops.multiply(lmask, mask))
    core = Image.new("L", (s, s), 0)
    cd = ImageDraw.Draw(core)
    w2 = max(int(width * 0.66), SS)
    cd.line(left, fill=255, width=w2, joint="curve")
    cd.line(right, fill=255, width=w2, joint="curve")
    inset = [(play[0][0] + shift * 0.6, play[0][1] + shift), (play[1][0] + shift * 0.6, play[1][1] - shift), (play[2][0] - shift, play[2][1])]
    cd.polygon(inset, fill=255)
    img.paste(YELLOW, (0, 0, s, s), ImageChops.multiply(core, mask))
    return img.resize((size, size), Image.LANCZOS)


def font(px):
    for name in ("bahnschrift.ttf", "segoeuib.ttf", "arialbd.ttf"):
        path = os.path.join(os.environ.get("WINDIR", "C:\\Windows"), "Fonts", name)
        if os.path.exists(path):
            return ImageFont.truetype(path, px)
    return ImageFont.load_default()


def scanlines(img, step=3, alpha=38):
    over = Image.new("RGBA", img.size, (0, 0, 0, 0))
    od = ImageDraw.Draw(over)
    for y in range(0, img.size[1], step):
        od.line([(0, y), (img.size[0], y)], fill=(0, 0, 0, alpha))
    return Image.alpha_composite(img, over)


def dispersed_text(img, xy, text, f, base=YELLOW, shift=2):
    d = ImageDraw.Draw(img)
    x, y = xy
    d.text((x - shift, y), text, font=f, fill=PINK + (200,))
    d.text((x + shift, y), text, font=f, fill=CYAN + (200,))
    d.text((x, y), text, font=f, fill=base + (255,))


def nsis_sidebar():
    w, h = 164, 314
    img = Image.new("RGBA", (w, h), NAVY + (255,))
    d = ImageDraw.Draw(img)
    for y in range(h):
        t = y / h
        d.line([(0, y), (w, y)], fill=(int(11 + 18 * t), int(13 + 6 * t), int(32 + 50 * t), 255))
    # slanted stripes at the bottom, below the text
    for i, c in enumerate((PINK, CYAN, YELLOW)):
        x0 = 60 + i * 16
        d.polygon([(x0, h), (x0 + 9, h), (x0 + 9 + 44, h - 44), (x0 + 44, h - 44)], fill=c + (255,))
    icon = draw_icon(96)
    img.alpha_composite(icon, ((w - 96) // 2, 26))
    dispersed_text(img, (18, 140), "VIDEO", font(40))
    dispersed_text(img, (18, 182), "SQUEEZE", font(30), base=(240, 240, 255))
    d.rectangle([(18, 232), (w - 18, 234)], fill=YELLOW + (255,))
    d.text((18, 242), "COMPRESS  //  CONVERT", font=font(12), fill=CYAN + (255,))
    img = scanlines(img)
    img.convert("RGB").save(os.path.join(ICONS, "nsis-sidebar.bmp"))


def nsis_header():
    w, h = 150, 57
    img = Image.new("RGBA", (w, h), NAVY + (255,))
    d = ImageDraw.Draw(img)
    # NSIS draws the header right of its own title: only the mark and stripes
    d.polygon([(w - 70, 0), (w, 0), (w, h), (w - 48, h)], fill=YELLOW + (255,))
    d.polygon([(w - 80, 0), (w - 74, 0), (w - 52, h), (w - 58, h)], fill=PINK + (255,))
    d.polygon([(w - 90, 0), (w - 84, 0), (w - 62, h), (w - 68, h)], fill=CYAN + (255,))
    icon = draw_icon(44)
    img.alpha_composite(icon, (w - 50, 6))
    img = scanlines(img, alpha=30)
    img.convert("RGB").save(os.path.join(ICONS, "nsis-header.bmp"))


def main():
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    images = {s: draw_icon(s) for s in sizes + [512]}
    # sizes stored as classic BMP: every part of Windows reads them (shortcuts, taskbar, Explorer);
    # small PNG entries in .ico are ignored in places and a shortcut shows the default icon
    images[256].save(os.path.join(ICONS, "icon.ico"), sizes=[(s, s) for s in sizes],
                     append_images=[images[s] for s in sizes if s != 256], bitmap_format="bmp")
    images[32].save(os.path.join(ICONS, "32x32.png"))
    images[128].save(os.path.join(ICONS, "128x128.png"))
    images[256].save(os.path.join(ICONS, "128x128@2x.png"))
    images[512].save(os.path.join(ICONS, "icon.png"))
    nsis_sidebar()
    nsis_header()
    # sheet with every size for a look
    sheet = Image.new("RGBA", (sum(sizes) + 20 * len(sizes) + 20, 300), (40, 40, 48, 255))
    x = 20
    for s in sizes:
        sheet.alpha_composite(images[s], (x, 20))
        x += s + 20
    sheet.save(os.path.join(HERE, "preview.png"))
    print("icons:", ICONS)


if __name__ == "__main__":
    main()
