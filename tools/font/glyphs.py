# Letter skeletons of the font. Squared forms: straight strokes, cut corners (as on the program window —
# top left and bottom right), sharp joins. Coordinates are stroke centres; l, r, b, t — the letter edge
# moved in by half a stroke so the stroke stays inside. Heights: capitals 700, lowercase 520,
# ascenders 740 and descenders −210. Lowercase Cyrillic is small capitals (а е о р с у х as Latin).
import pen

CAP = 700
XH = 520
ASC = 740
DESC = -210


class Glyph:
    def __init__(self, stroke):
        self.S = stroke
        self.contours = []

    def line(self, *pts):
        self.contours += pen.line(pts, self.S)

    def loop(self, *pts):
        self.contours += pen.loop(pts, self.S)

    def rect(self, x0, y0, x1, y1):
        self.contours.append(pen.clockwise([(x0, y0), (x0, y1), (x1, y1), (x1, y0)]))

    def poly(self, *pts):
        self.contours.append(pen.clockwise(list(pts)))


def box(g, W, H):
    h = g.S / 2
    return h, W - h, h, H - h


def cut(H, W):
    """Large corner cut."""
    return min(0.22 * H, 0.3 * W)


# ── capitals (and lowercase where the form is the same) ───────────────

def A(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((l, 0), (l, t - K), (l + K, t), (r - K, t), (r, t - K), (r, 0))
    g.line((l, 0.4 * H), (r, 0.4 * H))


def B(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.55 * cut(H, W); m = 0.52 * H; rt = r - 0.08 * W
    g.line((l, 0), (l, H))
    g.line((l, t), (rt - k, t), (rt, t - k), (rt, m), (l, m))
    g.line((l, m), (r - k, m), (r, m - k), (r, b + k), (r - k, b), (l, b))


def C(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((W, t), (l + K, t), (l, t - K), (l, b), (W, b))


def D(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.loop((l, b), (l, t), (r - K, t), (r, t - K), (r, b + K), (r - K, b))


def E(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((W, t), (l + K, t), (l, t - K), (l, b), (W, b))
    g.line((l, H / 2), (0.86 * W, H / 2))


def F(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((W, t), (l + K, t), (l, t - K), (l, 0))
    g.line((l, H / 2), (0.84 * W, H / 2))


def G_(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((W, t), (l + K, t), (l, t - K), (l, b), (r, b), (r, H / 2), (0.52 * W, H / 2))


def H_(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, 0), (l, H)); g.line((r, 0), (r, H)); g.line((l, H / 2), (r, H / 2))


def I(g, W, H):
    g.line((g.S / 2, 0), (g.S / 2, H))


def J(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((r, H), (r, b + K), (r - K, b), (l, b), (l, 0.32 * H))


def K_(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, 0), (l, H))
    g.line((r, H), (r, 0.8 * H), (l, 0.47 * H))
    g.line((l, 0.47 * H), (r, 0.2 * H), (r, 0))


def L(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, b), (W, b))


def M(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, 0), (l, t), (W / 2, 0.36 * H), (r, t), (r, 0))


def N(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, 0), (l, t), (r, b), (r, H))


def O(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.loop((l, t - K), (l + K, t), (r, t), (r, b + K), (r - K, b), (l, b))


def P(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.7 * cut(H, W); p = 0.44 * H
    g.line((l, 0), (l, H))
    g.line((l, t), (r - k, t), (r, t - k), (r, p + 0.6 * k), (r - 0.6 * k, p), (l, p))


def Q(g, W, H):
    O(g, W, H)
    g.line((0.56 * W, 0.3 * H), (W + 20, -70))


def R(g, W, H):
    P(g, W, H)
    l, r, b, t = box(g, W, H)
    g.line((0.52 * W, 0.44 * H), (r, 0.24 * H), (r, 0))


def S_(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W); m = H / 2
    g.line((W, t), (l + K, t), (l, t - K), (l, m), (r, m), (r, b + K), (r - K, b), (0, b))


def T(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((0, t), (W, t)); g.line((W / 2, 0), (W / 2, t))


def U(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((l, H), (l, b), (r - K, b), (r, b + K), (r, H))


def V(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, 0.44 * H), (W / 2, b), (r, 0.44 * H), (r, H))


def W_(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, b), (W / 2, 0.62 * H), (r, b), (r, H))


def X(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, 0.78 * H), (r, 0.22 * H), (r, 0))
    g.line((r, H), (r, 0.78 * H), (l, 0.22 * H), (l, 0))


def Y(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, 0.72 * H), (W / 2, 0.46 * H), (W / 2, 0))
    g.line((r, H), (r, 0.72 * H), (W / 2, 0.46 * H))


def Z(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((0, t), (r, t), (r, 0.74 * H), (l, 0.26 * H), (l, b), (W, b))


# ── lowercase Latin ─────────────────────────────────────────────────────

def a(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W); m = 0.52 * H
    g.line((0.12 * W, t), (r - k, t), (r, t - k), (r, 0))
    g.line((r, m), (l, m), (l, b), (r, b))


def b_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((l, ASC), (l, 0))
    g.line((l, t), (r - k, t), (r, t - k), (r, b), (l, b))


def c(g, W, H):
    C(g, W, H)


def d(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((r, ASC), (r, 0))
    g.line((r, t), (l + k, t), (l, t - k), (l, b), (r, b))


def e(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W); m = 0.52 * H
    g.line((r, m), (r, t), (l + k, t), (l, t - k), (l, b), (W, b))
    g.line((l, m), (r, m))


def f(g, W, H):
    h = g.S / 2; k = 0.8 * cut(H, W); xs = 0.36 * W
    g.line((W, ASC - h), (xs + k, ASC - h), (xs, ASC - h - k), (xs, 0))
    g.line((0, H - h), (0.92 * W, H - h))


def g_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W); dc = DESC + g.S / 2
    g.line((r, t), (l + k, t), (l, t - k), (l, b), (r, b))
    g.line((r, H), (r, dc + k), (r - k, dc), (0, dc))


def h_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((l, ASC), (l, 0))
    g.line((l, t), (r - k, t), (r, t - k), (r, 0))


def i_(g, W, H):
    g.line((g.S / 2, 0), (g.S / 2, H))
    g.rect(0, H + 80, g.S, H + 80 + g.S)


def j_(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.7 * cut(H, W); dc = DESC + g.S / 2
    g.line((r, H), (r, dc + k), (r - k, dc), (0, dc))
    g.rect(r - g.S / 2, H + 80, r + g.S / 2, H + 80 + g.S)


def k_(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, ASC), (l, 0))
    g.line((r, H), (r, 0.8 * H), (l, 0.46 * H))
    g.line((l, 0.46 * H), (r, 0.2 * H), (r, 0))


def l_(g, W, H):
    g.line((g.S / 2, 0), (g.S / 2, ASC))


def m_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((l, 0), (l, H))
    g.line((l, t), (W / 2, t), (W / 2, 0))
    g.line((W / 2, t), (r - k, t), (r, t - k), (r, 0))


def n_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((l, 0), (l, H))
    g.line((l, t), (r - k, t), (r, t - k), (r, 0))


def p_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((l, H), (l, DESC))
    g.line((l, t), (r - k, t), (r, t - k), (r, b), (l, b))


def q_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((r, H), (r, DESC))
    g.line((r, t), (l + k, t), (l, t - k), (l, b), (r, b))


def r_(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.8 * cut(H, W)
    g.line((l, 0), (l, H))
    g.line((l, t), (r - k, t), (r, t - k), (r, t - k - 50))


def t_(g, W, H):
    h = g.S / 2; k = 0.8 * cut(H, W); xs = 0.36 * W
    g.line((xs, 0.9 * ASC), (xs, h + k), (xs + k, h), (W, h))
    g.line((0, H - h), (0.95 * W, H - h))


def u_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((l, H), (l, b + k), (l + k, b), (r, b))
    g.line((r, H), (r, 0))


def y_(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W); dc = DESC + g.S / 2
    g.line((l, H), (l, b + k), (l + k, b), (r, b))
    g.line((r, H), (r, dc + k), (r - k, dc), (0, dc))


# ── digits (one width) ──────────────────────────────────────────────────

def d0(g, W, H):
    O(g, W, H)
    g.line((0.36 * W, 0.34 * H), (0.64 * W, 0.66 * H))


def d1(g, W, H):
    l, r, b, t = box(g, W, H); xs = 0.6 * W
    g.line((xs - 0.34 * W, t - 0.3 * W), (xs, t), (xs, 0))


def d2(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((0, t), (r - K, t), (r, t - K), (r, H / 2), (l, H / 2), (l, b), (W, b))


def d3(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((0, t), (r - K, t), (r, t - K), (r, b + K), (r - K, b), (0, b))
    g.line((0.32 * W, H / 2), (r, H / 2))


def d4(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, 0.36 * H), (W, 0.36 * H))
    g.line((0.7 * W, 0.64 * H), (0.7 * W, 0))


def d5(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W); k = 0.5 * K; m = H / 2
    g.line((W, t), (l, t), (l, m), (r - k, m), (r, m - k), (r, b + K), (r - K, b), (0, b))


def d6(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W); m = H / 2
    g.line((W, t), (l + K, t), (l, t - K), (l, b), (r - K, b), (r, b + K), (r, m))
    g.line((r, m), (l, m))


def d7(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((0, t), (r, t), (r, 0.64 * H), (0.42 * W, 0.26 * H), (0.42 * W, 0))


def d8(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.55 * cut(H, W)
    g.loop((l, t - k), (l + k, t), (r - k, t), (r, t - k), (r, b + k), (r - k, b), (l + k, b), (l, b + k))
    g.line((l, H / 2), (r, H / 2))


def d9(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W); m = H / 2
    g.line((0, b), (r - K, b), (r, b + K), (r, t), (l + K, t), (l, t - K), (l, m))
    g.line((l, m), (r, m))


# ── Cyrillic (own forms) ────────────────────────────────────────────────

def BE(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W); m = 0.52 * H
    g.line((W, t), (l, t), (l, b), (r - K, b), (r, b + K), (r, m))
    g.line((r, m), (l, m))


def GE(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((W, t), (l, t), (l, 0))


def DE(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.6 * cut(H, W); xl = 0.2 * W
    g.line((xl, b), (xl, t - k), (xl + k, t), (r, t), (r, b))
    g.line((l, 0.75 * DESC), (l, b), (r, b), (r, 0.75 * DESC))


def ZHE(g, W, H):
    l, r, b, t = box(g, W, H); xc = W / 2; m = 0.47 * H
    g.line((xc, 0), (xc, H))
    g.line((r, H), (r, 0.8 * H), (xc, m)); g.line((xc, m), (r, 0.2 * H), (r, 0))
    g.line((l, H), (l, 0.8 * H), (xc, m)); g.line((xc, m), (l, 0.2 * H), (l, 0))


def I_(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, b), (r, t), (r, 0))


def EL(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, 0), (l, 0.22 * H), (0.32 * W, t), (r, t), (r, 0))


def PE(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, 0), (l, t), (r, t), (r, 0))


def U_cyr(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W); k = 0.5 * K; m = H / 2
    g.line((l, H), (l, m + k), (l + k, m), (r, m))
    g.line((r, H), (r, b + K), (r - K, b), (0, b))


def EF(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.5 * cut(H, W)
    g.line((W / 2, 0), (W / 2, H))
    y0, y1 = 0.2 * H, 0.8 * H
    g.loop((l, y1 - k), (l + k, y1), (r, y1), (r, y0 + k), (r - k, y0), (l, y0))


def TSE(g, W, H):
    l, r, b, t = box(g, W, H); rb = 0.84 * W
    g.line((l, H), (l, b), (r, b), (r, 0.75 * DESC))
    g.line((rb, H), (rb, b))


def CHE(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.5 * cut(H, W); m = H / 2
    g.line((l, H), (l, m + k), (l + k, m), (r, m))
    g.line((r, H), (r, 0))


def SHA(g, W, H):
    l, r, b, t = box(g, W, H)
    g.line((l, H), (l, b), (r, b), (r, H))
    g.line((W / 2, H), (W / 2, b))


def SHCHA(g, W, H):
    l, r, b, t = box(g, W, H); rb = 0.86 * W
    g.line((l, H), (l, b), (r, b), (r, 0.75 * DESC))
    g.line((rb, H), (rb, b))
    g.line(((l + rb) / 2, H), ((l + rb) / 2, b))


def HARD(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.6 * cut(H, W); xs = 0.28 * W; m = 0.52 * H
    g.line((0, t), (xs, t), (xs, b), (r - k, b), (r, b + k), (r, m))
    g.line((r, m), (xs, m))


def YERU(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.6 * cut(H, W); xs = 0.6 * W; m = 0.52 * H
    g.line((l, H), (l, b), (xs - k, b), (xs, b + k), (xs, m))
    g.line((xs, m), (l, m))
    g.line((r, H), (r, 0))


def SOFT(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.6 * cut(H, W); m = 0.52 * H
    g.line((l, H), (l, b), (r - k, b), (r, b + k), (r, m))
    g.line((r, m), (l, m))


def EE(g, W, H):
    l, r, b, t = box(g, W, H); K = cut(H, W)
    g.line((0, t), (r - K, t), (r, t - K), (r, b), (0, b))
    g.line((0.3 * W, H / 2), (r, H / 2))


def YU(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.7 * cut(H, W); xo = 0.34 * W
    g.line((l, 0), (l, H))
    g.line((l, H / 2), (xo, H / 2))
    g.loop((xo, t - k), (xo + k, t), (r, t), (r, b + k), (r - k, b), (xo, b))


def YA(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.7 * cut(H, W); p = 0.44 * H
    g.line((r, 0), (r, H))
    g.line((r, t), (l + k, t), (l, t - k), (l, p + 0.6 * k), (l + 0.6 * k, p), (r, p))
    g.line((0.48 * W, p), (l, 0.24 * H), (l, 0))


def be_small(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W); top = ASC - g.S / 2
    g.line((W, top), (l + k, top), (l, top - k), (l, b), (r - k, b), (r, b + k), (r, t))
    g.line((r, t), (l, t))


def ef_small(g, W, H):
    l, r, b, t = box(g, W, H); k = cut(H, W)
    g.line((W / 2, ASC), (W / 2, DESC))
    g.loop((l, t - k), (l + k, t), (r, t), (r, b + k), (r - k, b), (l, b))


# diacritics
def with_dots(fn):
    def draw(g, W, H):
        fn(g, W, H)
        y = H + 80
        g.rect(0.3 * W - g.S / 2, y, 0.3 * W + g.S / 2, y + g.S)
        g.rect(0.7 * W - g.S / 2, y, 0.7 * W + g.S / 2, y + g.S)
    return draw


def with_breve(fn):
    def draw(g, W, H):
        fn(g, W, H)
        y = H + 60 + g.S / 2
        g.line((0.28 * W, y + 80), (0.28 * W, y), (0.72 * W, y), (0.72 * W, y + 80))
    return draw


# ── signs ───────────────────────────────────────────────────────────────
MID = 330   # centre line of mathematical signs


def period(g, W, H): g.rect(0, 0, g.S, g.S)


def comma(g, W, H):
    g.rect(0, 0, g.S, g.S)
    g.poly((0, 0), (g.S, 0), (g.S * 0.45, -150), (0, -150))


def colon(g, W, H):
    g.rect(0, 0, g.S, g.S); g.rect(0, XH - g.S, g.S, XH)


def semicolon(g, W, H):
    comma(g, W, H); g.rect(0, XH - g.S, g.S, XH)


def exclam(g, W, H):
    g.rect(0, 0.26 * CAP, g.S, CAP); g.rect(0, 0, g.S, g.S)


def question(g, W, H):
    l, r, b, t = box(g, W, H); k = 0.7 * cut(H, W); xc = W / 2
    g.line((0, t), (r - k, t), (r, t - k), (r, 0.52 * H), (xc, 0.52 * H), (xc, 0.27 * H))
    g.rect(xc - g.S / 2, 0, xc + g.S / 2, g.S)


def bar_at(y, width):
    def draw(g, W, H):
        g.rect(0, y - g.S / 2, width + (W - width), y + g.S / 2)
    return draw


def underscore(g, W, H): g.rect(0, -130, W, -130 + g.S)


def slash(g, W, H):
    s = g.S * 1.15
    g.poly((0, -100), (s, -100), (W, CAP + 40), (W - s, CAP + 40))


def backslash(g, W, H):
    s = g.S * 1.15
    g.poly((W - s, -100), (W, -100), (s, CAP + 40), (0, CAP + 40))


def vbar(g, W, H): g.rect(0, -180, g.S, CAP + 80)


def paren_l(g, W, H):
    h = g.S / 2; k = 90; top = CAP + 30 - h; bot = -150 + h
    g.line((W, top), (h + k, top), (h, top - k), (h, bot + k), (h + k, bot), (W, bot))


def paren_r(g, W, H):
    h = g.S / 2; k = 90; top = CAP + 30 - h; bot = -150 + h; r = W - h
    g.line((0, top), (r - k, top), (r, top - k), (r, bot + k), (r - k, bot), (0, bot))


def bracket_l(g, W, H):
    h = g.S / 2; top = CAP + 30 - h; bot = -150 + h
    g.line((W, top), (h, top), (h, bot), (W, bot))


def bracket_r(g, W, H):
    h = g.S / 2; top = CAP + 30 - h; bot = -150 + h
    g.line((0, top), (W - h, top), (W - h, bot), (0, bot))


def plus(g, W, H):
    h = g.S / 2
    g.rect(0, MID - h, W, MID + h); g.rect(W / 2 - h, MID - W / 2, W / 2 + h, MID + W / 2)


def equal(g, W, H):
    h = g.S / 2
    g.rect(0, MID + 100 - h, W, MID + 100 + h); g.rect(0, MID - 100 - h, W, MID - 100 + h)


def times(g, W, H):
    s = W / 2 - g.S * 0.4
    g.line((W / 2 - s, MID - s), (W / 2 + s, MID + s)); g.line((W / 2 - s, MID + s), (W / 2 + s, MID - s))


def divide(g, W, H):
    h = g.S / 2
    g.rect(0, MID - h, W, MID + h)
    g.rect(W / 2 - h, MID + 120, W / 2 + h, MID + 120 + g.S); g.rect(W / 2 - h, MID - 120 - g.S, W / 2 + h, MID - 120)


def plusminus(g, W, H):
    h = g.S / 2; c = MID + 70
    g.rect(0, c - h, W, c + h); g.rect(W / 2 - h, c - 200, W / 2 + h, c + 200); g.rect(0, 0, W, g.S)


def hash_(g, W, H):
    h = g.S / 2
    g.rect(0.3 * W - h, 0, 0.3 * W + h, CAP); g.rect(0.7 * W - h, 0, 0.7 * W + h, CAP)
    g.rect(0, 0.33 * CAP - h, W, 0.33 * CAP + h); g.rect(0, 0.67 * CAP - h, W, 0.67 * CAP + h)


def percent(g, W, H):
    h = g.S / 2; s = 230 + g.S
    g.loop((h, CAP - h), (s - h, CAP - h), (s - h, CAP - s + h), (h, CAP - s + h))
    g.loop((W - s + h, s - h), (W - h, s - h), (W - h, h), (W - s + h, h))
    q = g.S * 1.1
    g.poly((0.18 * W, 0), (0.18 * W + q, 0), (0.82 * W, CAP), (0.82 * W - q, CAP))


def quote(g, W, H): g.rect(0, 0.66 * CAP, g.S, CAP)


def dquote(g, W, H): g.rect(0, 0.66 * CAP, g.S, CAP); g.rect(W - g.S, 0.66 * CAP, W, CAP)


def middot(g, W, H): g.rect(0, MID - g.S / 2, g.S, MID + g.S / 2)


def bullet(g, W, H): g.rect(0, MID - W / 2, W, MID + W / 2)


def sharp(g, W, H):
    h = g.S / 2; x1, x2 = 0.32 * W, 0.68 * W
    g.rect(x1 - h, -40, x1 + h, CAP - 60); g.rect(x2 - h, 40, x2 + h, CAP + 20)
    for y in (150, 410):
        g.poly((0, y), (W, y + 120), (W, y + 120 + g.S), (0, y + g.S))


def flat(g, W, H):
    # raised above the line and narrower than b, the top steeply cut — reads as an accidental
    l, r, b, t = box(g, W, CAP); k = 80; y0 = 130 + g.S / 2; yb = 430
    g.line((l, CAP + 60), (l, y0 - g.S / 2))
    g.line((l, yb), (r - k, yb + 90), (r, yb + 90 - k), (r, y0 + 0.6 * k), (r - 0.6 * k, y0), (l, y0))


def arrow(g, W, H):
    h = g.S / 2
    g.rect(0, MID - h, W - 40, MID + h)
    g.line((W - 190, MID + 170), (W - h * 1.4, MID), (W - 190, MID - 170))


def tri_up(g, W, H): g.poly((0, 0), (W / 2, 0.86 * W), (W, 0))


def tri_down(g, W, H): g.poly((0, 0.86 * W), (W, 0.86 * W), (W / 2, 0))


def chevrons(left):
    def draw(g, W, H):
        for x0 in (0, 0.48 * W):
            a, z = (x0 + 0.5 * W, x0) if left else (x0, x0 + 0.5 * W)
            g.line((a, MID + 150), (z, MID), (a, MID - 150))
    return draw


def angle(left):
    def draw(g, W, H):
        a, z = (W, g.S / 2) if left else (0, W - g.S / 2)
        g.line((a, MID + 230), (z, MID), (a, MID - 230))
    return draw


def degree(g, W, H):
    h = g.S / 2
    g.loop((h, CAP - h), (W - h, CAP - h), (W - h, CAP - W + h), (h, CAP - W + h))


def asterisk(g, W, H):
    c = 0.74 * CAP; s = W / 2
    g.line((W / 2, c - s), (W / 2, c + s))
    g.line((W / 2 - s * 0.87, c - s / 2), (W / 2 + s * 0.87, c + s / 2))
    g.line((W / 2 - s * 0.87, c + s / 2), (W / 2 + s * 0.87, c - s / 2))


def ellipsis(g, W, H):
    for x in (0, W / 2 - g.S / 2, W - g.S):
        g.rect(x, 0, x + g.S, g.S)


def numero(g, W, H):
    N(g, 560, CAP)
    h = g.S / 2; x0 = 620; s = 200
    g.loop((x0 + h, CAP - h), (x0 + s - h, CAP - h), (x0 + s - h, CAP - s + h), (x0 + h, CAP - s + h))
    g.rect(x0, CAP - s - 80 - g.S, x0 + s, CAP - s - 80)


# ── table: (codes, body width, height, drawing); width None — a single stroke ──
TABLE = [
    ([0x41, 0x410], 560, CAP, A), ([0x42, 0x412], 560, CAP, B), ([0x43, 0x421], 540, CAP, C),
    ([0x44], 570, CAP, D), ([0x45, 0x415], 500, CAP, E), ([0x46], 490, CAP, F), ([0x47], 570, CAP, G_),
    ([0x48, 0x41D], 570, CAP, H_), ([0x49, 0x406], None, CAP, I), ([0x4A], 470, CAP, J),
    ([0x4B, 0x41A], 560, CAP, K_), ([0x4C], 470, CAP, L), ([0x4D, 0x41C], 680, CAP, M), ([0x4E], 590, CAP, N),
    ([0x4F, 0x41E], 580, CAP, O), ([0x50, 0x420], 540, CAP, P), ([0x51], 580, CAP, Q), ([0x52], 560, CAP, R),
    ([0x53], 540, CAP, S_), ([0x54, 0x422], 540, CAP, T), ([0x55], 570, CAP, U), ([0x56], 580, CAP, V),
    ([0x57], 760, CAP, W_), ([0x58, 0x425], 580, CAP, X), ([0x59], 580, CAP, Y), ([0x5A], 520, CAP, Z),

    ([0x61, 0x430], 470, XH, a), ([0x62], 480, XH, b_), ([0x63, 0x441], 450, XH, c), ([0x64], 480, XH, d),
    ([0x65, 0x435], 470, XH, e), ([0x66], 330, XH, f), ([0x67], 480, XH, g_), ([0x68], 470, XH, h_),
    ([0x69, 0x456], None, XH, i_), ([0x6A], 220, XH, j_), ([0x6B], 450, XH, k_), ([0x6C], None, XH, l_),
    ([0x6D], 700, XH, m_), ([0x6E], 470, XH, n_), ([0x6F, 0x43E], 480, XH, O), ([0x70, 0x440], 480, XH, p_),
    ([0x71], 480, XH, q_), ([0x72], 330, XH, r_), ([0x73], 450, XH, S_), ([0x74], 340, XH, t_),
    ([0x75], 470, XH, u_), ([0x76], 480, XH, V), ([0x77], 640, XH, W_), ([0x78, 0x445], 470, XH, X),
    ([0x79, 0x443], 470, XH, y_), ([0x7A], 430, XH, Z),

    ([0x30], 500, CAP, d0), ([0x31], 500, CAP, d1), ([0x32], 500, CAP, d2), ([0x33], 500, CAP, d3),
    ([0x34], 500, CAP, d4), ([0x35], 500, CAP, d5), ([0x36], 500, CAP, d6), ([0x37], 500, CAP, d7),
    ([0x38], 500, CAP, d8), ([0x39], 500, CAP, d9),

    ([0x411], 540, CAP, BE), ([0x413], 470, CAP, GE), ([0x414], 620, CAP, DE), ([0x401], 500, CAP, with_dots(E)),
    ([0x416], 760, CAP, ZHE), ([0x417], 520, CAP, d3), ([0x418], 590, CAP, I_), ([0x419], 590, CAP, with_breve(I_)),
    ([0x41B], 580, CAP, EL), ([0x41F], 570, CAP, PE), ([0x423], 560, CAP, U_cyr), ([0x424], 640, CAP, EF),
    ([0x426], 600, CAP, TSE), ([0x427], 540, CAP, CHE), ([0x428], 720, CAP, SHA), ([0x429], 770, CAP, SHCHA),
    ([0x42A], 600, CAP, HARD), ([0x42B], 720, CAP, YERU), ([0x42C], 520, CAP, SOFT), ([0x42D], 540, CAP, EE),
    ([0x42E], 760, CAP, YU), ([0x42F], 560, CAP, YA),

    ([0x431], 480, XH, be_small), ([0x432], 480, XH, B), ([0x433], 400, XH, GE), ([0x434], 530, XH, DE),
    ([0x451], 470, XH, with_dots(e)), ([0x436], 650, XH, ZHE), ([0x437], 440, XH, d3), ([0x438], 510, XH, I_),
    ([0x439], 510, XH, with_breve(I_)), ([0x43A], 470, XH, K_), ([0x43B], 500, XH, EL), ([0x43C], 590, XH, M),
    ([0x43D], 490, XH, H_), ([0x43F], 490, XH, PE), ([0x442], 460, XH, T), ([0x444], 600, XH, ef_small),
    ([0x446], 520, XH, TSE), ([0x447], 460, XH, CHE), ([0x448], 620, XH, SHA), ([0x449], 660, XH, SHCHA),
    ([0x44A], 520, XH, HARD), ([0x44B], 620, XH, YERU), ([0x44C], 450, XH, SOFT), ([0x44D], 460, XH, EE),
    ([0x44E], 650, XH, YU), ([0x44F], 480, XH, YA),

    ([0x2E], None, CAP, period), ([0x2C], None, CAP, comma), ([0x3A], None, CAP, colon), ([0x3B], None, CAP, semicolon),
    ([0x21], None, CAP, exclam), ([0x3F], 420, CAP, question), ([0x2D], 300, CAP, bar_at(290, 300)),
    ([0x2013], 500, CAP, bar_at(290, 500)), ([0x2014], 900, CAP, bar_at(290, 900)), ([0x2212], 500, CAP, bar_at(MID, 500)),
    ([0x5F], 500, CAP, underscore), ([0x2F], 380, CAP, slash), ([0x5C], 380, CAP, backslash), ([0x7C], None, CAP, vbar),
    ([0x28], 260, CAP, paren_l), ([0x29], 260, CAP, paren_r), ([0x5B], 240, CAP, bracket_l), ([0x5D], 240, CAP, bracket_r),
    ([0x2B], 500, CAP, plus), ([0x3D], 500, CAP, equal), ([0xD7], 460, CAP, times), ([0xF7], 500, CAP, divide),
    ([0xB1], 500, CAP, plusminus), ([0x23], 580, CAP, hash_), ([0x25], 720, CAP, percent),
    ([0x27, 0x2019, 0x2018], None, CAP, quote), ([0x22, 0x201C, 0x201D], 230, CAP, dquote),
    ([0xB7], None, CAP, middot), ([0x2022], 170, CAP, bullet), ([0x266F], 460, CAP, sharp), ([0x266D], 320, CAP, flat),
    ([0x2192], 620, CAP, arrow), ([0x25B2], 480, CAP, tri_up), ([0x25BC], 480, CAP, tri_down),
    ([0xAB], 460, CAP, chevrons(True)), ([0xBB], 460, CAP, chevrons(False)),
    ([0x3C], 400, CAP, angle(True)), ([0x3E], 400, CAP, angle(False)),
    ([0xB0], 240, CAP, degree), ([0x2A], 420, CAP, asterisk), ([0x2026], 560, CAP, ellipsis), ([0x2116], 820, CAP, numero),
]

SPACE = 260


def build(stroke, extra):
    """[(codes, body width, outlines)] for stroke width `stroke`; extra — added to the body width."""
    out = []
    for codes, width, height, fn in TABLE:
        W = stroke if width is None else width + extra
        g = Glyph(stroke)
        fn(g, W, height)
        out.append((codes, W, g.contours))
    return out
