//! 作图用的平面几何：向量运算、线和圆的交点、投影、距离。坐标都是图寸（mm）。

use crate::jww::{Arc, Ent, Shape, P};
use std::f64::consts::{PI, TAU};

pub const EPS: f64 = 1e-9;

pub fn p(x: f64, y: f64) -> P {
    P { x, y }
}
pub fn add(a: P, b: P) -> P {
    p(a.x + b.x, a.y + b.y)
}
pub fn sub(a: P, b: P) -> P {
    p(a.x - b.x, a.y - b.y)
}
pub fn mul(a: P, k: f64) -> P {
    p(a.x * k, a.y * k)
}
pub fn dot(a: P, b: P) -> f64 {
    a.x * b.x + a.y * b.y
}
pub fn cross(a: P, b: P) -> f64 {
    a.x * b.y - a.y * b.x
}
pub fn len(a: P) -> f64 {
    a.x.hypot(a.y)
}
pub fn dist(a: P, b: P) -> f64 {
    len(sub(a, b))
}
pub fn norm(a: P) -> P {
    let l = len(a);
    if l < EPS {
        p(1.0, 0.0)
    } else {
        mul(a, 1.0 / l)
    }
}
/// 左转 90°
pub fn perp(a: P) -> P {
    p(-a.y, a.x)
}
pub fn mid(a: P, b: P) -> P {
    p((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}
pub fn lerp(a: P, b: P, t: f64) -> P {
    p(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}
pub fn polar(c: P, r: f64, ang: f64) -> P {
    p(c.x + r * ang.cos(), c.y + r * ang.sin())
}
pub fn angle_of(v: P) -> f64 {
    v.y.atan2(v.x)
}
/// 角度规范到 [0, 2π)
pub fn norm_ang(a: f64) -> f64 {
    let r = a.rem_euclid(TAU);
    if r >= TAU - 1e-12 {
        0.0
    } else {
        r
    }
}
pub fn rot(v: P, ang: f64) -> P {
    let (s, c) = ang.sin_cos();
    p(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// 点 q 在直线 a→b 上的参数（a = 0, b = 1）
pub fn param(a: P, b: P, q: P) -> f64 {
    let d = sub(b, a);
    let l2 = dot(d, d);
    if l2 < EPS {
        0.0
    } else {
        dot(sub(q, a), d) / l2
    }
}
/// 点在（无限）直线上的垂足
pub fn project(a: P, b: P, q: P) -> P {
    lerp(a, b, param(a, b, q))
}
/// 点到线段的距离
pub fn dist_seg(a: P, b: P, q: P) -> f64 {
    dist(q, lerp(a, b, param(a, b, q).clamp(0.0, 1.0)))
}

/// 两条无限直线的交点
pub fn line_line(a1: P, b1: P, a2: P, b2: P) -> Option<P> {
    let d1 = sub(b1, a1);
    let d2 = sub(b2, a2);
    let den = cross(d1, d2);
    if den.abs() < EPS * len(d1).max(1.0) * len(d2).max(1.0) {
        return None;
    }
    let t = cross(sub(a2, a1), d2) / den;
    Some(lerp(a1, b1, t))
}

/// 无限直线和圆的交点
pub fn line_circle(a: P, b: P, c: P, r: f64) -> Vec<P> {
    let f = project(a, b, c);
    let h = dist(f, c);
    if h > r + 1e-9 {
        return vec![];
    }
    let d = norm(sub(b, a));
    let k = (r * r - h * h).max(0.0).sqrt();
    if k < 1e-9 {
        vec![f]
    } else {
        vec![sub(f, mul(d, k)), add(f, mul(d, k))]
    }
}

pub fn circle_circle(c1: P, r1: f64, c2: P, r2: f64) -> Vec<P> {
    let d = dist(c1, c2);
    if d < EPS || d > r1 + r2 + 1e-9 || d < (r1 - r2).abs() - 1e-9 {
        return vec![];
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let u = norm(sub(c2, c1));
    let m = add(c1, mul(u, a));
    if h < 1e-9 {
        vec![m]
    } else {
        vec![add(m, mul(perp(u), h)), sub(m, mul(perp(u), h))]
    }
}

/// 三点通过的圆（中心、半径）
pub fn circle3(a: P, b: P, c: P) -> Option<(P, f64)> {
    let m1 = mid(a, b);
    let m2 = mid(b, c);
    let o = line_line(m1, add(m1, perp(sub(b, a))), m2, add(m2, perp(sub(c, b))))?;
    Some((o, dist(o, a)))
}

/// 作图用的曲线：线段，或圆・圆弧（扁平率 1 的）。角度是绝对角（已含倾斜角）
#[derive(Clone, Copy, Debug)]
pub enum Curve {
    Seg(P, P),
    Circ { c: P, r: f64, a0: f64, sweep: f64, full: bool },
}

impl Curve {
    pub fn of(e: &Ent) -> Option<Curve> {
        match &e.shape {
            Shape::Line { a, b } => Some(Curve::Seg(*a, *b)),
            Shape::Arc(a) if (a.flat - 1.0).abs() < 1e-9 => Some(Curve::Circ {
                c: a.c,
                r: a.r,
                a0: norm_ang(a.start + a.tilt),
                sweep: if a.full { TAU } else { a.sweep },
                full: a.full,
            }),
            _ => None,
        }
    }

    /// 点是不是在圆弧的角度范围里
    pub fn on_arc(&self, q: P) -> bool {
        match *self {
            Curve::Seg(a, b) => {
                let t = param(a, b, q);
                (-1e-9..=1.0 + 1e-9).contains(&t)
            }
            Curve::Circ { c, a0, sweep, full, .. } => full || norm_ang(angle_of(sub(q, c)) - a0) <= sweep + 1e-9,
        }
    }

    /// 两条曲线的交点。`ext` = 线当作无限长、圆弧当作整圆
    pub fn meet(&self, o: &Curve, ext: bool) -> Vec<P> {
        let v = match (*self, *o) {
            (Curve::Seg(a, b), Curve::Seg(c, d)) => line_line(a, b, c, d).into_iter().collect(),
            (Curve::Seg(a, b), Curve::Circ { c, r, .. }) | (Curve::Circ { c, r, .. }, Curve::Seg(a, b)) => {
                line_circle(a, b, c, r)
            }
            (Curve::Circ { c: c1, r: r1, .. }, Curve::Circ { c: c2, r: r2, .. }) => circle_circle(c1, r1, c2, r2),
        };
        if ext {
            v
        } else {
            v.into_iter().filter(|q| self.on_arc(*q) && o.on_arc(*q)).collect()
        }
    }

    pub fn dist(&self, q: P) -> f64 {
        match *self {
            Curve::Seg(a, b) => dist_seg(a, b, q),
            Curve::Circ { c, r, a0, sweep, full } => {
                if full || self.on_arc(q) {
                    (dist(q, c) - r).abs()
                } else {
                    dist(q, polar(c, r, a0)).min(dist(q, polar(c, r, a0 + sweep)))
                }
            }
        }
    }

    /// 曲线上离 q 最近的点
    pub fn nearest(&self, q: P) -> P {
        match *self {
            Curve::Seg(a, b) => lerp(a, b, param(a, b, q).clamp(0.0, 1.0)),
            Curve::Circ { c, r, .. } => polar(c, r, angle_of(sub(q, c))),
        }
    }
}

/// 圆・圆弧图形。角度给绝对角（弧度），sweep > 0 为逆时针
pub fn arc_shape(c: P, r: f64, a0: f64, sweep: f64) -> Shape {
    let full = sweep >= TAU - 1e-9;
    Shape::Arc(Arc {
        c,
        r,
        start: if full { 0.0 } else { norm_ang(a0) },
        sweep: if full { TAU } else { sweep },
        tilt: 0.0,
        flat: 1.0,
        full,
    })
}

pub fn deg(r: f64) -> f64 {
    r * 180.0 / PI
}
pub fn rad(d: f64) -> f64 {
    d * PI / 180.0
}

/// Jw_cad 的角度输入：「30」度、「//0.15」勾配、「1割5分」「3寸」勾配
pub fn parse_angle(s: &str) -> Option<f64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(k) = s.strip_prefix("//") {
        return k.trim().parse::<f64>().ok().map(|k| deg(k.atan()));
    }
    if s.contains('割') || s.contains('分') || s.contains('寸') {
        let num = |t: &str| -> f64 {
            let t: String = t
                .chars()
                .map(|c| match c {
                    '０'..='９' => char::from_u32(c as u32 - '０' as u32 + '0' as u32).unwrap(),
                    '一' => '1', '二' => '2', '三' => '3', '四' => '4', '五' => '5',
                    '六' => '6', '七' => '7', '八' => '8', '九' => '9',
                    c => c,
                })
                .collect();
            t.trim().parse().unwrap_or(0.0)
        };
        let mut k = 0.0;
        let mut rest = s;
        if let Some(i) = rest.find('割') {
            k += num(&rest[..i]) / 10.0;
            rest = &rest[i + '割'.len_utf8()..];
        }
        if let Some(i) = rest.find('寸') {
            k += num(&rest[..i]) / 10.0;
            rest = &rest[i + '寸'.len_utf8()..];
        }
        if let Some(i) = rest.find('分') {
            k += num(&rest[..i]) / 100.0;
        }
        return Some(deg(k.atan()));
    }
    s.parse::<f64>().ok()
}

/// 数值输入：「5000」或「5000,10000」「50 50」。空 = None
pub fn parse_nums(s: &str) -> Vec<f64> {
    s.split(|c: char| c == ',' || c == '，' || c == '*' || c == '×' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .filter_map(|t| t.parse::<f64>().ok())
        .collect()
}
