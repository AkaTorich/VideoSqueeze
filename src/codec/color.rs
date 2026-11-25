// Colour conversion between RGB and limited-range YUV 4:2:0 (BT.601 below 720 lines, BT.709 from 720 up —
// the convention players assume for video without colour tags).
use super::Frame;

fn coeffs(h: usize) -> (f32, f32) {
    // (Kr, Kb)
    if h >= 720 { (0.2126, 0.0722) } else { (0.299, 0.114) }
}

/// RGB (3 bytes per pixel) or grey (1 byte) → limited-range YUV 4:2:0.
pub fn rgb_to_frame(px: &[u8], w: usize, h: usize, bpp: usize, full_range_yuv_jpeg: bool) -> Frame {
    let (kr, kb) = if full_range_yuv_jpeg { (0.299, 0.114) } else { coeffs(h) };
    let kg = 1.0 - kr - kb;
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut y = vec![0u8; w * h];
    let mut u = vec![128u8; cw * ch];
    let mut v = vec![128u8; cw * ch];
    let mut cb_acc = vec![0f32; cw * ch];
    let mut cr_acc = vec![0f32; cw * ch];
    let mut cnt = vec![0f32; cw * ch];
    for j in 0..h {
        for i in 0..w {
            let p = (j * w + i) * bpp;
            let (r, g, b) = if bpp >= 3 { (px[p] as f32, px[p + 1] as f32, px[p + 2] as f32) } else { (px[p] as f32, px[p] as f32, px[p] as f32) };
            let yy = kr * r + kg * g + kb * b;
            y[j * w + i] = (16.0 + yy * 219.0 / 255.0).round().clamp(0.0, 255.0) as u8;
            let c = (j / 2) * cw + i / 2;
            cb_acc[c] += (b - yy) / (2.0 * (1.0 - kb));
            cr_acc[c] += (r - yy) / (2.0 * (1.0 - kr));
            cnt[c] += 1.0;
        }
    }
    for c in 0..cw * ch {
        if cnt[c] > 0.0 {
            u[c] = (128.0 + cb_acc[c] / cnt[c] * 224.0 / 255.0).round().clamp(0.0, 255.0) as u8;
            v[c] = (128.0 + cr_acc[c] / cnt[c] * 224.0 / 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    Frame { w, h, y, u, v, pts: 0.0 }
}

/// Limited-range YUV 4:2:0 → RGBA.
pub fn frame_to_rgba(f: &Frame) -> Vec<u8> {
    let (kr, kb) = coeffs(f.h);
    let kg = 1.0 - kr - kb;
    let cw = f.cw();
    let mut out = vec![255u8; f.w * f.h * 4];
    for j in 0..f.h {
        for i in 0..f.w {
            let yy = (f.y[j * f.w + i] as f32 - 16.0) * 255.0 / 219.0;
            let c = (j / 2) * cw + i / 2;
            let cb = (f.u[c] as f32 - 128.0) * 255.0 / 224.0;
            let cr = (f.v[c] as f32 - 128.0) * 255.0 / 224.0;
            let r = yy + 2.0 * (1.0 - kr) * cr;
            let b = yy + 2.0 * (1.0 - kb) * cb;
            let g = (yy - kr * r - kb * b) / kg;
            let o = (j * f.w + i) * 4;
            out[o] = r.round().clamp(0.0, 255.0) as u8;
            out[o + 1] = g.round().clamp(0.0, 255.0) as u8;
            out[o + 2] = b.round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}
