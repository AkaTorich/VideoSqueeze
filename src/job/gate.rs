// Frame-rate limiting. Variable mode keeps the source timing and drops frames that come faster than the
// target rate; constant mode (AVI) puts exactly one frame in every slot, repeating the last one over gaps.
use crate::codec::Frame;

pub struct Gate {
    fps: f64,
    cfr: bool,
    t0: Option<f64>,
    last_slot: i64,
    last: Option<Frame>,
}

impl Gate {
    pub fn new(fps: f64, cfr: bool) -> Self {
        Gate { fps: fps.max(0.1), cfr, t0: None, last_slot: -1, last: None }
    }

    pub fn push(&mut self, f: Frame) -> Vec<Frame> {
        let t0 = *self.t0.get_or_insert(f.pts);
        let slot = ((f.pts - t0) * self.fps + 0.5).floor() as i64;
        let mut out = Vec::new();
        if slot <= self.last_slot {
            if self.cfr {
                self.last = Some(f);
            }
            return out;
        }
        if self.cfr {
            if let Some(prev) = self.last.take() {
                for s in self.last_slot + 1..slot {
                    let mut d = prev.clone();
                    d.pts = t0 + s as f64 / self.fps;
                    out.push(d);
                }
            }
            let mut f = f;
            f.pts = t0 + slot as f64 / self.fps;
            self.last = Some(f.clone());
            out.push(f);
        } else {
            out.push(f);
        }
        self.last_slot = slot;
        out
    }
}
