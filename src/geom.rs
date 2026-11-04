//! 平面几何：仿射变换、包围盒、圆弧展开

use crate::jww::P;

/// x' = a x + c y + e,  y' = b x + d y + f
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const ID: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(x: f64, y: f64) -> Affine {
        Affine { e: x, f: y, ..Affine::ID }
    }
    pub fn rotate(rad: f64) -> Affine {
        let (s, c) = rad.sin_cos();
        Affine { a: c, b: s, c: -s, d: c, e: 0.0, f: 0.0 }
    }
    pub fn scale(sx: f64, sy: f64) -> Affine {
        Affine { a: sx, d: sy, ..Affine::ID }
    }
    pub fn shear_x(k: f64) -> Affine {
        Affine { c: k, ..Affine::ID }
    }

    /// self ∘ o（先 o 后 self）
    pub fn then(&self, o: &Affine) -> Affine {
        Affine {
            a: self.a * o.a + self.c * o.b,
            b: self.b * o.a + self.d * o.b,
            c: self.a * o.c + self.c * o.d,
            d: self.b * o.c + self.d * o.d,
            e: self.a * o.e + self.c * o.f + self.e,
            f: self.b * o.e + self.d * o.f + self.f,
        }
    }

    pub fn apply(&self, p: P) -> P {
        P { x: self.a * p.x + self.c * p.y + self.e, y: self.b * p.x + self.d * p.y + self.f }
    }

    /// 最大伸缩率（决定曲线展开要多细）
    pub fn max_scale(&self) -> f64 {
        (self.a.hypot(self.b)).max(self.c.hypot(self.d))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct BBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl BBox {
    pub const EMPTY: BBox = BBox { x0: f64::MAX, y0: f64::MAX, x1: f64::MIN, y1: f64::MIN };

    pub fn add(&mut self, p: P) {
        self.x0 = self.x0.min(p.x);
        self.y0 = self.y0.min(p.y);
        self.x1 = self.x1.max(p.x);
        self.y1 = self.y1.max(p.y);
    }
    pub fn union(&mut self, o: &BBox) {
        if !o.is_empty() {
            self.add(P { x: o.x0, y: o.y0 });
            self.add(P { x: o.x1, y: o.y1 });
        }
    }
    pub fn is_empty(&self) -> bool {
        self.x0 > self.x1
    }
    pub fn hits(&self, o: &BBox) -> bool {
        !(self.x1 < o.x0 || o.x1 < self.x0 || self.y1 < o.y0 || o.y1 < self.y0)
    }
}

/// 椭圆弧上参数 t 的点：中心 c，半径 r，扁平率 flat（纵/横），倾斜 tilt
pub fn ellipse_pt(c: P, r: f64, flat: f64, tilt: f64, t: f64) -> P {
    let (s, co) = t.sin_cos();
    let (x, y) = (r * co, r * flat * s);
    let (ts, tc) = tilt.sin_cos();
    P { x: c.x + x * tc - y * ts, y: c.y + x * ts + y * tc }
}

/// 把椭圆弧展开成折线。tol 是弦高容差（与 r 同单位）
pub fn flatten_arc(c: P, r: f64, flat: f64, tilt: f64, start: f64, sweep: f64, tol: f64, out: &mut Vec<P>) {
    let rmax = r.abs() * flat.abs().max(1.0);
    let step = if rmax <= tol { std::f64::consts::PI / 2.0 } else { 2.0 * (1.0 - tol / rmax).acos() };
    let n = ((sweep.abs() / step.max(1e-4)).ceil() as usize).clamp(2, 2048);
    for i in 0..=n {
        out.push(ellipse_pt(c, r, flat, tilt, start + sweep * i as f64 / n as f64));
    }
}
