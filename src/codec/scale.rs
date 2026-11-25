// Picture resizing: separable triangle filter whose support widens with the reduction factor, so
// downscaling averages every source pixel (no aliasing) and upscaling is plain bilinear.
use rayon::prelude::*;

use super::Frame;

struct Taps {
    start: usize,
    weights: Vec<f32>,
}

fn taps(src: usize, dst: usize) -> Vec<Taps> {
    let ratio = src as f32 / dst as f32;
    let support = ratio.max(1.0);
    (0..dst)
        .map(|i| {
            let center = (i as f32 + 0.5) * ratio - 0.5;
            let lo = (center - support).floor().max(0.0) as usize;
            let hi = ((center + support).ceil() as usize).min(src - 1);
            let mut weights: Vec<f32> = (lo..=hi).map(|s| (1.0 - (s as f32 - center).abs() / support).max(0.0)).collect();
            let sum: f32 = weights.iter().sum();
            if sum > 0.0 {
                weights.iter_mut().for_each(|w| *w /= sum);
            } else {
                weights = vec![0.0; hi - lo + 1];
                weights[(center.round() as usize).clamp(lo, hi) - lo] = 1.0;
            }
            Taps { start: lo, weights }
        })
        .collect()
}

pub fn resize_plane(src: &[u8], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<u8> {
    if sw == dw && sh == dh {
        return src.to_vec();
    }
    let hx = taps(sw, dw);
    let vy = taps(sh, dh);
    // horizontal pass
    let mut mid = vec![0f32; dw * sh];
    mid.par_chunks_mut(dw).enumerate().for_each(|(row, out)| {
        let line = &src[row * sw..row * sw + sw];
        for (x, t) in hx.iter().enumerate() {
            out[x] = t.weights.iter().enumerate().map(|(k, w)| line[t.start + k] as f32 * w).sum();
        }
    });
    // vertical pass
    let mut out = vec![0u8; dw * dh];
    out.par_chunks_mut(dw).enumerate().for_each(|(row, line)| {
        let t = &vy[row];
        for (x, px) in line.iter_mut().enumerate() {
            let v: f32 = t.weights.iter().enumerate().map(|(k, w)| mid[(t.start + k) * dw + x] * w).sum();
            *px = v.round().clamp(0.0, 255.0) as u8;
        }
    });
    out
}

/// Resize a frame to `dw × dh` (both even).
pub fn scale_frame(f: &Frame, dw: usize, dh: usize) -> Frame {
    if f.w == dw && f.h == dh {
        return f.clone();
    }
    let (cw, ch) = (dw.div_ceil(2), dh.div_ceil(2));
    Frame {
        w: dw,
        h: dh,
        y: resize_plane(&f.y, f.w, f.h, dw, dh),
        u: resize_plane(&f.u, f.cw(), f.ch(), cw, ch),
        v: resize_plane(&f.v, f.cw(), f.ch(), cw, ch),
        pts: f.pts,
    }
}

/// Turn a frame clockwise by 90, 180 or 270 degrees.
pub fn rotate_frame(f: &Frame, degrees: u32) -> Frame {
    let turn = |src: &[u8], w: usize, h: usize| -> (Vec<u8>, usize, usize) {
        let mut out = vec![0u8; w * h];
        match degrees {
            90 => {
                for y in 0..h {
                    for x in 0..w {
                        out[x * h + (h - 1 - y)] = src[y * w + x];
                    }
                }
                (out, h, w)
            }
            270 => {
                for y in 0..h {
                    for x in 0..w {
                        out[(w - 1 - x) * h + y] = src[y * w + x];
                    }
                }
                (out, h, w)
            }
            180 => {
                for (i, px) in src.iter().enumerate() {
                    out[w * h - 1 - i] = *px;
                }
                (out, w, h)
            }
            _ => (src.to_vec(), w, h),
        }
    };
    let (y, w, h) = turn(&f.y, f.w, f.h);
    let (u, _, _) = turn(&f.u, f.cw(), f.ch());
    let (v, _, _) = turn(&f.v, f.cw(), f.ch());
    Frame { w, h, y, u, v, pts: f.pts }
}

/// Rotation of an MP4 display matrix, degrees clockwise (0 when none).
pub fn matrix_rotation(m: Option<[i32; 9]>) -> u32 {
    let Some(m) = m else { return 0 };
    match (m[0].signum(), m[1].signum(), m[3].signum(), m[4].signum()) {
        (0, 1, -1, 0) => 90,
        (-1, 0, 0, -1) => 180,
        (0, -1, 1, 0) => 270,
        _ => 0,
    }
}
