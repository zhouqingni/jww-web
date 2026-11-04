//! 照 Jw_cad 鼠标操作补的编辑：切断、线・圆弧的伸缩、端点移动、2線（可拐弯・包絡）、
//! 框内消去・中間線消去、连续线、闭合区域的ハッチ・塗り、接円、分割的各种组合、寸法的细节。
//! 坐标都是图寸（纸面 mm），长度输入是实寸。

use crate::cad::*;
use crate::edit::{side_dir, Editor};
use crate::fill::{clean, cross_params, FillColor};
use crate::jww::{Attr, Ent, Point, Shape, P};
use std::f64::consts::{PI, TAU};

/// 两点重合的判断（图寸 mm）
const EPS: f64 = 1e-4;

/// 2線的起点・终点：点（投影到基准线），或者包絡的线（和它的交点）
#[derive(Clone, Copy, Debug)]
pub enum End {
    Pt(P),
    Line(P, P),
}

/// 接円的对象
#[derive(Clone, Copy, Debug)]
pub enum TanObj {
    Line(P, P),
    Circ(P, f64),
    Pt(P),
}

impl TanObj {
    pub fn of(c: Curve) -> TanObj {
        match c {
            Curve::Seg(a, b) => TanObj::Line(a, b),
            Curve::Circ { c, r, .. } => TanObj::Circ(c, r),
        }
    }
    /// 圆心到这个对象的距离（= 接円的半径）
    fn dist(&self, q: P) -> f64 {
        match *self {
            TanObj::Line(a, b) => dist(q, project(a, b, q)),
            TanObj::Circ(c, r) => (dist(q, c) - r).abs(),
            TanObj::Pt(x) => dist(q, x),
        }
    }
    /// 接点
    fn touch(&self, c: P) -> P {
        match *self {
            TanObj::Line(a, b) => project(a, b, c),
            TanObj::Circ(o, r) => polar(o, r, angle_of(sub(c, o))),
            TanObj::Pt(x) => x,
        }
    }
    /// 圆心可能在的位置（半径 r 时）
    fn loci(&self, r: f64) -> Vec<Curve> {
        let full = |c: P, r: f64| Curve::Circ { c, r, a0: 0.0, sweep: TAU, full: true };
        match *self {
            TanObj::Line(a, b) => {
                let n = mul(perp(norm(sub(b, a))), r);
                vec![Curve::Seg(add(a, n), add(b, n)), Curve::Seg(sub(a, n), sub(b, n))]
            }
            TanObj::Circ(c, rc) => {
                let mut v = vec![full(c, rc + r)];
                if (rc - r).abs() > 1e-9 {
                    v.push(full(c, (rc - r).abs()));
                }
                v
            }
            TanObj::Pt(x) => vec![full(x, r)],
        }
    }
}

/// 线段和框的重叠部分（参数 t0..t1），没有就 None
fn clip_rect(a: P, b: P, x0: f64, x1: f64, y0: f64, y1: f64) -> Option<(f64, f64)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let d = sub(b, a);
    for (pv, qv) in [(-d.x, a.x - x0), (d.x, x1 - a.x), (-d.y, a.y - y0), (d.y, y1 - a.y)] {
        if pv.abs() < 1e-15 {
            if qv < 0.0 {
                return None;
            }
        } else {
            let t = qv / pv;
            if pv < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t1 - t0 > 1e-12).then_some((t0, t1))
}

/// 线段 a-b 在 x 处伸缩：留下 pick 那一侧，protrude（图寸）往外再突出
fn seg_to(a: P, b: P, pick: P, x: P, protrude: f64) -> (P, P) {
    let (t, tp) = (param(a, b, x), param(a, b, pick));
    let u = norm(sub(b, a));
    if t > tp {
        (a, add(x, mul(u, protrude)))
    } else {
        (sub(x, mul(u, protrude)), b)
    }
}

impl Editor {
    fn set_arc(&mut self, i: usize, a0: f64, sweep: f64) {
        if let Shape::Arc(a) = &mut self.doc.ents[i].shape {
            a.start = norm_ang(a0 - a.tilt);
            a.sweep = sweep;
            a.full = false;
        }
    }

    /// 切断：图形 i 在 q 处（投影）断开。整圆变成从 q 开始的一整圈弧
    pub fn cut_at(&mut self, i: usize, q: P) -> bool {
        let Some(c) = self.curve(i) else { return false };
        let a = self.doc.ents[i].a.clone();
        match c {
            Curve::Seg(s, t) => {
                let k = param(s, t, q);
                if !(1e-9..1.0 - 1e-9).contains(&k) {
                    return false;
                }
                let x = lerp(s, t, k);
                self.replace(i, a, vec![Shape::Line { a: s, b: x }, Shape::Line { a: x, b: t }]);
            }
            Curve::Circ { c, a0, sweep, full, .. } => {
                let ang = angle_of(sub(q, c));
                if full {
                    self.set_arc(i, ang, TAU);
                    return true;
                }
                let u = norm_ang(ang - a0);
                if u <= 1e-9 || u >= sweep - 1e-9 {
                    return false;
                }
                let e = self.doc.ents[i].clone();
                self.set_arc(i, a0, u);
                let k = self.push_attr(a, e.shape);
                self.set_arc(k, a0 + u, sweep - u);
            }
        }
        true
    }

    /// 伸縮：线・圆弧 i 伸缩到点 q（线 = 投影，圆弧 = q 的方向），留下点 pick 那一侧。
    /// protrude = 突出寸法（实寸，只对线）
    pub fn stretch_pt(&mut self, i: usize, pick: P, q: P, protrude: f64) -> bool {
        let pr = self.paper(protrude);
        match self.curve(i) {
            Some(Curve::Seg(a, b)) => {
                let (s, t) = seg_to(a, b, pick, project(a, b, q), pr);
                if dist(s, t) < 1e-9 {
                    return false;
                }
                self.doc.ents[i].shape = Shape::Line { a: s, b: t };
                true
            }
            Some(Curve::Circ { c, a0, sweep, full: false, .. }) => {
                let ap = norm_ang(angle_of(sub(pick, c)) - a0).min(sweep);
                let at = norm_ang(angle_of(sub(q, c)) - a0);
                let (s, w) = if at <= sweep {
                    if at > ap {
                        (a0, at)
                    } else {
                        (a0 + at, sweep - at)
                    }
                } else if at - sweep <= TAU - at {
                    (a0, at)
                } else {
                    (a0 + at, sweep + TAU - at)
                };
                if w < 1e-9 {
                    return false;
                }
                self.set_arc(i, s, w);
                true
            }
            _ => false,
        }
    }

    /// 伸縮：伸缩到基准线 j。基准线是圆时取离点基准线的位置 jpick 近的交点，否则离 pick 近的
    pub fn stretch_to(&mut self, i: usize, pick: P, j: usize, jpick: Option<P>, protrude: f64) -> bool {
        let (Some(c), Some(t)) = (self.curve(i), self.curve(j)) else { return false };
        let near = match (t, jpick) {
            (Curve::Circ { .. }, Some(h)) => h,
            _ => pick,
        };
        let Some(x) = c.meet(&t, true).into_iter().min_by(|u, v| dist(*u, near).total_cmp(&dist(*v, near))) else { return false };
        self.stretch_pt(i, pick, x, protrude)
    }

    /// 端点移動：线 i 离 near 近的端点移到 to
    pub fn move_end(&mut self, i: usize, near: P, to: P) -> bool {
        let Some(Curve::Seg(a, b)) = self.curve(i) else { return false };
        self.doc.ents[i].shape = if dist(near, a) <= dist(near, b) { Shape::Line { a: to, b } } else { Shape::Line { a, b: to } };
        true
    }

    /// 2線：沿基准线 bases（各段当作无限长，依次拐弯）从 s 到 t，左侧偏 d1、右侧偏 d2（实寸）。
    /// 返回左右两条折线的点；端点是包絡线时，第三个值是两条线和它的交点
    pub fn two_line_path(&self, bases: &[(P, P)], s: End, t: End, d1: f64, d2: f64) -> Option<(Vec<P>, Vec<P>, [Option<(P, P)>; 2])> {
        if bases.is_empty() {
            return None;
        }
        let (d1, d2) = (self.paper(d1), self.paper(d2));
        let n = bases.len();
        let anchor = |e: End, k: usize| -> P {
            let (a, b) = bases[k];
            match e {
                End::Pt(q) => project(a, b, q),
                End::Line(c, d) => line_line(a, b, c, d).unwrap_or(a),
            }
        };
        // 各段的方向：朝着下一个拐点（最后一段朝终点）
        let mut dirs = Vec::with_capacity(n);
        let mut from = anchor(s, 0);
        for k in 0..n {
            let (a, b) = bases[k];
            let to = if k + 1 < n { line_line(a, b, bases[k + 1].0, bases[k + 1].1)? } else { anchor(t, k) };
            let mut u = norm(sub(b, a));
            if dot(u, sub(to, from)) < 0.0 {
                u = mul(u, -1.0);
            }
            dirs.push(u);
            from = to;
        }
        // 偏移 d 的一侧（d > 0 = 前进方向的左侧）
        let side = |d: f64| -> Option<Vec<P>> {
            let off: Vec<(P, P)> = (0..n)
                .map(|k| {
                    let o = mul(perp(dirs[k]), d);
                    (add(bases[k].0, o), add(bases[k].0, add(o, dirs[k])))
                })
                .collect();
            let end = |e: End, k: usize| -> Option<P> {
                match e {
                    End::Pt(q) => Some(add(project(bases[k].0, bases[k].1, q), mul(perp(dirs[k]), d))),
                    End::Line(c, dd) => line_line(off[k].0, off[k].1, c, dd),
                }
            };
            let mut v = vec![end(s, 0)?];
            for k in 0..n - 1 {
                v.push(line_line(off[k].0, off[k].1, off[k + 1].0, off[k + 1].1)?);
            }
            v.push(end(t, n - 1)?);
            Some(v)
        };
        let l = side(d1)?;
        let r = side(-d2)?;
        let hit = |e: End, a: P, b: P| match e {
            End::Line(..) => Some((a, b)),
            End::Pt(_) => None,
        };
        let h0 = hit(s, l[0], r[0]);
        let h1 = hit(t, *l.last().unwrap(), *r.last().unwrap());
        Some((l, r, [h0, h1]))
    }

    /// 2線を作図。
    /// caps = 始点・終点の留線（Some(留線出, 実寸)：線をその分伸ばして端を閉じる）。
    /// env = 包絡する線（下标・指示した位置）：2線の外側を指示 = 2線の間を消す（開口）、
    /// 内側を指示 = 2線の間だけ残す（端を閉じる）
    pub fn two_lines_draw(
        &mut self,
        l: &[P],
        r: &[P],
        hits: [Option<(P, P)>; 2],
        env: [Option<(usize, P)>; 2],
        caps: [Option<f64>; 2],
    ) -> usize {
        let (mut l, mut r) = (l.to_vec(), r.to_vec());
        let mut cap_lines = Vec::new();
        for (k, cap) in caps.iter().enumerate() {
            let Some(out) = cap else { continue };
            if env[k].is_some() || l.len() < 2 {
                continue;
            }
            let out = self.paper(*out);
            let (i0, i1) = if k == 0 { (0, 1) } else { (l.len() - 1, l.len() - 2) };
            let u = norm(sub(l[i0], l[i1]));
            l[i0] = add(l[i0], mul(u, out));
            r[i0] = add(r[i0], mul(u, out));
            cap_lines.push((l[i0], r[i0]));
        }
        let mut n = 0;
        for side in [&l, &r] {
            for w in side.windows(2) {
                if dist(w[0], w[1]) > 1e-9 {
                    self.line(w[0], w[1]);
                    n += 1;
                }
            }
        }
        for (a, b) in cap_lines {
            self.line(a, b);
            n += 1;
        }
        // 下标大的先处理，免得 replace 删掉图形后下标错位
        let mut ev: Vec<(usize, P, (P, P))> = (0..2).filter_map(|k| Some((env[k]?.0, env[k]?.1, hits[k]?))).collect();
        ev.sort_by(|a, b| b.0.cmp(&a.0));
        for (i, pick, (a, b)) in ev {
            let Some(Curve::Seg(s, t)) = self.curve(i) else { continue };
            let (ta, tb, tp) = (param(s, t, a), param(s, t, b), param(s, t, pick));
            if (ta - tp) * (tb - tp) < 0.0 {
                // 内側 = 2線の間だけ残す
                let (lo, hi) = if ta < tb { (a, b) } else { (b, a) };
                self.doc.ents[i].shape = Shape::Line { a: lo, b: hi };
            } else {
                self.erase_part(i, a, b);
            }
        }
        n
    }

    /// 包絡「範囲内消去」：框里的线（的框内部分）消掉。返回动了几条
    pub fn erase_in_rect(&mut self, a: P, b: P) -> usize {
        self.rect_erase(a, b, false)
    }

    /// 包絡「中間線消去」：两端都在框外、穿过框的线，框内的部分消掉
    pub fn erase_through_rect(&mut self, a: P, b: P) -> usize {
        self.rect_erase(a, b, true)
    }

    fn rect_erase(&mut self, a: P, b: P, through: bool) -> usize {
        let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        let inside = |q: P| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1;
        let mut jobs = Vec::new();
        for (i, e) in self.doc.ents.iter().enumerate() {
            let Shape::Line { a: s, b: t } = e.shape else { continue };
            if !self.editable(&e.a) || (through && (inside(s) || inside(t))) {
                continue;
            }
            if let Some((t0, t1)) = clip_rect(s, t, x0, x1, y0, y1) {
                jobs.push((i, lerp(s, t, t0), lerp(s, t, t1)));
            }
        }
        let n = jobs.len();
        for (i, s, t) in jobs.into_iter().rev() {
            self.erase_part(i, s, t);
        }
        n
    }

    /// 图形的两个端点（线・圆弧）。整圆没有
    fn ends_of(&self, i: usize) -> Option<(P, P)> {
        match self.curve(i)? {
            Curve::Seg(a, b) => Some((a, b)),
            Curve::Circ { c, r, a0, sweep, full: false } => Some((polar(c, r, a0), polar(c, r, a0 + sweep))),
            _ => None,
        }
    }

    /// 连续线：和 i 端点相连（同一图层）的线・圆弧，一直连下去
    pub fn chain_from(&self, i: usize) -> Vec<usize> {
        let Some(e0) = self.doc.ents.get(i) else { return vec![] };
        let (g, l) = (e0.a.glayer, e0.a.layer);
        let cand: Vec<(usize, P, P)> = self
            .doc
            .ents
            .iter()
            .enumerate()
            .filter(|(_, e)| e.a.glayer == g && e.a.layer == l && self.editable(&e.a))
            .filter_map(|(k, _)| self.ends_of(k).map(|(a, b)| (k, a, b)))
            .collect();
        if self.ends_of(i).is_none() {
            return vec![i];
        }
        let mut got = vec![i];
        let mut open = vec![i];
        while let Some(k) = open.pop() {
            let (a, b) = self.ends_of(k).unwrap();
            for &(m, c, d) in &cand {
                if got.contains(&m) {
                    continue;
                }
                if [c, d].iter().any(|x| dist(*x, a) < EPS || dist(*x, b) < EPS) {
                    got.push(m);
                    open.push(m);
                }
            }
        }
        got
    }

    /// 闭合的连续线（每个端点都正好接两条），或单独的整圆。不闭合就 None
    pub fn closed_chain(&self, i: usize) -> Option<Vec<usize>> {
        if let Some(Curve::Circ { full: true, .. }) = self.curve(i) {
            return Some(vec![i]);
        }
        let ch = self.chain_from(i);
        let mut pts: Vec<(P, usize)> = Vec::new();
        for &k in &ch {
            let (a, b) = self.ends_of(k)?;
            for x in [a, b] {
                match pts.iter_mut().find(|(q, _)| dist(*q, x) < EPS) {
                    Some(v) => v.1 += 1,
                    None => pts.push((x, 1)),
                }
            }
        }
        (ch.len() >= 2 && pts.iter().all(|(_, c)| *c == 2)).then_some(ch)
    }

    /// 选中的线・圆弧连成闭合的多角形（圆弧细分）。整圆单独一个
    pub fn loops_of(&self, idx: &[usize]) -> Vec<Vec<P>> {
        let mut pieces: Vec<Vec<P>> = Vec::new();
        let mut loops = Vec::new();
        for &i in idx {
            match self.curve(i) {
                Some(Curve::Seg(a, b)) => pieces.push(vec![a, b]),
                Some(Curve::Circ { c, r, a0, sweep, full }) => {
                    let n = ((sweep / (PI / 36.0)).ceil() as usize).max(2);
                    let v: Vec<P> = (0..=n).map(|k| polar(c, r, a0 + sweep * k as f64 / n as f64)).collect();
                    if full {
                        loops.push(v[..n].to_vec());
                    } else {
                        pieces.push(v);
                    }
                }
                None => {}
            }
        }
        while let Some(mut cur) = pieces.pop() {
            loop {
                let end = *cur.last().unwrap();
                if cur.len() > 2 && dist(end, cur[0]) < EPS {
                    cur.pop();
                    loops.push(cur);
                    break;
                }
                let Some(k) = pieces.iter().position(|v| dist(v[0], end) < EPS || dist(*v.last().unwrap(), end) < EPS) else { break };
                let mut v = pieces.remove(k);
                if dist(v[0], end) >= EPS {
                    v.reverse();
                }
                cur.extend(v.into_iter().skip(1));
            }
        }
        loops.into_iter().filter(|v| v.len() >= 3).collect()
    }

    /// ハッチ：几个闭合多角形（偶奇规则，里面的是中抜け）
    pub fn hatch_loops(&mut self, loops: &[Vec<P>], deg_: f64, pitch_real: f64, cross: bool) -> Vec<usize> {
        let loops: Vec<Vec<P>> = loops.iter().map(|v| clean(v)).filter(|v| v.len() >= 3).collect();
        let pitch = self.paper(pitch_real);
        if loops.is_empty() || pitch <= 1e-9 {
            return vec![];
        }
        let mut out = Vec::new();
        let sets: &[f64] = if cross { &[0.0, 90.0] } else { &[0.0] };
        for add_deg in sets {
            let u = p(rad(deg_ + add_deg).cos(), rad(deg_ + add_deg).sin());
            let n = perp(u);
            let (lo, hi) = loops.iter().flatten().map(|q| dot(*q, n)).fold((f64::MAX, f64::MIN), |(l, h), o| (l.min(o), h.max(o)));
            let e = pitch * 1e-6;
            let mut k = ((lo + e) / pitch).ceil() as i64;
            while (k as f64) * pitch < hi - e && out.len() < 20_000 {
                let a = mul(n, k as f64 * pitch);
                let mut ts: Vec<f64> = loops.iter().flat_map(|v| cross_params(v, a, u)).collect();
                ts.sort_by(|x, y| x.total_cmp(y));
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

    /// 塗り：几个闭合多角形各自涂实
    pub fn solid_loops(&mut self, loops: &[Vec<P>], col: FillColor) -> Vec<usize> {
        loops.iter().flat_map(|v| self.solid_poly(v, col)).collect()
    }

    /// 接円：半径 r（图寸）给定、和两个对象相切的圆心候补
    pub fn tan_centers(objs: [TanObj; 2], r: f64) -> Vec<P> {
        let mut v: Vec<P> = Vec::new();
        for c1 in objs[0].loci(r) {
            for c2 in objs[1].loci(r) {
                for x in c1.meet(&c2, true) {
                    if !v.iter().any(|y| dist(*y, x) < 1e-7) {
                        v.push(x);
                    }
                }
            }
        }
        v
    }

    /// 接円：半径不定、和三个对象相切。picks = 点各对象的位置，取接点离点击位置最近的解
    pub fn tan_circle3(objs: [TanObj; 3], picks: [P; 3]) -> Option<(P, f64)> {
        let f = |c: P, r: f64| [objs[0].dist(c) - r, objs[1].dist(c) - r, objs[2].dist(c) - r];
        let g = mid(mid(picks[0], picks[1]), picks[2]);
        let span = picks.iter().map(|q| dist(*q, g)).fold(0.0f64, f64::max).max(1.0);
        let mut best: Option<(P, f64, f64)> = None;
        for gx in -3..=3 {
            for gy in -3..=3 {
                let mut c = add(g, p(gx as f64 * span / 2.0, gy as f64 * span / 2.0));
                let mut r = objs.iter().map(|o| o.dist(c)).sum::<f64>() / 3.0;
                let mut ok = false;
                for _ in 0..60 {
                    let f0 = f(c, r);
                    if f0.iter().all(|v| v.abs() < 1e-10) {
                        ok = true;
                        break;
                    }
                    // 数值雅可比，牛顿法
                    let h = 1e-7 * span;
                    let fx = f(add(c, p(h, 0.0)), r);
                    let fy = f(add(c, p(0.0, h)), r);
                    let fr = f(c, r + h);
                    let j: [[f64; 3]; 3] = std::array::from_fn(|k| [(fx[k] - f0[k]) / h, (fy[k] - f0[k]) / h, (fr[k] - f0[k]) / h]);
                    let Some(d) = solve3(j, f0) else { break };
                    c = sub(c, p(d[0], d[1]));
                    r -= d[2];
                }
                if !ok || r <= 1e-9 {
                    continue;
                }
                let score: f64 = (0..3).map(|k| dist(objs[k].touch(c), picks[k])).sum();
                if best.map_or(true, |b| score < b.2 - 1e-9) {
                    best = Some((c, r, score));
                }
            }
        }
        best.map(|b| (b.0, b.1))
    }

    /// 分割：线 i 和点 q 之间 n 等分，画平行线
    pub fn divide_line_point(&mut self, i: usize, q: P, n: usize) -> Vec<usize> {
        let Some(Curve::Seg(a, b)) = self.curve(i) else { return vec![] };
        let d = sub(q, project(a, b, q));
        (1..n).map(|k| {
            let o = mul(d, k as f64 / n as f64);
            self.line(add(a, o), add(b, o))
        })
        .collect()
    }

    /// 分割：两个圆（弧）之间 n 等分，中心・半径・角度按比例
    pub fn divide_circles(&mut self, i: usize, j: usize, n: usize) -> Vec<usize> {
        let (Some(Curve::Circ { c: c1, r: r1, a0: s1, sweep: w1, .. }), Some(Curve::Circ { c: c2, r: r2, a0: s2, sweep: w2, .. })) =
            (self.curve(i), self.curve(j))
        else {
            return vec![];
        };
        (1..n)
            .map(|k| {
                let t = k as f64 / n as f64;
                let ds = (s2 - s1 + PI).rem_euclid(TAU) - PI;
                self.arc(lerp(c1, c2, t), r1 + (r2 - r1) * t, s1 + ds * t, w1 + (w2 - w1) * t)
            })
            .collect()
    }

    /// 分割：圆和点之间 n 等分（中心往点移，半径按比例缩到 0）
    pub fn divide_circle_point(&mut self, i: usize, q: P, n: usize) -> Vec<usize> {
        let Some(Curve::Circ { c, r, a0, sweep, .. }) = self.curve(i) else { return vec![] };
        (1..n).map(|k| {
            let t = k as f64 / n as f64;
            self.arc(lerp(c, q, t), r * (1.0 - t), a0, sweep)
        })
        .collect()
    }

    /// 分割：线・圆上，从 s 到 t（投影）之间 n 等分打点（圆是逆时针）
    pub fn divide_along(&mut self, i: usize, s: P, t: P, n: usize) -> Vec<usize> {
        match self.curve(i) {
            Some(Curve::Seg(a, b)) => {
                let (x, y) = (project(a, b, s), project(a, b, t));
                self.divide_points(x, y, n)
            }
            Some(Curve::Circ { c, r, .. }) => {
                let a0 = angle_of(sub(s, c));
                let mut w = norm_ang(angle_of(sub(t, c)) - a0);
                if w < 1e-12 {
                    w = TAU;
                }
                (1..n).map(|k| self.point(polar(c, r, a0 + w * k as f64 / n as f64))).collect()
            }
            None => vec![],
        }
    }

    /// 連続点分割：折线 pts 按总长 n 等分打点。ends = 两端也打
    pub fn divide_polyline(&mut self, pts: &[P], n: usize, ends: bool) -> Vec<usize> {
        let total: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        if total < 1e-12 || n == 0 {
            return vec![];
        }
        let mut out = Vec::new();
        let (k0, k1) = if ends { (0, n) } else { (1, n - 1) };
        for k in k0..=k1 {
            let mut d = total * k as f64 / n as f64;
            for w in pts.windows(2) {
                let l = dist(w[0], w[1]);
                if d <= l + 1e-12 {
                    out.push(self.point(lerp(w[0], w[1], if l > 0.0 { (d / l).min(1.0) } else { 0.0 })));
                    break;
                }
                d -= l;
            }
        }
        out
    }

    /// 寸法値の数字（小数桁 dim_dec、末尾の 0 は消す、3 桁カンマ）
    pub fn fmt_dim(&self, v: f64) -> String {
        let dec = self.dim_dec.min(3) as usize;
        let k = 10f64.powi(dec as i32);
        let v = (v * k).round() / k;
        let neg = v < 0.0;
        let s = format!("{:.dec$}", v.abs());
        let (int, frac) = s.split_once('.').unwrap_or((&s, ""));
        let mut o = String::new();
        for (j, ch) in int.chars().enumerate() {
            if j > 0 && (int.len() - j) % 3 == 0 {
                o.push(',');
            }
            o.push(ch);
        }
        let frac = frac.trim_end_matches('0');
        if !frac.is_empty() {
            o.push('.');
            o.push_str(frac);
        }
        if neg {
            o.insert(0, '-');
        }
        o
    }

    /// 寸法の端部：点・矢印（外向き out の端で内向きに開く）・逆矢印
    pub(crate) fn dim_end_mark(&mut self, q: P, out: P, a: &Attr) -> Vec<usize> {
        let u = norm(out);
        match self.dim_end {
            1 | 2 => {
                // 矢印 3mm・15°
                let back = if self.dim_end == 1 { mul(u, -1.0) } else { u };
                let mut la = a.clone();
                la.flag = 0x2000;
                let mut v = Vec::new();
                for s in [1.0, -1.0] {
                    let d = rot(back, rad(15.0) * s);
                    v.push(self.push_attr(la.clone(), Shape::Line { a: q, b: add(q, mul(d, 3.0)) }));
                }
                v
            }
            _ => {
                let mut pa = a.clone();
                pa.style = 1;
                pa.flag = 0x40;
                vec![self.push_attr(pa, Shape::Point(Point { p: q, temp: false, code: 0, angle: 0.0, scale: 1.0 }))]
            }
        }
    }

    /// 測点 pp から寸法線（at を通る、角度 ang）への引出線：（寸法線上の足, 引出線の始まり）。
    /// toward = 図形側の点（引出線の向きを決める）
    fn dim_foot(&self, pp: P, ang: f64, at: P, lead: Option<P>, toward: P) -> (P, P) {
        let u = p(ang.cos(), ang.sin());
        let n = perp(u);
        let off = dot(at, n);
        let nn = if dot(sub(at, toward), n) >= 0.0 { n } else { mul(n, -1.0) };
        // 引出角：法線から傾ける
        let v = rot(nn, rad(self.dim_lead_ang));
        let hit = |base: P, o: f64| -> P {
            let dv = dot(v, n);
            if dv.abs() < 1e-9 {
                return base;
            }
            add(base, mul(v, (o - dot(base, n)) / dv))
        };
        let q = hit(pp, off);
        let s = match lead {
            Some(l) => hit(pp, dot(l, n)),
            None => pp,
        };
        (q, s)
    }

    /// 寸法のグループ番号：Jw_cad と同じく -1, -2, … と下へ数える（寸法ごとに一つ）
    pub fn dim_group(&self) -> u32 {
        self.doc.ents.iter().map(|e| e.a.group).filter(|g| *g >= 0x8000_0000).min().map_or(u32::MAX, |g| g - 1)
    }

    /// 寸法（引出線の始点の位置 lead 付き）：引出線は lead の線から寸法線まで（lead = None なら測点から）。
    /// 図面の設定が「寸法図形」なら、寸法線と値を一つの寸法図形にする（Jw_cad と同じ並び：点・点・引出線・引出線・寸法図形）
    pub fn dim_lead(&mut self, p1: P, p2: P, ang: f64, at: P, lead: Option<P>) -> Vec<usize> {
        let (q1, s1) = self.dim_foot(p1, ang, at, lead, p1);
        let (q2, s2) = self.dim_foot(p2, ang, at, lead, p1);
        let real = self.real(dist(q1, q2));
        let (c_dim, c_lead, c_pt) = self.dim_colors();
        let g = self.dim_group();
        let mut a = self.attr();
        a.group = g;
        let mut out = Vec::new();
        let d = norm(sub(q2, q1));
        // 端部（点は矢印・点色）
        let mut pa = a.clone();
        pa.color = c_pt;
        out.extend(self.dim_end_mark(q1, mul(d, -1.0), &pa));
        out.extend(self.dim_end_mark(q2, d, &pa));
        // 引出線：寸法線から引出線の始点へ
        let mut la = a.clone();
        la.color = c_lead;
        la.flag = 0x2000;
        for (s, qq) in [(s1, q1), (s2, q2)] {
            if dist(qq, s) > 1e-6 {
                out.push(self.push_attr(la.clone(), Shape::Line { a: qq, b: s }));
            }
        }
        let txt = self.fmt_dim(real);
        let mut da = a.clone();
        da.color = c_dim;
        da.flag = 0x2000;
        if self.dim_fig() {
            let k = self.dim_text(q1, q2, &txt, &a);
            let text = self.doc.ents.remove(k);
            let mut inner = da.clone();
            inner.group = 0;
            let mut ta = text.a.clone();
            ta.group = 0;
            let zero = |sh: Shape| Box::new(Ent { a: Attr { flag: 0, ..inner.clone() }, shape: sh });
            let pt0 = || Shape::Point(Point { p: P::default(), temp: false, code: 0, angle: 0.0, scale: 1.0 });
            let ln0 = || Shape::Line { a: P::default(), b: P::default() };
            let dim = crate::jww::Dim {
                line: Box::new(Ent { a: inner.clone(), shape: Shape::Line { a: q1, b: q2 } }),
                text: Box::new(Ent { a: ta, shape: text.shape }),
                sxf: Some(crate::jww::DimSxf { mode: 0, lines: [zero(ln0()), zero(ln0())], points: [zero(pt0()), zero(pt0()), zero(pt0()), zero(pt0())] }),
            };
            out.push(self.push_attr(da, Shape::Dim(dim)));
        } else {
            out.push(self.push_attr(da, Shape::Line { a: q1, b: q2 }));
            out.push(self.dim_text(q1, q2, &txt, &a));
        }
        out
    }

    /// 累進寸法の１区間：起点 base から p2 までの累計を p2 の位置に書く。prev = 直前の点、first = 最初の区間
    #[allow(clippy::too_many_arguments)]
    pub fn dim_cumulative(&mut self, base: P, prev: P, p2: P, ang: f64, at: P, lead: Option<P>, first: bool) -> Vec<usize> {
        let (q0, s0) = self.dim_foot(base, ang, at, lead, base);
        let (qp, _) = self.dim_foot(prev, ang, at, lead, base);
        let (q2, s2) = self.dim_foot(p2, ang, at, lead, base);
        let a = self.attr();
        let mut la = a.clone();
        la.flag = 0x2000;
        let mut out = Vec::new();
        let mut legs = vec![(s2, q2)];
        if first {
            legs.push((s0, q0));
        }
        for (s, qq) in legs {
            let v = sub(qq, s);
            if len(v) > 1e-6 {
                out.push(self.push_attr(la.clone(), Shape::Line { a: s, b: add(qq, mul(norm(v), 1.0)) }));
            }
        }
        out.push(self.push_attr(la.clone(), Shape::Line { a: qp, b: q2 }));
        let d = norm(sub(q2, qp));
        if first {
            out.extend(self.dim_end_mark(q0, mul(d, -1.0), &a));
            let z = self.fmt_dim(0.0);
            out.push(self.cum_text(q0, d, &z, &a));
        }
        out.extend(self.dim_end_mark(q2, d, &a));
        let txt = self.fmt_dim(self.real(dist(q0, q2)));
        out.push(self.cum_text(q2, d, &txt, &a));
        out
    }

    /// 累進寸法の値：その点から、寸法線と直角に立てて書く
    fn cum_text(&mut self, q: P, d: P, s: &str, a: &Attr) -> usize {
        let n = perp(d);
        let up = if n.y > 1e-9 || (n.y.abs() <= 1e-9 && n.x < 0.0) { n } else { mul(n, -1.0) };
        let m = self.doc.moji[2];
        let w = ((m.w + m.gap) * s.chars().count() as f64).max(1.0);
        let st = add(q, mul(up, 1.0));
        self.dim_text(st, add(st, mul(up, w)), s, a)
    }

    /// 寸法値だけ：s から t の向きに、その長さの値を書く
    pub fn dim_value_only(&mut self, s0: P, t0: P) -> usize {
        let a = self.attr();
        let txt = self.fmt_dim(self.real(dist(s0, t0)));
        self.dim_text(s0, t0, &txt, &a)
    }

    /// 半径・直径寸法：outside = 寸法値を円の外側に（右クリック）
    pub fn dim_radius_side(&mut self, i: usize, toward: P, diameter: bool, outside: bool) -> Vec<usize> {
        if !outside {
            return self.dim_radius(i, toward, diameter);
        }
        let Some(Curve::Circ { c, r, .. }) = self.curve(i) else { return vec![] };
        let u = norm(sub(toward, c));
        let q2 = add(c, mul(u, r));
        let q1 = if diameter { sub(c, mul(u, r)) } else { c };
        let real = self.real(if diameter { 2.0 * r } else { r });
        let s = if diameter { format!("φ{}", self.fmt_dim(real)) } else { format!("R{}", self.fmt_dim(real)) };
        let a = self.attr();
        let m = self.doc.moji[2];
        let w = (m.w + m.gap) * s.chars().count() as f64 + 2.0;
        let q3 = add(q2, mul(u, w));
        let mut la = a.clone();
        la.flag = 0x2000;
        let mut out = vec![self.push_attr(la, Shape::Line { a: q1, b: q3 })];
        out.push(self.dim_text(add(q2, mul(u, 1.0)), q3, &s, &a));
        out
    }

    /// 円周・角度寸法の共通：中心 c、寸法線の半径 rd、引出線の始まり半径 rl（None = なし）、
    /// start から逆時針 sweep。text = 寸法値
    pub fn dim_arc_lead(&mut self, c: P, rd: f64, rl: Option<f64>, start: f64, sweep: f64, text: &str) -> Vec<usize> {
        let mut out = Vec::new();
        if let Some(rl) = rl {
            let mut la = self.attr();
            la.flag = 0x2000;
            for t in [start, start + sweep] {
                let ext = if rd >= rl { 1.0 } else { -1.0 };
                out.push(self.push_attr(la.clone(), Shape::Line { a: polar(c, rl, t), b: polar(c, rd + ext, t) }));
            }
        }
        out.extend(self.dim_arc(c, rd, start, sweep, text));
        out
    }

    /// 2線間の角度：线 i（点 pi）から逆時針に线 j（点 pj）まで、寸法線は at を通る
    pub fn dim_two_lines(&mut self, i: usize, pi: P, j: usize, pj: P, at: P) -> Vec<usize> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (self.curve(i), self.curve(j)) else { return vec![] };
        let Some(x) = line_line(a1, b1, a2, b2) else { return vec![] };
        let (u1, u2) = (side_dir(a1, b1, x, pi), side_dir(a2, b2, x, pj));
        let s = angle_of(u1);
        let w = norm_ang(angle_of(u2) - s);
        let txt = format!("{}°", crate::edit::fmt_num(deg(w), 2));
        self.dim_arc(x, dist(x, at).max(1e-6), s, w, &txt)
    }

    /// 文字の複写：文字 i を q（左下）に写す
    pub fn copy_text_to(&mut self, i: usize, q: P, s: &str) -> Option<usize> {
        let Some(Ent { a, shape: Shape::Text(t) }) = self.doc.ents.get(i).cloned() else { return None };
        let mut t = t;
        let d = sub(q, t.start);
        t.start = q;
        t.end = add(t.end, d);
        if t.s != s {
            // 内容を変えたら長さを付け直す
            let k = dist(t.start, t.end) / t.s.chars().count().max(1) as f64;
            let u = norm(sub(t.end, t.start));
            t.s = s.to_string();
            t.end = add(t.start, mul(u, k * s.chars().count() as f64));
        }
        let a: Attr = a;
        Some(self.push_attr(a, Shape::Text(t)))
    }
}

/// 3×3 一次方程式 j·x = f
fn solve3(j: [[f64; 3]; 3], f: [f64; 3]) -> Option<[f64; 3]> {
    let det = |m: [[f64; 3]; 3]| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let d = det(j);
    if d.abs() < 1e-18 {
        return None;
    }
    let mut x = [0.0; 3];
    for k in 0..3 {
        let mut m = j;
        for r in 0..3 {
            m[r][k] = f[r];
        }
        x[k] = det(m) / d;
    }
    Some(x)
}

/// 分割の位置（割合 ts、0〜1）で作図する
impl Editor {
    /// ２点間：点
    pub fn div_points_ts(&mut self, a: P, b: P, ts: &[f64]) -> Vec<usize> {
        ts.iter().map(|&t| self.point(lerp(a, b, t))).collect()
    }
    /// ２線間：線（rev = 逆分割、B の端を逆に対応させる）
    pub fn div_lines_ts(&mut self, i: usize, j: usize, ts: &[f64], rev: bool) -> Vec<usize> {
        let Some((a1, b1, a2, b2)) = self.line_pair(i, j, rev) else { return vec![] };
        ts.iter().map(|&t| self.line(lerp(a1, a2, t), lerp(b1, b2, t))).collect()
    }
    /// 線 i と線 j の端の対応（向きをそろえる。rev なら逆）
    fn line_pair(&self, i: usize, j: usize, rev: bool) -> Option<(P, P, P, P)> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(mut a2, mut b2))) = (self.curve(i), self.curve(j)) else { return None };
        if (dot(sub(b1, a1), sub(b2, a2)) < 0.0) != rev {
            std::mem::swap(&mut a2, &mut b2);
        }
        Some((a1, b1, a2, b2))
    }
    /// 線と点：平行線
    pub fn div_line_point_ts(&mut self, i: usize, q: P, ts: &[f64]) -> Vec<usize> {
        let Some(Curve::Seg(a, b)) = self.curve(i) else { return vec![] };
        let d = sub(q, project(a, b, q));
        ts.iter().map(|&t| self.line(add(a, mul(d, t)), add(b, mul(d, t)))).collect()
    }
    /// ２円間：中心・半径・角度を比例
    pub fn div_circles_ts(&mut self, i: usize, j: usize, ts: &[f64]) -> Vec<usize> {
        let (Some(Curve::Circ { c: c1, r: r1, a0: s1, sweep: w1, .. }), Some(Curve::Circ { c: c2, r: r2, a0: s2, sweep: w2, .. })) =
            (self.curve(i), self.curve(j))
        else {
            return vec![];
        };
        let ds = (s2 - s1 + PI).rem_euclid(TAU) - PI;
        ts.iter().map(|&t| self.arc(lerp(c1, c2, t), r1 + (r2 - r1) * t, s1 + ds * t, w1 + (w2 - w1) * t)).collect()
    }
    /// 円と点
    pub fn div_circle_point_ts(&mut self, i: usize, q: P, ts: &[f64]) -> Vec<usize> {
        let Some(Curve::Circ { c, r, a0, sweep, .. }) = self.curve(i) else { return vec![] };
        ts.iter().map(|&t| self.arc(lerp(c, q, t), r * (1.0 - t), a0, sweep)).collect()
    }
    /// 線上・円周上の s〜t（円は左回り）
    pub fn div_along_ts(&mut self, i: usize, s: P, t: P, ts: &[f64]) -> Vec<usize> {
        match self.curve(i) {
            Some(Curve::Seg(a, b)) => {
                let (x, y) = (project(a, b, s), project(a, b, t));
                self.div_points_ts(x, y, ts)
            }
            Some(Curve::Circ { c, r, .. }) => {
                let a0 = angle_of(sub(s, c));
                let mut w = norm_ang(angle_of(sub(t, c)) - a0);
                if w < 1e-12 {
                    w = TAU;
                }
                ts.iter().map(|&k| self.point(polar(c, r, a0 + w * k))).collect()
            }
            None => vec![],
        }
    }
    /// 線上・円周上の s〜t の長さ（図寸）
    pub fn along_len(&self, i: usize, s: P, t: P) -> f64 {
        match self.curve(i) {
            Some(Curve::Seg(a, b)) => dist(project(a, b, s), project(a, b, t)),
            Some(Curve::Circ { c, r, .. }) => {
                let w = norm_ang(angle_of(sub(t, c)) - angle_of(sub(s, c)));
                r * if w < 1e-12 { TAU } else { w }
            }
            None => 0.0,
        }
    }
    /// 円全周分割：s（円周上へ投影）から、ccw なら左回りに一周
    pub fn div_full_circle(&mut self, i: usize, s: P, ccw: bool, ts: &[f64]) -> Vec<usize> {
        let Some(Curve::Circ { c, r, .. }) = self.curve(i) else { return vec![] };
        let a0 = angle_of(sub(s, c));
        let k = if ccw { TAU } else { -TAU };
        let mut v = vec![self.point(polar(c, r, a0))];
        v.extend(ts.iter().map(|&t| self.point(polar(c, r, a0 + k * t))));
        v
    }
    /// 折線上：全長に対する割合
    pub fn div_polyline_ts(&mut self, pts: &[P], ts: &[f64]) -> Vec<usize> {
        let total: f64 = pts.windows(2).map(|w| dist(w[0], w[1])).sum();
        let mut out = Vec::new();
        for &k in ts {
            let mut d = total * k;
            for w in pts.windows(2) {
                let l = dist(w[0], w[1]);
                if d <= l + 1e-12 {
                    out.push(self.point(lerp(w[0], w[1], if l > 0.0 { (d / l).min(1.0) } else { 0.0 })));
                    break;
                }
                d -= l;
            }
        }
        out
    }
    /// 等角度分割：交わる２線の間の角を分ける線（交点から、長さは両端の長さを比例）
    pub fn div_angle_lines(&mut self, i: usize, pi: P, j: usize, pj: P, ts: &[f64]) -> Vec<usize> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (self.curve(i), self.curve(j)) else { return vec![] };
        let Some(x) = line_line(a1, b1, a2, b2) else { return vec![] };
        let (u1, u2) = (side_dir(a1, b1, x, pi), side_dir(a2, b2, x, pj));
        let far = |a: P, b: P| dist(a, x).max(dist(b, x));
        let (l1, l2) = (far(a1, b1), far(a2, b2));
        let s = angle_of(u1);
        let mut w = norm_ang(angle_of(u2) - s);
        if w > PI {
            w -= TAU;
        }
        ts.iter().map(|&t| self.line(x, polar(x, l1 + (l2 - l1) * t, s + w * t))).collect()
    }
    /// 馬目地分割：２線間を rows（割合）で分けて、各段に縦目地 m 本を半分ずつずらして入れる
    pub fn div_umame(&mut self, i: usize, j: usize, ts: &[f64], m: usize, rev: bool) -> Vec<usize> {
        let Some((a1, b1, a2, b2)) = self.line_pair(i, j, rev) else { return vec![] };
        let mut out = Vec::new();
        let mut rows = vec![0.0];
        rows.extend_from_slice(ts);
        rows.push(1.0);
        for &t in ts {
            out.push(self.line(lerp(a1, a2, t), lerp(b1, b2, t)));
        }
        let m = m.max(1);
        for (k, w) in rows.windows(2).enumerate() {
            let (s0, s1) = (lerp(a1, a2, w[0]), lerp(b1, b2, w[0]));
            let (e0, e1) = (lerp(a1, a2, w[1]), lerp(b1, b2, w[1]));
            let shift = if k % 2 == 1 { 0.5 } else { 0.0 };
            for c in 0..=m {
                let u = (c as f64 + shift) / m as f64;
                if u <= 1e-9 || u >= 1.0 - 1e-9 {
                    continue;
                }
                out.push(self.line(lerp(s0, s1, u), lerp(e0, e1, u)));
            }
        }
        out
    }
}

/// 文字の「連」（連結・切断）と円環ソリッド
impl Editor {
    fn text_of(&self, i: usize) -> Option<crate::jww::Text> {
        match self.doc.ents.get(i).map(|e| &e.shape) {
            Some(Shape::Text(t)) => Some(t.clone()),
            _ => None,
        }
    }

    /// 文字 t の先頭 k 文字の幅（図寸）
    fn text_width(t: &crate::jww::Text, k: usize) -> f64 {
        let pre: String = t.s.chars().take(k).collect();
        let n = crate::edit::make_text(t.start, &pre, t.size_x, t.size_y, t.gap, t.deg, t.kind, &t.font);
        dist(n.start, n.end)
    }

    /// 連結：文字 j を文字 i の前（front）か後ろにつなぐ。keep = j を残す（複写して連結）
    pub fn text_join(&mut self, i: usize, j: usize, front: bool, keep: bool) -> bool {
        let (Some(a), Some(b)) = (self.text_of(i), self.text_of(j)) else { return false };
        if i == j {
            return false;
        }
        let s = if front { format!("{}{}", b.s, a.s) } else { format!("{}{}", a.s, b.s) };
        let mut start = a.start;
        if front {
            // 前付け：元の文字の位置はそのまま、前に伸ばす
            let mut tmp = a.clone();
            tmp.s = b.s.clone();
            let w = Self::text_width(&tmp, usize::MAX);
            let u = p(rad(a.deg).cos(), rad(a.deg).sin());
            start = sub(a.start, mul(u, w));
        }
        let n = crate::edit::make_text(start, &s, a.size_x, a.size_y, a.gap, a.deg, a.kind, &a.font);
        if let Shape::Text(t) = &mut self.doc.ents[i].shape {
            t.s = n.s;
            t.start = n.start;
            t.end = n.end;
        }
        if !keep {
            self.erase(vec![j]);
        }
        true
    }

    /// 切断：文字 i を q に近い文字の切れ目で二つに分ける
    pub fn text_cut(&mut self, i: usize, q: P) -> bool {
        let Some(t) = self.text_of(i) else { return false };
        let n = t.s.chars().count();
        let u = p(rad(t.deg).cos(), rad(t.deg).sin());
        let x = dot(sub(q, t.start), u);
        let k = (1..n).min_by(|a, b| (Self::text_width(&t, *a) - x).abs().total_cmp(&(Self::text_width(&t, *b) - x).abs()));
        let Some(k) = k else { return false };
        let head: String = t.s.chars().take(k).collect();
        let tail: String = t.s.chars().skip(k).collect();
        let w = Self::text_width(&t, k);
        let a = self.doc.ents[i].a.clone();
        self.set_text(i, &head);
        let st = add(t.start, mul(u, w));
        let mut nt = crate::edit::make_text(st, &tail, t.size_x, t.size_y, t.gap, t.deg, t.kind, &t.font);
        nt.italic = t.italic;
        nt.bold = t.bold;
        self.push_attr(a, Shape::Text(nt));
        true
    }

    /// 円環ソリッド：円（弧）i の内側半径 r_in（実寸）までの輪
    pub fn solid_ring(&mut self, i: usize, r_in: f64, col: crate::fill::FillColor) -> Vec<usize> {
        let Some(Curve::Circ { c, r, a0, sweep, .. }) = self.curve(i) else { return vec![] };
        let ri = self.paper(r_in);
        if ri <= 0.0 || ri >= r {
            return vec![];
        }
        let n = ((sweep / (PI / 36.0)).ceil() as usize).max(2);
        let mut out = Vec::new();
        for k in 0..n {
            let (t0, t1) = (a0 + sweep * k as f64 / n as f64, a0 + sweep * (k + 1) as f64 / n as f64);
            let quad = [polar(c, r, t0), polar(c, r, t1), polar(c, ri, t1), polar(c, ri, t0)];
            out.extend(self.solid_poly(&quad, col));
        }
        out
    }
}
