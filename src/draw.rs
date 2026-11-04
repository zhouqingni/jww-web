//! 把 .jww 的图形展开成"线、面、点、字形"，交给画面或 PDF。
//! 画面和 PDF 共用这一份逻辑，屏幕上看到的和打出来的才会一致。
//!
//! 所有坐标都是图寸（纸面 mm，原点在纸中心）。

use crate::font::{Fonts, Glyph};
use crate::geom::{flatten_arc, Affine, BBox};
use crate::jww::{is_half, Attr, Doc, Ent, Point, Shape, Text, P};
use std::collections::HashMap;
use std::f64::consts::TAU;

#[derive(Clone, Copy, Debug)]
pub enum Col {
    /// 线色号（查 Doc.pens）
    Pen(u16),
    /// COLORREF
    Rgb(u32),
}

pub trait Sink {
    /// 曲线展开的弦高容差（图寸 mm）
    fn tol(&self) -> f64;
    /// 块整体在不在可见范围（画面用来跳过看不见的块）
    fn wants(&self, _bb: &BBox) -> bool {
        true
    }
    fn line(&mut self, pts: &[P], a: &Attr);
    fn fill(&mut self, rings: &[Vec<P>], col: Col, a: &Attr);
    fn point(&mut self, p: P, a: &Attr, pt: &Point);
    fn glyph(&mut self, g: &Glyph, m: &Affine, a: &Attr);
}

/// 图层可见性：块里面的东西跟着块所在的图层走
pub fn layer_key(a: &Attr) -> usize {
    (a.glayer as usize & 15) * 16 + (a.layer as usize & 15)
}

pub struct Walker<'a> {
    pub doc: &'a Doc,
    pub fonts: &'a Fonts,
    blocks: HashMap<u32, usize>,
}

impl<'a> Walker<'a> {
    pub fn new(doc: &'a Doc, fonts: &'a Fonts) -> Self {
        let blocks = doc.blocks.iter().enumerate().map(|(i, b)| (b.number, i)).collect();
        Walker { doc, fonts, blocks }
    }

    /// 画一个图形。`top` 是最外层图形的属性（块里的东西用它来定图层）
    pub fn ent(&self, e: &Ent, m: &Affine, top: &Attr, depth: u32, s: &mut dyn Sink) {
        match &e.shape {
            Shape::Line { a, b } => s.line(&[m.apply(*a), m.apply(*b)], &self.attr(e, top)),
            Shape::Arc(arc) => {
                let tol = s.tol() / m.max_scale().max(1e-12);
                let (st, sw) = if arc.full { (0.0, TAU) } else { (arc.start, arc.sweep) };
                let mut pts = Vec::new();
                flatten_arc(arc.c, arc.r, arc.flat, arc.tilt, st, sw, tol, &mut pts);
                for p in pts.iter_mut() {
                    *p = m.apply(*p);
                }
                s.line(&pts, &self.attr(e, top));
            }
            Shape::Point(pt) => {
                let mut q = pt.clone();
                q.p = m.apply(pt.p);
                s.point(q.p, &self.attr(e, top), &q);
            }
            Shape::Text(t) => self.text(t, m, &self.attr(e, top), e.a.flag, s),
            Shape::Dim(d) => {
                self.ent(&d.line, m, top, depth, s);
                self.ent(&d.text, m, top, depth, s);
                if let Some(x) = &d.sxf {
                    if x.mode != 0 {
                        for l in &x.lines {
                            self.ent(l, m, top, depth, s);
                        }
                        for p in &x.points {
                            self.ent(p, m, top, depth, s);
                        }
                    }
                }
            }
            Shape::Solid { pts, rgb } => {
                let col = match rgb {
                    Some(c) => Col::Rgb(*c),
                    None => Col::Pen(e.a.color),
                };
                let ring = if e.a.style >= 101 {
                    self.solid_circle(e.a.style, pts, s.tol() / m.max_scale().max(1e-12))
                } else {
                    vec![pts.to_vec()]
                };
                let ring: Vec<Vec<P>> = ring.into_iter().map(|r| r.into_iter().map(|p| m.apply(p)).collect()).collect();
                s.fill(&ring, col, &self.attr(e, top));
            }
            Shape::Block { base, sx, sy, rot, number } => {
                if depth > 16 {
                    return;
                }
                let Some(&i) = self.blocks.get(number) else { return };
                let mm = m
                    .then(&Affine::translate(base.x, base.y))
                    .then(&Affine::rotate(*rot))
                    .then(&Affine::scale(*sx, *sy));
                let top = if depth == 0 { &e.a } else { top };
                for c in &self.doc.blocks[i].ents {
                    self.ent(c, &mm, top, depth + 1, s);
                }
            }
        }
    }

    /// 块里面的图形保留自己的线色・线种，图层用最外层的
    fn attr(&self, e: &Ent, top: &Attr) -> Attr {
        let mut a = e.a.clone();
        a.layer = top.layer;
        a.glayer = top.glayer;
        a
    }

    /// 圆实体（线种 101 以上）。pts = m_start, m_end, m_DPoint2, m_DPoint3
    fn solid_circle(&self, style: u8, pts: &[P; 4], tol: f64) -> Vec<Vec<P>> {
        let c = pts[0];
        let (r, flat) = (pts[1].x, pts[1].y);
        let (tilt, start) = (pts[2].x, pts[2].y);
        let (sweep, kind) = (pts[3].x, pts[3].y);
        let flat = if flat == 0.0 { 1.0 } else { flat };
        let mut outer = Vec::new();
        match style {
            105 | 106 => {
                let full = (sweep.abs() - TAU).abs() < 1e-9 || sweep == 0.0;
                let sw = if full { TAU } else { sweep };
                flatten_arc(c, r, flat, tilt, start, sw, tol, &mut outer);
                let rin = kind;
                let fin = if style == 106 && r > 0.0 { (r * flat - (r - rin)) / rin.max(1e-12) } else { flat };
                let mut inner = Vec::new();
                flatten_arc(c, rin, fin, tilt, start, sw, tol, &mut inner);
                if full {
                    return vec![outer, inner];
                }
                inner.reverse();
                outer.extend(inner);
                vec![outer]
            }
            _ => {
                if kind >= 100.0 {
                    flatten_arc(c, r, flat, tilt, 0.0, TAU, tol, &mut outer);
                } else {
                    flatten_arc(c, r, flat, tilt, start, sweep, tol, &mut outer);
                    if kind == 0.0 {
                        outer.push(c); // 扇形
                    } else if kind < 0.0 {
                        // 外侧圆弧实体：圆弧与两端切线所夹的部分
                        let mid = start + sweep / 2.0;
                        let k = r / (sweep / 2.0).cos().max(1e-6);
                        let corner = crate::geom::ellipse_pt(c, k, flat, tilt, mid);
                        outer.push(corner);
                    }
                    // 5 = 弓形：直接闭合即可
                }
                vec![outer]
            }
        }
    }

    /// 文字：每个字拉伸到「横宽 × 高」的格子里。
    /// 第 i 个字从 Σ(前面各字的 c×(横宽+字距)) 开始，c = 全角 1 / 半角 0.5
    fn text(&self, t: &Text, m: &Affine, a: &Attr, flag: u16, s: &mut dyn Sink) {
        if t.s.starts_with("^@") {
            return; // 图像等特殊文字，以后再做
        }
        let Some(face) = self.fonts.pick(&t.font) else { return };
        let desc = self.fonts.desc(face) as f64;
        let vertical = flag & 0x20 != 0;
        let base = m.then(&Affine::translate(t.start.x, t.start.y)).then(&Affine::rotate(t.deg.to_radians()));
        let mut u = 0.0;
        for ch in t.s.chars() {
            let c = if is_half(ch) { 0.5 } else { 1.0 };
            let cell = c * t.size_x;
            if let Some(g) = self.fonts.glyph(face, ch) {
                let dx = (cell - g.adv as f64 * t.size_x) / 2.0;
                let mut lm = if vertical {
                    // 縦字：从起点往下排，格子高 = 横宽
                    base.then(&Affine::translate(dx.max(0.0), -u - t.size_y + desc * t.size_y))
                } else {
                    base.then(&Affine::translate(u + dx, desc * t.size_y))
                };
                lm = lm.then(&Affine::scale(t.size_x, t.size_y));
                if t.italic {
                    lm = lm.then(&Affine::shear_x(0.25));
                }
                if !g.cmds.is_empty() {
                    s.glyph(&g, &lm, a);
                }
            }
            u += if vertical { t.size_y + t.gap } else { c * (t.size_x + t.gap) };
        }
    }
}

/// 只量范围的 Sink
pub struct BoxSink {
    pub bb: BBox,
}

impl Sink for BoxSink {
    fn tol(&self) -> f64 {
        0.5
    }
    fn line(&mut self, pts: &[P], _: &Attr) {
        for p in pts {
            self.bb.add(*p);
        }
    }
    fn fill(&mut self, rings: &[Vec<P>], _: Col, _: &Attr) {
        for r in rings {
            for p in r {
                self.bb.add(*p);
            }
        }
    }
    fn point(&mut self, p: P, _: &Attr, _: &Point) {
        self.bb.add(p);
    }
    fn glyph(&mut self, g: &Glyph, m: &Affine, _: &Attr) {
        for (x, y) in [(0.0, -0.2), (g.adv as f64, -0.2), (0.0, 1.0), (g.adv as f64, 1.0)] {
            self.bb.add(m.apply(P { x, y }));
        }
    }
}

/// 每个最外层图形的范围（画面上只画看得见的）
pub fn bboxes(w: &Walker) -> (Vec<BBox>, BBox) {
    let mut all = BBox::EMPTY;
    let v = w
        .doc
        .ents
        .iter()
        .map(|e| {
            let mut s = BoxSink { bb: BBox::EMPTY };
            w.ent(e, &Affine::ID, &e.a, 0, &mut s);
            all.union(&s.bb);
            s.bb
        })
        .collect();
    (v, all)
}

/// 图的"主要范围"：去掉离群的零星图形（远处孤零零的点、误操作的线）。
/// 按图形中心坐标取 0.5%〜99.5% 分位，再把中心落在里面的图形范围合起来
pub fn main_box(boxes: &[BBox]) -> BBox {
    let ok: Vec<&BBox> = boxes.iter().filter(|b| !b.is_empty()).collect();
    if ok.len() < 50 {
        let mut all = BBox::EMPTY;
        for b in ok {
            all.union(b);
        }
        return all;
    }
    let q = |mut v: Vec<f64>| {
        v.sort_by(|a, b| a.total_cmp(b));
        let n = v.len();
        (v[n / 200], v[n - 1 - n / 200])
    };
    let (x0, x1) = q(ok.iter().map(|b| (b.x0 + b.x1) / 2.0).collect());
    let (y0, y1) = q(ok.iter().map(|b| (b.y0 + b.y1) / 2.0).collect());
    // 分位范围再放宽一半，免得把边上的图框切掉
    let (mx, my) = ((x1 - x0) * 0.5, (y1 - y0) * 0.5);
    let mut all = BBox::EMPTY;
    for b in ok {
        let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        if cx >= x0 - mx && cx <= x1 + mx && cy >= y0 - my && cy <= y1 + my {
            all.union(b);
        }
    }
    all
}

/// COLORREF → (r, g, b)
pub fn rgb(c: u32) -> (u8, u8, u8) {
    (c as u8, (c >> 8) as u8, (c >> 16) as u8)
}

/// 线种的虚线图案：返回 [实, 虚, 实, 虚 …] 的格数（每格 = pitch）。实线返回 None
pub fn dash_cells(doc: &Doc, style: u8) -> Option<(Vec<u32>, u32, u32)> {
    let t = doc.ltypes.get(style as usize)?;
    if style <= 1 || t.pattern == 0 || t.unit_dot == 0 || (11..=15).contains(&style) {
        return None;
    }
    let n = t.unit_dot.min(32);
    let bits: Vec<bool> = (0..n).map(|i| t.pattern >> (31 - i) & 1 == 1).collect();
    if bits.iter().all(|&b| b) || bits.iter().all(|&b| !b) {
        return None;
    }
    // 旋转到"实"的开头，方便写成 实/虚 交替
    let first_on = (0..n as usize).find(|&i| bits[i] && !bits[(i + n as usize - 1) % n as usize]).unwrap_or(0);
    let mut runs = Vec::new();
    let mut cur = bits[first_on];
    let mut len = 0;
    for k in 0..n as usize {
        let b = bits[(first_on + k) % n as usize];
        if b == cur {
            len += 1;
        } else {
            runs.push(len);
            cur = b;
            len = 1;
        }
    }
    runs.push(len);
    if runs.len() % 2 == 1 {
        runs.push(0);
    }
    Some((runs, t.pitch.max(1), t.prt_pitch.max(1)))
}

