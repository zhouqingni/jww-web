//! 第二阶段 F 组：塗り（ソリッド）、ハッチ、測定、整理。
//!
//! 区域都用「多角形的顶点」表示（图寸），由界面上一个个点出来。

use crate::cad::*;
use crate::edit::Editor;
use crate::jww::{Attr, Shape, P};

/// 塗りの色：线色（写入线色）或任意色（RGB，Jw_cad 的 0xBBGGRR）
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FillColor {
    Pen,
    Rgb(u32),
}

/// 多角形面积（带符号，逆时针为正）
pub fn signed_area(pts: &[P]) -> f64 {
    let n = pts.len();
    (0..n).map(|k| cross(pts[k], pts[(k + 1) % n])).sum::<f64>() / 2.0
}

/// 去掉首尾重复、重合的相邻点
pub(crate) fn clean(pts: &[P]) -> Vec<P> {
    let mut v: Vec<P> = Vec::new();
    for &q in pts {
        if v.last().map_or(true, |l| dist(*l, q) > 1e-9) {
            v.push(q);
        }
    }
    while v.len() > 1 && dist(v[0], *v.last().unwrap()) < 1e-9 {
        v.pop();
    }
    v
}

/// 耳切法分三角形（凹多角形也行，自交的不保证）
pub fn triangulate(pts: &[P]) -> Vec<[P; 3]> {
    let mut v = clean(pts);
    if v.len() < 3 {
        return vec![];
    }
    if signed_area(&v) < 0.0 {
        v.reverse();
    }
    // 压在边上的也算在里面（凹角正好在对角线上时不能切）
    let inside = |q: P, a: P, b: P, c: P| {
        cross(sub(b, a), sub(q, a)) >= -1e-12 && cross(sub(c, b), sub(q, b)) >= -1e-12 && cross(sub(a, c), sub(q, c)) >= -1e-12
    };
    let mut out = Vec::new();
    let mut guard = 0;
    while v.len() > 3 && guard < 10_000 {
        guard += 1;
        let n = v.len();
        let mut cut = None;
        for i in 0..n {
            let (a, b, c) = (v[(i + n - 1) % n], v[i], v[(i + 1) % n]);
            if cross(sub(b, a), sub(c, b)) <= 1e-12 {
                continue; // 凹角或共线
            }
            if (0..n).all(|k| {
                let q = v[k];
                q == a || q == b || q == c || !inside(q, a, b, c)
            }) {
                cut = Some(i);
                break;
            }
        }
        // 找不到耳朵（共线点之类）：直接去掉一个共线点
        let i = cut.unwrap_or_else(|| {
            (0..n)
                .find(|&i| cross(sub(v[i], v[(i + n - 1) % n]), sub(v[(i + 1) % n], v[i])).abs() <= 1e-12)
                .unwrap_or(0)
        });
        if cut.is_some() {
            out.push([v[(i + n - 1) % n], v[i], v[(i + 1) % n]]);
        }
        v.remove(i);
    }
    if v.len() == 3 && signed_area(&v).abs() > 1e-12 {
        out.push([v[0], v[1], v[2]]);
    }
    out
}

/// 直线 a→(a+u) 和多角形各边的交点参数（偶奇规则，排好序）
pub(crate) fn cross_params(poly: &[P], a: P, u: P) -> Vec<f64> {
    let n = poly.len();
    let mut ts = Vec::new();
    let nrm = perp(u);
    for k in 0..n {
        let (s, e) = (poly[k], poly[(k + 1) % n]);
        let (ds, de) = (dot(sub(s, a), nrm), dot(sub(e, a), nrm));
        // 半开区间，顶点正好在线上时只算一次
        if (ds > 0.0) != (de > 0.0) {
            let x = lerp(s, e, ds / (ds - de));
            ts.push(dot(sub(x, a), u));
        }
    }
    ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
    ts
}

impl Editor {
    /// 塗り：多角形涂实，拆成三角形的ソリッド（第 4 点和第 3 点重合）。返回新图形的下标
    pub fn solid_poly(&mut self, pts: &[P], col: FillColor) -> Vec<usize> {
        let mut a: Attr = self.attr();
        a.style = 1;
        let rgb = match col {
            FillColor::Pen => None,
            FillColor::Rgb(c) => {
                a.color = 10;
                Some(c)
            }
        };
        triangulate(pts)
            .into_iter()
            .map(|[p1, p2, p3]| self.push_attr(a.clone(), Shape::Solid { pts: [p1, p2, p3, p3], rgb }))
            .collect()
    }

    /// ハッチ：多角形里面画平行线。角度（度）、间隔（实寸）。cross = 再画一组垂直的（クロス）
    pub fn hatch(&mut self, poly: &[P], deg_: f64, pitch_real: f64, cross: bool) -> Vec<usize> {
        let poly = clean(poly);
        let pitch = self.paper(pitch_real);
        if poly.len() < 3 || pitch <= 1e-9 {
            return vec![];
        }
        let mut out = Vec::new();
        let sets: &[f64] = if cross { &[0.0, 90.0] } else { &[0.0] };
        for add_deg in sets {
            let u = p(rad(deg_ + add_deg).cos(), rad(deg_ + add_deg).sin());
            let n = perp(u);
            // 以原点为基准排线，和 Jw_cad 一样不同区域的线能对齐
            let offs: Vec<f64> = poly.iter().map(|q| dot(*q, n)).collect();
            let (lo, hi) = offs.iter().fold((f64::MAX, f64::MIN), |(l, h), &o| (l.min(o), h.max(o)));
            // 正好压在边界上的线不画
            let e = pitch * 1e-6;
            let mut k = ((lo + e) / pitch).ceil() as i64;
            while (k as f64) * pitch < hi - e && out.len() < 20_000 {
                let a = mul(n, k as f64 * pitch);
                let ts = cross_params(&poly, a, u);
                for w in ts.chunks_exact(2) {
                    if w[1] - w[0] > 1e-9 {
                        out.push(self.line(add(a, mul(u, w[0])), add(a, mul(u, w[1]))));
                    }
                }
                k += 1;
            }
        }
        out
    }

    /// 整理：去掉长度 0 的线、重复的线，同属性同一直线上重叠或相接的线连成一条。
    /// 返回减少的图形数
    pub fn tidy(&mut self) -> usize {
        let before = self.doc.ents.len();
        let key = |a: &Attr| (a.color, a.style, a.width, a.layer, a.glayer, a.flag, a.group);
        // 先把要整理的线按（属性、所在直线）分组
        struct L {
            i: usize,
            u: P,
            off: f64,
            t0: f64,
            t1: f64,
        }
        let mut lines: Vec<L> = Vec::new();
        let mut drop = vec![false; before];
        for (i, e) in self.doc.ents.iter().enumerate() {
            if !self.editable(&e.a) {
                continue;
            }
            if let Shape::Line { a, b } = e.shape {
                if dist(a, b) < 1e-9 {
                    drop[i] = true;
                    continue;
                }
                // 方向统一到 x 正（垂直线 y 正）
                let mut u = norm(sub(b, a));
                if u.x < -1e-12 || (u.x.abs() <= 1e-12 && u.y < 0.0) {
                    u = mul(u, -1.0);
                }
                let (ta, tb) = (dot(a, u), dot(b, u));
                lines.push(L { i, u, off: cross(u, a), t0: ta.min(tb), t1: ta.max(tb) });
            }
        }
        const TOL: f64 = 1e-6;
        let mut used = vec![false; lines.len()];
        let mut fresh = Vec::new();
        for s in 0..lines.len() {
            if used[s] {
                continue;
            }
            let ks = key(&self.doc.ents[lines[s].i].a);
            let mut grp: Vec<usize> = (s..lines.len())
                .filter(|&k| {
                    !used[k]
                        && key(&self.doc.ents[lines[k].i].a) == ks
                        && dist(lines[k].u, lines[s].u) < 1e-9
                        && (lines[k].off - lines[s].off).abs() < TOL
                })
                .collect();
            for &k in &grp {
                used[k] = true;
            }
            if grp.len() < 2 {
                continue;
            }
            grp.sort_by(|x, y| lines[*x].t0.partial_cmp(&lines[*y].t0).unwrap());
            let (u, off) = (lines[s].u, lines[s].off);
            let at = |t: f64| add(mul(u, t), mul(perp(u), off));
            // 合并区间
            let mut cur: Option<(f64, f64, usize)> = None;
            let mut merged: Vec<(f64, f64, usize, usize)> = Vec::new(); // (t0, t1, 代表下标, 条数)
            let mut cnt = 0;
            for &k in &grp {
                let l = &lines[k];
                match cur {
                    Some((c0, c1, rep)) if l.t0 <= c1 + TOL => {
                        cur = Some((c0, c1.max(l.t1), rep));
                        cnt += 1;
                    }
                    _ => {
                        if let Some((c0, c1, rep)) = cur {
                            merged.push((c0, c1, rep, cnt));
                        }
                        cur = Some((l.t0, l.t1, l.i));
                        cnt = 1;
                    }
                }
                drop[l.i] = true;
            }
            if let Some((c0, c1, rep)) = cur {
                merged.push((c0, c1, rep, cnt));
            }
            for (t0, t1, rep, _) in merged {
                let a = self.doc.ents[rep].a.clone();
                fresh.push((rep, a, Shape::Line { a: at(t0), b: at(t1) }));
            }
        }
        // 合并后的线放回代表线原来的位置，其余删掉（保持图形顺序）
        for (rep, a, shape) in fresh {
            drop[rep] = false;
            self.doc.ents[rep].a = a;
            self.doc.ents[rep].shape = shape;
        }
        let idx: Vec<usize> = (0..before).filter(|&i| drop[i]).collect();
        self.erase(idx);
        before - self.doc.ents.len()
    }
}

/// 两条线段的交点参数（都在 0〜1 之内才算），平行返回 None
fn seg_cross(a: P, b: P, c: P, d: P) -> Option<(f64, f64)> {
    let (r, s) = (sub(b, a), sub(d, c));
    let den = cross(r, s);
    if den.abs() < 1e-12 * len(r).max(1.0) * len(s).max(1.0) {
        return None;
    }
    let t = cross(sub(c, a), s) / den;
    let u = cross(sub(c, a), r) / den;
    let e = 1e-9;
    (t > -e && t < 1.0 + e && u > -e && u < 1.0 + e).then_some((t, u))
}

/// 凸四边形（逆时针或顺时针都行）里面（严格）
fn in_quad(q: &[P; 4], x: P) -> bool {
    let s: Vec<f64> = (0..4).map(|k| cross(sub(q[(k + 1) % 4], q[k]), sub(x, q[k]))).collect();
    s.iter().all(|v| *v > 1e-12) || s.iter().all(|v| *v < -1e-12)
}

/// 同一直线上重叠・相接的线段连成一条
fn merge_collinear(segs: Vec<(P, P)>) -> Vec<(P, P)> {
    let mut keyed: Vec<(P, f64, f64, f64)> = segs
        .iter()
        .filter(|(a, b)| dist(*a, *b) > 1e-9)
        .map(|&(a, b)| {
            let mut u = norm(sub(b, a));
            if u.x < -1e-12 || (u.x.abs() <= 1e-12 && u.y < 0.0) {
                u = mul(u, -1.0);
            }
            let (ta, tb) = (dot(a, u), dot(b, u));
            (u, cross(u, a), ta.min(tb), ta.max(tb))
        })
        .collect();
    keyed.sort_by(|x, y| (x.0.x, x.0.y, x.1, x.2).partial_cmp(&(y.0.x, y.0.y, y.1, y.2)).unwrap());
    let mut out: Vec<(P, f64, f64, f64)> = Vec::new();
    for k in keyed {
        if let Some(l) = out.last_mut() {
            if dist(l.0, k.0) < 1e-9 && (l.1 - k.1).abs() < 1e-6 && k.2 <= l.3 + 1e-6 {
                l.3 = l.3.max(k.3);
                continue;
            }
        }
        out.push(k);
    }
    out.into_iter().map(|(u, off, t0, t1)| (add(mul(u, t0), mul(perp(u), off)), add(mul(u, t1), mul(perp(u), off)))).collect()
}

/// 包絡：线（两条平行线 = 一道墙）的外轮廓。成对的平行线各自围成一块，取所有块的并集的边界；
/// 不成对的线只去掉落在块里面的部分。返回处理后的线段
pub fn envelope_segs(lines: &[(P, P)]) -> Vec<(P, P)> {
    let n = lines.len();
    let dir = |i: usize| norm(sub(lines[i].1, lines[i].0));
    // 1. 配对：平行、有重叠、距离最近的两条
    let mut cand: Vec<(f64, usize, usize)> = Vec::new();
    for i in 0..n {
        for j in i + 1..n {
            let (ui, uj) = (dir(i), dir(j));
            if cross(ui, uj).abs() > 1e-9 {
                continue;
            }
            let (a, b) = lines[i];
            let d = cross(ui, sub(lines[j].0, a)).abs();
            if d < 1e-9 {
                continue;
            }
            let (t0, t1) = (0.0f64, dot(sub(b, a), ui));
            let (s0, s1) = (dot(sub(lines[j].0, a), ui), dot(sub(lines[j].1, a), ui));
            let lo = t0.max(s0.min(s1));
            let hi = t1.min(s0.max(s1));
            if hi - lo > 1e-9 {
                cand.push((d, i, j));
            }
        }
    }
    cand.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    let mut paired = vec![false; n];
    let mut quads: Vec<[P; 4]> = Vec::new();
    let mut pieces: Vec<(P, P, bool)> = Vec::new(); // (a, b, 是块的边)
    for (_, i, j) in cand {
        if paired[i] || paired[j] {
            continue;
        }
        paired[i] = true;
        paired[j] = true;
        let (a, b) = lines[i];
        let u = dir(i);
        let pr = |x: P| dot(sub(x, a), u);
        let (s0, s1) = (pr(lines[j].0), pr(lines[j].1));
        let (t_lo, t_hi) = (0.0f64.max(s0.min(s1)), pr(b).min(s0.max(s1)));
        let on_i = |t: f64| add(a, mul(u, t));
        let (c, _) = lines[j];
        let on_j = |t: f64| add(c, mul(u, t - pr(c)));
        let q = [on_i(t_lo), on_i(t_hi), on_j(t_hi), on_j(t_lo)];
        quads.push(q);
        for k in 0..4 {
            pieces.push((q[k], q[(k + 1) % 4], true));
        }
        // 重叠以外剩下的部分当普通线
        for (x, y) in [(a, b), lines[j]] {
            let (tx, ty) = (pr(x), pr(y));
            let (lo, hi) = (tx.min(ty), tx.max(ty));
            // 这条线上参数 t 的点
            let at = |t: f64| add(x, mul(u, t - tx));
            if t_lo - lo > 1e-9 {
                pieces.push((at(lo), at(t_lo), false));
            }
            if hi - t_hi > 1e-9 {
                pieces.push((at(t_hi), at(hi), false));
            }
        }
    }
    for i in 0..n {
        if !paired[i] {
            pieces.push((lines[i].0, lines[i].1, false));
        }
    }
    // 2. 在所有交点处切断
    let mut out = Vec::new();
    let inside = |x: P| quads.iter().any(|q| in_quad(q, x));
    for (k, &(a, b, edge)) in pieces.iter().enumerate() {
        let mut ts = vec![0.0, 1.0];
        for (m, &(c, d, _)) in pieces.iter().enumerate() {
            if m != k {
                if let Some((t, _)) = seg_cross(a, b, c, d) {
                    ts.push(t.clamp(0.0, 1.0));
                }
            }
        }
        ts.sort_by(|x, y| x.partial_cmp(y).unwrap());
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        let nrm = mul(perp(norm(sub(b, a))), 1e-5);
        for w in ts.windows(2) {
            let (s, e) = (lerp(a, b, w[0]), lerp(a, b, w[1]));
            if dist(s, e) < 1e-9 {
                continue;
            }
            let m = mid(s, e);
            let (l, r) = (inside(add(m, nrm)), inside(sub(m, nrm)));
            // 3. 块的边：一侧在里一侧在外才留；普通线：两侧都在里面就去掉
            let keep = if edge { l != r } else { !(l && r) };
            if keep {
                out.push((s, e));
            }
        }
    }
    merge_collinear(out)
}

impl Editor {
    /// 包絡：框 a-b 里（两端都在框内、可编辑）的线，按线色・线种・图层分组做包絡。返回新线的条数
    pub fn envelope(&mut self, a: P, b: P) -> usize {
        let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        let inside = |q: P| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1;
        let mut groups: std::collections::BTreeMap<(u16, u8, u16, u16), Vec<usize>> = Default::default();
        for (i, e) in self.doc.ents.iter().enumerate() {
            if let Shape::Line { a, b } = e.shape {
                if self.editable(&e.a) && inside(a) && inside(b) && dist(a, b) > 1e-9 {
                    groups.entry((e.a.color, e.a.style, e.a.glayer, e.a.layer)).or_default().push(i);
                }
            }
        }
        let mut drop = Vec::new();
        let mut add_ = Vec::new();
        for idx in groups.values() {
            if idx.len() < 2 {
                continue;
            }
            let ls: Vec<(P, P)> = idx
                .iter()
                .map(|&i| if let Shape::Line { a, b } = self.doc.ents[i].shape { (a, b) } else { unreachable!() })
                .collect();
            let attr = self.doc.ents[idx[0]].a.clone();
            for s in envelope_segs(&ls) {
                add_.push((attr.clone(), s));
            }
            drop.extend(idx.iter().copied());
        }
        let n = add_.len();
        self.erase(drop);
        for (a, (p1, p2)) in add_ {
            self.push_attr(a, Shape::Line { a: p1, b: p2 });
        }
        n
    }
}

/// 測定：折线的长度（图寸）
pub fn path_len(pts: &[P]) -> f64 {
    pts.windows(2).map(|w| dist(w[0], w[1])).sum()
}
