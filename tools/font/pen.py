# Font pen: a letter's centre line → outline of a stroke of constant width.
# Joins are sharp (where the edges cross); on a steep turn the outer edge is cut — hence the bevelled
# corners of M, N, V, W. Stroke ends are square, across the line.
# Fills run clockwise, counters (inside closed lines) anticlockwise: TrueType's non-zero winding rule
# merges overlapping strokes into one shape.
import math

# the outer edge of a steep turn is cut if the spike would be longer than 1.6 stroke widths
BEVEL_DOT = -0.22


def _unit(ax, ay, bx, by):
    dx, dy = bx - ax, by - ay
    n = math.hypot(dx, dy)
    return dx / n, dy / n


def _clean(points):
    out = []
    for p in points:
        if not out or abs(p[0] - out[-1][0]) > 1e-6 or abs(p[1] - out[-1][1]) > 1e-6:
            out.append((float(p[0]), float(p[1])))
    return out


def _joint(v, d1, d2, off):
    """Edge points on side off (+ left, − right) at vertex v between directions d1 and d2."""
    n1 = (-d1[1], d1[0])
    n2 = (-d2[1], d2[0])
    dot = n1[0] * n2[0] + n1[1] * n2[1]
    cross = d1[0] * d2[1] - d1[1] * d2[0]
    inner = (cross > 0 and off > 0) or (cross < 0 and off < 0)
    if abs(cross) < 1e-9 and dot > 0:
        return [(v[0] + n1[0] * off, v[1] + n1[1] * off)]
    if inner or dot >= BEVEL_DOT:
        k = off / (1.0 + dot)
        return [(v[0] + (n1[0] + n2[0]) * k, v[1] + (n1[1] + n2[1]) * k)]
    return [(v[0] + n1[0] * off, v[1] + n1[1] * off), (v[0] + n2[0] * off, v[1] + n2[1] * off)]


def _side_open(pts, off):
    dirs = [_unit(*pts[i], *pts[i + 1]) for i in range(len(pts) - 1)]
    d0 = dirs[0]
    side = [(pts[0][0] - d0[1] * off, pts[0][1] + d0[0] * off)]
    for i in range(1, len(pts) - 1):
        side += _joint(pts[i], dirs[i - 1], dirs[i], off)
    dl = dirs[-1]
    side.append((pts[-1][0] - dl[1] * off, pts[-1][1] + dl[0] * off))
    return side


def _side_loop(pts, off):
    n = len(pts)
    dirs = [_unit(*pts[i], *pts[(i + 1) % n]) for i in range(n)]
    side = []
    for i in range(n):
        side += _joint(pts[i], dirs[i - 1], dirs[i], off)
    return side


def area(poly):
    s = 0.0
    for i in range(len(poly)):
        x0, y0 = poly[i]
        x1, y1 = poly[(i + 1) % len(poly)]
        s += x0 * y1 - x1 * y0
    return s / 2.0


def clockwise(poly):
    return poly if area(poly) < 0 else poly[::-1]


def counter(poly):
    return poly if area(poly) > 0 else poly[::-1]


def line(points, width):
    """Open stroke: one clockwise outline."""
    pts = _clean(points)
    h = width / 2.0
    left = _side_open(pts, h)
    right = _side_open(pts, -h)
    return [clockwise(left + right[::-1])]


def loop(points, width):
    """Closed stroke: outer outline clockwise, the counter anticlockwise."""
    pts = _clean(points)
    h = width / 2.0
    a = _side_loop(pts, h)
    b = _side_loop(pts, -h)
    outer, inner = (a, b) if abs(area(a)) > abs(area(b)) else (b, a)
    return [clockwise(outer), counter(inner)]
