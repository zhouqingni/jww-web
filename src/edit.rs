//! 编辑命令（不依赖界面）。每个命令都能直接用坐标调用，测试按练习题一步步调用。
//!
//! 坐标是图寸（纸面 mm）。输入的长度是实寸，用 `real()`/`paper()` 按写入图层组的缩尺换算。

use crate::cad::*;
use crate::jww::{self, Attr, Doc, Ent, GLayer, Point, Shape, Text, P};
use std::f64::consts::{PI, TAU};

/// 空白图纸的底子：Jw_cad 样例图的设定部分（去掉图形、名字、说明）
const TEMPLATE: &[u8] = include_bytes!("../tests/fixtures/site.jww");

#[derive(Clone)]
struct Snap {
    ents: Vec<Ent>,
    write_glay: u32,
    glayers: Vec<GLayer>,
    paper: u32,
}

/// 写入属性（线色・线种）
#[derive(Clone, Copy, Debug)]
pub struct Pen {
    pub color: u16,
    pub style: u8,
}

pub struct Editor {
    pub doc: Doc,
    pub pen: Pen,
    /// 选中的图形（ents 的下标）
    pub sel: Vec<usize>,
    pub dirty: bool,
    /// 改一次加一（界面用来判断要不要重算显示）
    pub rev: u64,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    /// 文字：文字种（1〜10，0 = 任意）、任意时的大小・间隔，角度（度），字体
    pub moji_kind: u32,
    pub moji_any: (f64, f64, f64),
    pub moji_deg: f64,
    pub font: String,
    /// 环境设定（.jwf）的图层别线色・线种：换写入图层时线属性跟着换（0 = 不换）
    pub lay_pens: Option<[[(u16, u8); 16]; 16]>,
    /// 点图形时的限定（Jw_cad：Shift = 只选书込レイヤ，Ctrl = 只选书込线色・线种）
    pub pick_only: (bool, bool),
    /// 属性取得的「レイヤ反転表示」：看不见的层可以选，看得见的层不能选
    pub invert: bool,
    /// 寸法：小数桁（0〜3）、端部（0 = 点、1 = 矢印、2 = 逆矢印）、引出角（度）
    pub dim_dec: u32,
    pub dim_end: u8,
    pub dim_lead_ang: f64,
}

/// 公司图纸的「寸法設定」1〜5（从她的图纸读出来的）：
/// 寸法線色 2・引出線色 2・矢印点色 3・文字種 2、寸法図形にする
pub const COMPANY_DIM_SET: [u32; 5] = [2000322, 5, 50150050, 12000000, 14100];

/// 角的位置：-1 = 左/下，0 = 中，1 = 右/上
pub type Anchor = (i32, i32);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PolyMode {
    /// 中心 → 顶点
    Vertex,
    /// 中心 → 边
    Side,
    /// 边长
    Edge,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DimDir {
    Horizontal,
    Vertical,
    /// 任意角度（度）
    Angle(f64),
}

impl Editor {
    pub fn new(doc: Doc) -> Self {
        Editor {
            doc,
            pen: Pen { color: 2, style: 1 },
            sel: Vec::new(),
            dirty: false,
            rev: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            moji_kind: 3,
            moji_any: (3.0, 3.0, 0.5),
            moji_deg: 0.0,
            font: "ＭＳ ゴシック".into(),
            lay_pens: None,
            pick_only: (false, false),
            invert: false,
            dim_dec: 2,
            dim_end: 0,
            dim_lead_ang: 0.0,
        }
    }

    /// 新图纸：A3、各图层组 1/100、0 组 0 层写入
    pub fn blank() -> Self {
        let mut d = jww::read_bytes(TEMPLATE).expect("模板图读不出来");
        d.memo.clear();
        d.ents.clear();
        d.blocks.clear();
        d.images.clear();
        d.paper = 3;
        d.write_glay = 0;
        for (i, g) in d.glayers.iter_mut().enumerate() {
            g.state = if i == 0 { 3 } else { 2 };
            g.write_lay = 0;
            g.scale = 100.0;
            g.protect = 0;
            g.name.clear();
            for k in 0..16 {
                g.lay_state[k] = if k == 0 { 3 } else { 2 };
                g.lay_protect[k] = 0;
                g.lay_name[k].clear();
            }
        }
        // 寸法設定は会社の図面と同じに（寸法図形・寸法の色）
        let at = d.dim_set_at;
        for (k, v) in COMPANY_DIM_SET.iter().enumerate() {
            d.settings_raw[at + k * 4..at + k * 4 + 4].copy_from_slice(&v.to_le_bytes());
        }
        Editor::new(d)
    }

    /// 図面の「寸法設定」1〜5
    pub fn dim_set(&self) -> [u32; 5] {
        let at = self.doc.dim_set_at;
        let raw = &self.doc.settings_raw;
        std::array::from_fn(|k| raw.get(at + k * 4..at + k * 4 + 4).map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]])))
    }

    /// 「寸法線と値を寸法図形にする」
    pub fn dim_fig(&self) -> bool {
        self.dim_set()[4] / 10000 % 10 == 1
    }

    pub fn set_dim_fig(&mut self, on: bool) {
        let mut v = self.dim_set()[4];
        let cur = v / 10000 % 10;
        v = v - cur * 10000 + if on { 10000 } else { 0 };
        let at = self.doc.dim_set_at + 16;
        if let Some(b) = self.doc.settings_raw.get_mut(at..at + 4) {
            b.copy_from_slice(&v.to_le_bytes());
        }
    }

    /// 寸法の色：（寸法線色, 引出線色, 矢印・点色）。0 のときは書込線色
    pub fn dim_colors(&self) -> (u16, u16, u16) {
        let v = self.dim_set()[0];
        let c = |d: u32| match d % 10 {
            0 => self.pen.color,
            x => x as u16,
        };
        (c(v), c(v / 10), c(v / 100))
    }

    // ---------- 撤销・重做 ----------

    fn snap(&self) -> Snap {
        Snap {
            ents: self.doc.ents.clone(),
            write_glay: self.doc.write_glay,
            glayers: self.doc.glayers.clone(),
            paper: self.doc.paper,
        }
    }
    fn restore(&mut self, s: Snap) {
        self.doc.ents = s.ents;
        self.doc.write_glay = s.write_glay;
        self.doc.glayers = s.glayers;
        self.doc.paper = s.paper;
        self.sel.clear();
    }
    /// 改图之前调用一次（一个命令 = 一步撤销）
    pub fn begin(&mut self) {
        let s = self.snap();
        self.undo.push(s);
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.dirty = true;
        self.rev += 1;
    }
    pub fn undo(&mut self) -> bool {
        match self.undo.pop() {
            Some(s) => {
                let cur = self.snap();
                self.redo.push(cur);
                self.restore(s);
                self.dirty = true;
                self.rev += 1;
                true
            }
            None => false,
        }
    }
    pub fn redo(&mut self) -> bool {
        match self.redo.pop() {
            Some(s) => {
                let cur = self.snap();
                self.undo.push(cur);
                self.restore(s);
                self.dirty = true;
                self.rev += 1;
                true
            }
            None => false,
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    // ---------- 写入图层・缩尺 ----------

    pub fn glay(&self) -> usize {
        self.doc.write_glay as usize & 15
    }
    pub fn lay(&self) -> usize {
        self.doc.glayers[self.glay()].write_lay as usize & 15
    }
    pub fn scale(&self) -> f64 {
        let s = self.doc.glayers[self.glay()].scale;
        if s > 0.0 {
            s
        } else {
            1.0
        }
    }
    /// 实寸 → 图寸
    pub fn paper(&self, real: f64) -> f64 {
        real / self.scale()
    }
    /// 图寸 → 实寸
    pub fn real(&self, paper: f64) -> f64 {
        paper * self.scale()
    }

    /// 设写入图层组・图层（原来的写入层变成"可编辑"）
    pub fn set_write(&mut self, g: usize, l: usize) {
        let og = self.glay();
        let ol = self.lay();
        if self.doc.glayers[og].lay_state[ol] == 3 {
            self.doc.glayers[og].lay_state[ol] = 2;
        }
        if self.doc.glayers[og].state == 3 {
            self.doc.glayers[og].state = 2;
        }
        self.doc.write_glay = g as u32;
        let gl = &mut self.doc.glayers[g];
        gl.write_lay = l as u32;
        gl.state = 3;
        gl.lay_state[l] = 3;
        if let Some((c, s)) = self.lay_pens.map(|t| t[g & 15][l & 15]) {
            if c != 0 {
                self.pen.color = c;
            }
            if s != 0 {
                self.pen.style = s;
            }
        }
    }

    pub fn attr(&self) -> Attr {
        Attr {
            group: 0,
            style: self.pen.style,
            color: self.pen.color,
            width: 0,
            layer: self.lay() as u16,
            glayer: self.glay() as u16,
            flag: 0,
        }
    }

    /// 加一个图形（用当前写入属性），返回下标
    pub fn push(&mut self, shape: Shape) -> usize {
        let a = self.attr();
        self.push_attr(a, shape)
    }
    pub fn push_attr(&mut self, a: Attr, shape: Shape) -> usize {
        self.doc.ents.push(Ent { a, shape });
        self.doc.ents.len() - 1
    }

    pub fn curve(&self, i: usize) -> Option<Curve> {
        self.doc.ents.get(i).and_then(Curve::of)
    }

    /// 能编辑的图层（不显示・只显示・保护的不能选）
    pub fn editable(&self, a: &Attr) -> bool {
        let g = &self.doc.glayers[a.glayer as usize & 15];
        if self.invert {
            // 反転表示：原来不显示・只显示的层可以选
            return g.state < 2 || g.lay_state[a.layer as usize & 15] < 2;
        }
        g.state >= 2 && g.protect == 0 && g.lay_state[a.layer as usize & 15] >= 2 && g.lay_protect[a.layer as usize & 15] == 0
    }

    /// Shift・Ctrl 的限定
    fn pick_ok(&self, e: &Ent) -> bool {
        let (shift, ctrl) = self.pick_only;
        if shift && (e.a.glayer as usize != self.glay() || e.a.layer as usize != self.lay()) {
            return false;
        }
        if ctrl {
            let style_ok = matches!(e.shape, Shape::Point(_)) || e.a.style == self.pen.style;
            if e.a.color != self.pen.color || !style_ok {
                return false;
            }
        }
        true
    }

    // ---------- 拾取 ----------

    /// 离 q 最近的图形（tol 以内）。filter 决定要哪种
    pub fn pick(&self, q: P, tol: f64, filter: impl Fn(&Ent) -> bool) -> Option<usize> {
        let mut best = None;
        let mut bd = tol;
        for (i, e) in self.doc.ents.iter().enumerate() {
            if !self.editable(&e.a) || !self.pick_ok(e) || !filter(e) {
                continue;
            }
            let d = ent_dist(e, q);
            if d <= bd {
                bd = d;
                best = Some(i);
            }
        }
        best
    }

    /// 右键「读取点」：端点・交点・点・圆心以外的特征点。tol 以内最近的
    pub fn read_point(&self, q: P, tol: f64) -> Option<P> {
        let mut best: Option<P> = None;
        let mut bd = tol;
        let mut try_pt = |c: P, best: &mut Option<P>| {
            let d = dist(c, q);
            if d <= bd {
                bd = d;
                *best = Some(c);
            }
        };
        let mut near: Vec<Curve> = Vec::new();
        for e in &self.doc.ents {
            match &e.shape {
                Shape::Line { a, b } => {
                    try_pt(*a, &mut best);
                    try_pt(*b, &mut best);
                }
                Shape::Arc(a) if !a.full => {
                    if let Some(Curve::Circ { c, r, a0, sweep, .. }) = Curve::of(e) {
                        try_pt(polar(c, r, a0), &mut best);
                        try_pt(polar(c, r, a0 + sweep), &mut best);
                    }
                }
                Shape::Point(pt) => try_pt(pt.p, &mut best),
                Shape::Text(t) => try_pt(t.start, &mut best),
                Shape::Block { base, .. } => try_pt(*base, &mut best),
                _ => {}
            }
            if let Some(c) = Curve::of(e) {
                if c.dist(q) <= tol {
                    near.push(c);
                }
            }
        }
        // 交点（只看光标附近的线）
        for i in 0..near.len() {
            for j in i + 1..near.len() {
                for x in near[i].meet(&near[j], false) {
                    try_pt(x, &mut best);
                }
            }
        }
        best
    }

    // ---------- B 作图 ----------

    pub fn line(&mut self, a: P, b: P) -> usize {
        self.push(Shape::Line { a, b })
    }

    /// 矩形：两个对角点
    pub fn rect(&mut self, a: P, b: P) -> Vec<usize> {
        let c = [a, p(b.x, a.y), b, p(a.x, b.y)];
        (0..4).map(|k| self.line(c[k], c[(k + 1) % 4])).collect()
    }

    /// 矩形：实寸 w × h，基准点 base 在矩形的 anchor 位置（(1,1) = 右上角）
    pub fn rect_size(&mut self, base: P, w: f64, h: f64, anchor: Anchor) -> Vec<usize> {
        let (a, b) = self.rect_corners(base, w, h, anchor);
        self.rect(a, b)
    }
    pub fn rect_corners(&self, base: P, w: f64, h: f64, anchor: Anchor) -> (P, P) {
        let (w, h) = (self.paper(w), self.paper(h));
        let x0 = base.x - w * (anchor.0 + 1) as f64 / 2.0;
        let y0 = base.y - h * (anchor.1 + 1) as f64 / 2.0;
        (p(x0, y0), p(x0 + w, y0 + h))
    }

    /// 圆：圆心、实寸半径
    pub fn circle(&mut self, c: P, r_real: f64) -> usize {
        let r = self.paper(r_real);
        self.push(arc_shape(c, r, 0.0, TAU))
    }
    pub fn circle3(&mut self, a: P, b: P, c: P) -> Option<usize> {
        let (o, r) = circle3(a, b, c)?;
        Some(self.push(arc_shape(o, r, 0.0, TAU)))
    }
    pub fn arc(&mut self, c: P, r: f64, a0: f64, sweep: f64) -> usize {
        self.push(arc_shape(c, r, a0, sweep))
    }

    pub fn point(&mut self, q: P) -> usize {
        let mut a = self.attr();
        a.style = 1;
        self.push_attr(a, Shape::Point(Point { p: q, temp: false, code: 0, angle: 0.0, scale: 1.0 }))
    }

    /// 正多角形：实寸 size，`ang` 第一个顶点的方向（度）
    pub fn polygon(&mut self, c: P, n: usize, size: f64, mode: PolyMode, ang: f64) -> Vec<usize> {
        let v = polygon_pts(c, n, self.paper(size), mode, ang);
        (0..n).map(|k| self.line(v[k], v[(k + 1) % n])).collect()
    }

    // ---------- C 编辑 ----------

    /// 複線：图形 i 往 side 那一侧偏 d（实寸）。线和圆都行
    pub fn offset(&mut self, i: usize, d_real: f64, side: P) -> Option<usize> {
        let e = self.doc.ents.get(i)?.clone();
        let d = self.paper(d_real);
        let shape = offset_shape(&e, d, side)?;
        Some(self.push_attr(e.a, shape))
    }

    /// 複線（两侧）
    pub fn offset_both(&mut self, i: usize, d_real: f64) -> Vec<usize> {
        let mut v = Vec::new();
        let Some(e) = self.doc.ents.get(i).cloned() else { return v };
        let d = self.paper(d_real);
        for s in [-1.0, 1.0] {
            if let Some(sh) = offset_signed(&e, d * s) {
                v.push(self.push_attr(e.a.clone(), sh));
            }
        }
        v
    }

    /// 2線：线 i 的两侧各偏 d1, d2（实寸），用当前写入属性
    pub fn two_lines(&mut self, i: usize, d1: f64, d2: f64) -> Vec<usize> {
        let Some(e) = self.doc.ents.get(i).cloned() else { return vec![] };
        let mut v = Vec::new();
        for (d, s) in [(d1, 1.0), (d2, -1.0)] {
            if let Some(sh) = offset_signed(&e, self.paper(d) * s) {
                v.push(self.push(sh));
            }
        }
        v
    }

    /// 中心線：两条线（平行 = 中间线，相交 = 角平分线）。返回无限线上两点
    pub fn center_of_lines(&self, i: usize, j: usize, pi: P, pj: P) -> Option<(P, P)> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (self.curve(i), self.curve(j)) else { return None };
        match line_line(a1, b1, a2, b2) {
            None => {
                let f = project(a2, b2, a1);
                let m = mid(a1, f);
                Some((m, add(m, sub(b1, a1))))
            }
            Some(x) => {
                // 角平分线：取点击那一侧的方向
                let u1 = side_dir(a1, b1, x, pi);
                let u2 = side_dir(a2, b2, x, pj);
                let bis = norm(add(u1, u2));
                Some((x, add(x, bis)))
            }
        }
    }
    /// 中心線：两点的垂直平分线
    pub fn center_of_points(a: P, b: P) -> (P, P) {
        let m = mid(a, b);
        (m, add(m, perp(sub(b, a))))
    }
    /// 中心線画出来：无限线 (a, b) 上，从 s 的投影到 t 的投影
    pub fn center_line(&mut self, l: (P, P), s: P, t: P) -> usize {
        let (a, b) = (project(l.0, l.1, s), project(l.0, l.1, t));
        self.line(a, b)
    }

    /// 分割：两点之间 n 等分，打点
    pub fn divide_points(&mut self, a: P, b: P, n: usize) -> Vec<usize> {
        (1..n).map(|k| self.point(lerp(a, b, k as f64 / n as f64))).collect()
    }
    /// 分割：两条线之间 n 等分，画线
    pub fn divide_lines(&mut self, i: usize, j: usize, n: usize) -> Vec<usize> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(mut a2, mut b2))) = (self.curve(i), self.curve(j)) else {
            return vec![];
        };
        if dot(sub(b1, a1), sub(b2, a2)) < 0.0 {
            std::mem::swap(&mut a2, &mut b2);
        }
        (1..n)
            .map(|k| {
                let t = k as f64 / n as f64;
                self.line(lerp(a1, a2, t), lerp(b1, b2, t))
            })
            .collect()
    }
    /// 分割：线 i 本身 n 等分打点
    pub fn divide_line(&mut self, i: usize, n: usize) -> Vec<usize> {
        match self.curve(i) {
            Some(Curve::Seg(a, b)) => self.divide_points(a, b, n),
            Some(Curve::Circ { c, r, a0, sweep, full }) => {
                let m = if full { n } else { n - 1 };
                (1..=m)
                    .map(|k| {
                        let t = if full { k - 1 } else { k };
                        self.point(polar(c, r, a0 + sweep * t as f64 / n as f64))
                    })
                    .collect()
            }
            None => vec![],
        }
    }

    /// 线 i 改成端点 a, b
    fn set_seg(&mut self, i: usize, a: P, b: P) {
        self.doc.ents[i].shape = Shape::Line { a, b };
    }

    /// 线 i 剪/伸到 x，留下 pick 那一侧
    fn cut_to(&mut self, i: usize, pick: P, x: P) -> bool {
        let Some(Curve::Seg(a, b)) = self.curve(i) else { return false };
        let tx = param(a, b, x);
        let tp = param(a, b, pick);
        if tp < tx {
            self.set_seg(i, a, x)
        } else {
            self.set_seg(i, x, b)
        }
        true
    }

    /// コーナー：两条线伸缩到交点，各留下点击那一侧
    pub fn corner(&mut self, i: usize, pi: P, j: usize, pj: P) -> bool {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (self.curve(i), self.curve(j)) else { return false };
        let Some(x) = line_line(a1, b1, a2, b2) else { return false };
        self.cut_to(i, pi, x);
        self.cut_to(j, pj, x);
        true
    }

    /// 伸縮：线 i 伸缩到基准线 j（j 当作无限长），留下点击 pick 那一侧
    pub fn stretch(&mut self, i: usize, pick: P, j: usize) -> bool {
        let (Some(c), Some(t)) = (self.curve(i), self.curve(j)) else { return false };
        let Curve::Seg(..) = c else { return false };
        let xs = c.meet(&t, true);
        let Some(x) = xs.into_iter().min_by(|u, v| dist(*u, pick).total_cmp(&dist(*v, pick))) else { return false };
        self.cut_to(i, pick, x)
    }

    /// 伸縮：线 i 伸缩到点 q 的投影（在线外 = 把近的端点伸过去；在线内 = 剪掉，留 pick 那侧）
    pub fn stretch_to_point(&mut self, i: usize, pick: P, q: P) -> bool {
        let Some(Curve::Seg(a, b)) = self.curve(i) else { return false };
        let x = project(a, b, q);
        let t = param(a, b, q);
        if t < 0.0 {
            self.set_seg(i, x, b);
        } else if t > 1.0 {
            self.set_seg(i, a, x);
        } else {
            self.cut_to(i, pick, x);
        }
        true
    }

    /// 伸縮「一括処理」：和线段 s0-s1 相交的线全部伸缩到基准线 j，留下 s0-s1 穿过的那侧
    pub fn stretch_batch(&mut self, j: usize, s0: P, s1: P) -> usize {
        let Some(base) = self.curve(j) else { return 0 };
        let sel = Curve::Seg(s0, s1);
        let mut n = 0;
        for i in 0..self.doc.ents.len() {
            if i == j || !self.editable(&self.doc.ents[i].a) {
                continue;
            }
            let Some(c @ Curve::Seg(..)) = self.curve(i) else { continue };
            let Some(hit) = c.meet(&sel, false).into_iter().next() else { continue };
            if let Some(x) = c.meet(&base, true).into_iter().min_by(|u, v| dist(*u, hit).total_cmp(&dist(*v, hit))) {
                self.cut_to(i, hit, x);
                n += 1;
            }
        }
        n
    }

    /// 面取：两条线的角。r 实寸。round = 丸面（圆弧），否则角面（斜线，两边各 r）
    pub fn chamfer(&mut self, i: usize, pi: P, j: usize, pj: P, r_real: f64, round: bool) -> Option<usize> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (self.curve(i), self.curve(j)) else { return None };
        let x = line_line(a1, b1, a2, b2)?;
        let r = self.paper(r_real);
        let u1 = side_dir(a1, b1, x, pi);
        let u2 = side_dir(a2, b2, x, pj);
        let th = dot(u1, u2).clamp(-1.0, 1.0).acos() / 2.0;
        if th < 1e-6 || th > PI / 2.0 - 1e-6 {
            return None;
        }
        let (t1, t2) = if round {
            let k = r / th.tan();
            (add(x, mul(u1, k)), add(x, mul(u2, k)))
        } else {
            (add(x, mul(u1, r)), add(x, mul(u2, r)))
        };
        self.cut_to(i, pi, t1);
        self.cut_to(j, pj, t2);
        let a = self.doc.ents[i].a.clone();
        if round {
            let c = add(x, mul(norm(add(u1, u2)), r / th.sin()));
            let (s1, s2) = (angle_of(sub(t1, c)), angle_of(sub(t2, c)));
            // 短的那段弧
            let mut sw = norm_ang(s2 - s1);
            let mut st = s1;
            if sw > PI {
                st = s2;
                sw = TAU - sw;
            }
            Some(self.push_attr(a, arc_shape(c, r, st, sw)))
        } else {
            Some(self.push_attr(a, Shape::Line { a: t1, b: t2 }))
        }
    }

    /// 接線：两个圆的公切线，取切点离点击处 h1, h2 最近的那条
    pub fn tangent_cc(&mut self, i: usize, h1: P, j: usize, h2: P) -> Option<usize> {
        let (Some(Curve::Circ { c: c1, r: r1, .. }), Some(Curve::Circ { c: c2, r: r2, .. })) = (self.curve(i), self.curve(j))
        else {
            return None;
        };
        let (a, b) = tangents_cc(c1, r1, c2, r2)
            .into_iter()
            .min_by(|u, v| (dist(u.0, h1) + dist(u.1, h2)).total_cmp(&(dist(v.0, h1) + dist(v.1, h2))))?;
        Some(self.line(a, b))
    }

    /// 接線：从点 q 到圆 i 的切线，取切点离 h 近的
    pub fn tangent_pc(&mut self, q: P, i: usize, h: P) -> Option<usize> {
        let Some(Curve::Circ { c, r, .. }) = self.curve(i) else { return None };
        let d = dist(q, c);
        if d <= r {
            return None;
        }
        let base = angle_of(sub(q, c));
        let al = (r / d).acos();
        let t = [polar(c, r, base + al), polar(c, r, base - al)];
        let t = if dist(t[0], h) < dist(t[1], h) { t[0] } else { t[1] };
        Some(self.line(q, t))
    }

    /// 接円：和两条线相切、实寸半径 r，圆心在 hint 附近的那个
    pub fn tangent_circle_ll(&mut self, i: usize, j: usize, r_real: f64, hint: P) -> Option<usize> {
        let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (self.curve(i), self.curve(j)) else { return None };
        let r = self.paper(r_real);
        let n1 = mul(perp(norm(sub(b1, a1))), r);
        let n2 = mul(perp(norm(sub(b2, a2))), r);
        let mut best: Option<P> = None;
        for s1 in [-1.0, 1.0] {
            for s2 in [-1.0, 1.0] {
                let (o1, o2) = (mul(n1, s1), mul(n2, s2));
                if let Some(c) = line_line(add(a1, o1), add(b1, o1), add(a2, o2), add(b2, o2)) {
                    if best.map(|b| dist(c, hint) < dist(b, hint)).unwrap_or(true) {
                        best = Some(c);
                    }
                }
            }
        }
        let c = best?;
        Some(self.push(arc_shape(c, r, 0.0, TAU)))
    }

    // ---------- 消去 ----------

    pub fn erase(&mut self, mut idx: Vec<usize>) {
        idx.sort_unstable();
        idx.dedup();
        for i in idx.into_iter().rev() {
            if i < self.doc.ents.len() {
                self.doc.ents.remove(i);
            }
        }
        self.sel.clear();
    }

    /// 部分消し：图形 i 上 s 到 t 之间消掉（圆是从 s 逆时针到 t）
    pub fn erase_part(&mut self, i: usize, s: P, t: P) -> bool {
        let Some(c) = self.curve(i) else { return false };
        let a = self.doc.ents[i].a.clone();
        match c {
            Curve::Seg(a0, b0) => {
                let (mut u, mut v) = (param(a0, b0, s).clamp(0.0, 1.0), param(a0, b0, t).clamp(0.0, 1.0));
                if u > v {
                    std::mem::swap(&mut u, &mut v);
                }
                let mut parts = Vec::new();
                if u > 1e-9 {
                    parts.push(Shape::Line { a: a0, b: lerp(a0, b0, u) });
                }
                if v < 1.0 - 1e-9 {
                    parts.push(Shape::Line { a: lerp(a0, b0, v), b: b0 });
                }
                self.replace(i, a, parts);
            }
            Curve::Circ { c, r, a0, sweep, full } => {
                let (ps, pt) = (norm_ang(angle_of(sub(s, c))), norm_ang(angle_of(sub(t, c))));
                if full {
                    // 剩下 t → s 的弧
                    self.replace(i, a, vec![arc_shape(c, r, pt, norm_ang(ps - pt))]);
                } else {
                    let (mut u, mut v) = (norm_ang(ps - a0).min(sweep), norm_ang(pt - a0).min(sweep));
                    if u > v {
                        std::mem::swap(&mut u, &mut v);
                    }
                    let mut parts = Vec::new();
                    if u > 1e-9 {
                        parts.push(arc_shape(c, r, a0, u));
                    }
                    if v < sweep - 1e-9 {
                        parts.push(arc_shape(c, r, a0 + v, sweep - v));
                    }
                    self.replace(i, a, parts);
                }
            }
        }
        true
    }

    /// 節間消し：消掉点击处前后两个交点之间的那一节（没有交点就到端点）
    pub fn erase_between(&mut self, i: usize, pick: P) -> bool {
        let Some(c) = self.curve(i) else { return false };
        let mut xs: Vec<P> = Vec::new();
        for (k, e) in self.doc.ents.iter().enumerate() {
            if k == i {
                continue;
            }
            if let Some(o) = Curve::of(e) {
                xs.extend(c.meet(&o, false));
            }
        }
        match c {
            Curve::Seg(a, b) => {
                let tp = param(a, b, pick);
                let mut lo: f64 = 0.0;
                let mut hi: f64 = 1.0;
                for x in xs {
                    let t = param(a, b, x);
                    if t < tp - 1e-9 && t > lo {
                        lo = t;
                    }
                    if t > tp + 1e-9 && t < hi {
                        hi = t;
                    }
                }
                self.erase_part(i, lerp(a, b, lo), lerp(a, b, hi))
            }
            Curve::Circ { c: o, a0, sweep, full, .. } => {
                let ap = norm_ang(angle_of(sub(pick, o)) - a0);
                let mut lo: f64 = if full { f64::NAN } else { 0.0 };
                let mut hi: f64 = if full { f64::NAN } else { sweep };
                for x in &xs {
                    let t = norm_ang(angle_of(sub(*x, o)) - a0);
                    if full {
                        // 整圆：找点击处前后最近的交点
                        let back = norm_ang(ap - t);
                        let fwd = norm_ang(t - ap);
                        if lo.is_nan() || back < norm_ang(ap - lo) {
                            lo = t;
                        }
                        if hi.is_nan() || fwd < norm_ang(hi - ap) {
                            hi = t;
                        }
                    } else {
                        if t < ap && t > lo {
                            lo = t;
                        }
                        if t > ap && t < hi {
                            hi = t;
                        }
                    }
                }
                if full && (xs.len() < 2) {
                    return false;
                }
                let Curve::Circ { c: cc, r, .. } = c else { unreachable!() };
                self.erase_part(i, polar(cc, r, a0 + lo), polar(cc, r, a0 + hi))
            }
        }
    }

    /// 把图形 i 换成 parts（可以是空 = 删掉）
    pub(crate) fn replace(&mut self, i: usize, a: Attr, parts: Vec<Shape>) {
        let mut it = parts.into_iter();
        match it.next() {
            Some(s) => self.doc.ents[i].shape = s,
            None => {
                self.doc.ents.remove(i);
                return;
            }
        }
        for s in it {
            self.push_attr(a.clone(), s);
        }
    }

    // ---------- パラメ・曲線・寸法（角度・弧長）・寸解 ----------

    /// パラメトリック変形：框 a-b 里的端点移动 d。整个在框里的图形整个移动，
    /// 线只有一端在框里就只动那一端（线跟着伸缩）。返回动了几个
    pub fn parametric(&mut self, a: P, b: P, d: P) -> usize {
        let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        let inside = |q: P| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1;
        let mut n = 0;
        for i in 0..self.doc.ents.len() {
            if !self.editable(&self.doc.ents[i].a) {
                continue;
            }
            let e = &mut self.doc.ents[i];
            if key_points(e).iter().all(|q| inside(*q)) {
                translate(&mut e.shape, d);
                n += 1;
            } else if let Shape::Line { a: s, b: t } = &mut e.shape {
                let (si, ti) = (inside(*s), inside(*t));
                if si {
                    *s = add(*s, d);
                }
                if ti {
                    *t = add(*t, d);
                }
                if si || ti {
                    n += 1;
                }
            }
        }
        n
    }

    /// 曲線：通过各点的样条（Catmull-Rom），每段分 div 份画成线，带同一个曲線属性号
    pub fn spline(&mut self, pts: &[P], div: usize) -> Vec<usize> {
        if pts.len() < 2 {
            return vec![];
        }
        let curve = spline_pts(pts, div.max(1));
        self.polyline_curve(&curve)
    }

    /// 折れ線を一本の曲線（同じ曲線属性号の線）として作図
    pub fn polyline_curve(&mut self, pts: &[P]) -> Vec<usize> {
        // 寸法のグループ（u32::MAX から下へ）は除いて、次の番号
        let group = self.doc.ents.iter().map(|e| e.a.group).filter(|&g| g < 0x8000_0000).max().unwrap_or(0) + 1;
        let mut a = self.attr();
        a.group = group;
        pts.windows(2).filter(|w| dist(w[0], w[1]) > 1e-12).map(|w| self.push_attr(a.clone(), Shape::Line { a: w[0], b: w[1] })).collect()
    }

    /// 角度寸法：中心 c，从 p1 方向逆时针量到 p2 方向，寸法線（圆弧）通过 at
    pub fn dim_angle(&mut self, c: P, p1: P, p2: P, at: P) -> Vec<usize> {
        let r = dist(c, at);
        if r < 1e-9 {
            return vec![];
        }
        let a0 = angle_of(sub(p1, c));
        let mut sw = norm_ang(angle_of(sub(p2, c)) - a0);
        let mut start = a0;
        // at 不在 p1→p2 逆时针那一侧：量另一边（p2→p1）
        if norm_ang(angle_of(sub(at, c)) - a0) > sw + 1e-12 {
            start = angle_of(sub(p2, c));
            sw = std::f64::consts::TAU - sw;
        }
        let s = format!("{}°", fmt_num(deg(sw), 2));
        self.dim_arc(c, r, start, sw, &s)
    }

    /// 弧長寸法（円周）：圆弧 i 的弧长，寸法線是通过 at 的同心圆弧
    pub fn dim_arclen(&mut self, i: usize, at: P) -> Vec<usize> {
        let Some(Curve::Circ { c, r, a0, sweep, .. }) = self.curve(i) else { return vec![] };
        let real = self.real(r * sweep);
        let s = format!("⌒{}", self.fmt_dim(real));
        self.dim_arc(c, dist(c, at).max(1e-6), a0, sweep, &s)
    }

    /// 圆弧形的寸法線 + 两端的端点 + 中间的文字
    pub(crate) fn dim_arc(&mut self, c: P, r: f64, start: f64, sweep: f64, s: &str) -> Vec<usize> {
        let a = self.attr();
        let mut la = a.clone();
        la.flag = 0x2000;
        let mut out = vec![self.push_attr(la, arc_shape(c, r, start, sweep))];
        // 端部：外向き = 弧の外へ向かう接線方向
        for (t, sgn) in [(start, -1.0), (start + sweep, 1.0)] {
            let out_dir = mul(perp(p(t.cos(), t.sin())), sgn);
            out.extend(self.dim_end_mark(polar(c, r, t), out_dir, &a));
        }
        // 文字：弧中点的切线方向
        let m = start + sweep / 2.0;
        let pm = polar(c, r, m);
        let tang = perp(p(m.cos(), m.sin()));
        out.push(self.dim_text(sub(pm, tang), add(pm, tang), s, &a));
        out
    }

    /// 寸解：寸法图形（寸法線＋寸法值）拆成普通的线和文字
    pub fn explode_dims(&mut self, idx: &[usize]) -> usize {
        let mut n = 0;
        let mut idx = idx.to_vec();
        idx.sort_unstable();
        for &i in idx.iter().rev() {
            let Some(Shape::Dim(d)) = self.doc.ents.get(i).map(|e| e.shape.clone()) else { continue };
            let mut parts = vec![*d.line, *d.text];
            if let Some(x) = d.sxf {
                parts.extend(x.lines.into_iter().map(|b| *b));
                parts.extend(x.points.into_iter().map(|b| *b));
            }
            self.doc.ents.remove(i);
            for (k, mut e) in parts.into_iter().enumerate() {
                e.a.flag = 0;
                self.doc.ents.insert(i + k, e);
            }
            n += 1;
        }
        self.sel.clear();
        n
    }

    // ---------- 図形（.jws）----------

    /// 図形を q に置く（図形の基準点を q に合わせる）。実寸を保つように
    /// 図形側の縮尺 / 書込グループの縮尺 で拡大縮小し、さらに mag 倍・ang 度回す。
    /// 書込レイヤに作図。ブロックは今は置かない。返回（放了几个, 跳过几个）
    pub fn place_figure(&mut self, fig: &jww::Jws, q: P, mag: f64, ang_deg: f64) -> (usize, usize) {
        let (g, l, ws) = (self.glay() as u16, self.lay() as u16, self.scale());
        let (mut n, mut skip) = (0, 0);
        for e in &fig.ents {
            if matches!(e.shape, Shape::Block { .. }) {
                skip += 1;
                continue;
            }
            let k = fig.scales[e.a.glayer as usize & 15] / ws * mag;
            let mut e = e.clone();
            transform(&mut e, fig.base, q, k, rad(ang_deg));
            e.a.glayer = g;
            e.a.layer = l;
            self.doc.ents.push(e);
            n += 1;
        }
        (n, skip)
    }

    /// 図登：选中的图形存成 .jws（基准点 base，图寸）。ブロックは入れない
    pub fn figure_bytes(&self, idx: &[usize], base: P) -> Vec<u8> {
        let tpl = jww::read_jws_bytes(crate::jwwsave::JWS_TEMPLATE).expect("模板 .jws 读不出来");
        let ents: Vec<Ent> = idx
            .iter()
            .filter_map(|&i| self.doc.ents.get(i))
            .filter(|e| !matches!(e.shape, Shape::Block { .. }))
            .cloned()
            .collect();
        let mut scales = [1.0; 16];
        for (g, s) in scales.iter_mut().enumerate() {
            if let Some(gl) = self.doc.glayers.get(g) {
                *s = gl.scale;
            }
        }
        crate::jwwsave::write_jws(&tpl, &ents, base, &scales)
    }

    // ---------- D 属性・移动 ----------

    /// 属取：图形 i 的属性变成当前写入属性（图层也切过去）
    pub fn take_attr(&mut self, i: usize) {
        let Some(e) = self.doc.ents.get(i) else { return };
        let a = e.a.clone();
        if !matches!(e.shape, Shape::Text(_) | Shape::Point(_)) {
            self.pen = Pen { color: a.color, style: a.style };
        }
        self.set_write(a.glayer as usize & 15, a.layer as usize & 15);
    }

    /// 属変：把选中的图形改成当前写入属性（线色・线种・图层）
    pub fn change_attr(&mut self, idx: &[usize], color: bool, style: bool, layer: bool) {
        let (g, l, pen) = (self.glay() as u16, self.lay() as u16, self.pen);
        for &i in idx {
            let Some(e) = self.doc.ents.get_mut(i) else { continue };
            let is_line = matches!(e.shape, Shape::Line { .. } | Shape::Arc(_));
            if color && !matches!(e.shape, Shape::Text(_)) {
                e.a.color = pen.color;
            }
            if style && is_line {
                e.a.style = pen.style;
            }
            if layer {
                e.a.glayer = g;
                e.a.layer = l;
            }
        }
    }

    /// 複写：返回新图形的下标
    pub fn copy(&mut self, idx: &[usize], d: P) -> Vec<usize> {
        let mut v = Vec::new();
        for &i in idx {
            if let Some(e) = self.doc.ents.get(i).cloned() {
                let mut e = e;
                translate(&mut e.shape, d);
                self.doc.ents.push(e);
                v.push(self.doc.ents.len() - 1);
            }
        }
        v
    }
    pub fn move_(&mut self, idx: &[usize], d: P) {
        for &i in idx {
            if let Some(e) = self.doc.ents.get_mut(i) {
                translate(&mut e.shape, d);
            }
        }
    }

    /// 范围选择：完全在框里的图形（可编辑的）
    pub fn select_window(&self, a: P, b: P) -> Vec<usize> {
        let (x0, x1, y0, y1) = (a.x.min(b.x), a.x.max(b.x), a.y.min(b.y), a.y.max(b.y));
        let inside = |q: P| q.x >= x0 && q.x <= x1 && q.y >= y0 && q.y <= y1;
        self.doc
            .ents
            .iter()
            .enumerate()
            .filter(|(_, e)| self.editable(&e.a) && key_points(e).iter().all(|q| inside(*q)))
            .map(|(i, _)| i)
            .collect()
    }

    // ---------- E 文字・寸法 ----------

    fn moji_size(&self) -> (f64, f64, f64, u16) {
        let k = self.moji_kind as usize;
        if (1..=10).contains(&k) {
            let m = self.doc.moji[k];
            (m.w, m.h, m.gap, m.color as u16)
        } else {
            (self.moji_any.0, self.moji_any.1, self.moji_any.2, self.pen.color)
        }
    }

    /// 文字：q 是左下角
    pub fn text(&mut self, q: P, s: &str) -> usize {
        let (w, h, gap, color) = self.moji_size();
        let t = make_text(q, s, w, h, gap, self.moji_deg, self.moji_kind, &self.font);
        let mut a = self.attr();
        a.color = color;
        a.style = 1;
        self.push_attr(a, Shape::Text(t))
    }

    /// 改字：内容换掉，起点・大小不变，终点重算
    pub fn set_text(&mut self, i: usize, s: &str) -> bool {
        let Some(Shape::Text(t)) = self.doc.ents.get(i).map(|e| e.shape.clone()) else { return false };
        let n = make_text(t.start, s, t.size_x, t.size_y, t.gap, t.deg, t.kind, &t.font);
        if let Shape::Text(old) = &mut self.doc.ents[i].shape {
            old.s = n.s;
            old.end = n.end;
        }
        true
    }

    /// 寸法：p1, p2 之间，寸法线通过 at。返回加的图形
    pub fn dim(&mut self, p1: P, p2: P, dir: DimDir, at: P) -> Vec<usize> {
        let ang = match dir {
            DimDir::Horizontal => 0.0,
            DimDir::Vertical => PI / 2.0,
            DimDir::Angle(d) => rad(d),
        };
        let u = p(ang.cos(), ang.sin());
        let n = perp(u);
        // 寸法线所在的位置（沿法线方向）
        let off = dot(at, n);
        let q1 = add(mul(u, dot(p1, u)), mul(n, off));
        let q2 = add(mul(u, dot(p2, u)), mul(n, off));
        let real = self.real(dist(q1, q2));
        let mut out = Vec::new();
        let a = self.attr();
        let mut la = a.clone();
        la.flag = 0x2000;
        // 引出線：从测点到寸法线再超出 1mm
        for (pp, qq) in [(p1, q1), (p2, q2)] {
            let v = sub(qq, pp);
            if len(v) > 1e-6 {
                let e = add(qq, mul(norm(v), 1.0));
                out.push(self.push_attr(la.clone(), Shape::Line { a: pp, b: e }));
            }
        }
        out.push(self.push_attr(la.clone(), Shape::Line { a: q1, b: q2 }));
        let mut pa = a.clone();
        pa.style = 1;
        pa.flag = 0x40;
        for q in [q1, q2] {
            out.push(self.push_attr(pa.clone(), Shape::Point(Point { p: q, temp: false, code: 0, angle: 0.0, scale: 1.0 })));
        }
        let txt = self.fmt_dim(real);
        out.push(self.dim_text(q1, q2, &txt, &a));
        out
    }

    /// 寸法值文字：寸法线中央、上方 0.5mm，文字种 2
    pub(crate) fn dim_text(&mut self, q1: P, q2: P, s: &str, a: &Attr) -> usize {
        let (mut q1, mut q2) = (q1, q2);
        // 文字永远朝上・朝左读
        let v = sub(q2, q1);
        if v.x < -1e-9 || (v.x.abs() <= 1e-9 && v.y < 0.0) {
            std::mem::swap(&mut q1, &mut q2);
        }
        let ang = angle_of(sub(q2, q1));
        let m = self.doc.moji[2];
        let mut t = make_text(p(0.0, 0.0), s, m.w, m.h, m.gap, deg(ang), 2, &self.font);
        let w = dist(t.start, t.end);
        let u = norm(sub(q2, q1));
        let st = add(sub(mid(q1, q2), mul(u, w / 2.0)), mul(perp(u), 0.5));
        t.start = st;
        t.end = add(st, mul(u, w));
        let mut ta = a.clone();
        // Jw_cad の寸法値：線種の欄に文字種（2）
        ta.style = 2;
        ta.color = m.color as u16;
        ta.flag = 0x4010;
        self.push_attr(ta, Shape::Text(t))
    }

    /// 連続寸法：pts 依次量过去；double = 再加一行总长（二重寸法），间隔 gap（图寸）
    pub fn dim_chain(&mut self, pts: &[P], dir: DimDir, at: P, double: Option<f64>) -> Vec<usize> {
        let mut out = Vec::new();
        for w in pts.windows(2) {
            out.extend(self.dim(w[0], w[1], dir, at));
        }
        if let (Some(g), Some(f), Some(l)) = (double, pts.first(), pts.last()) {
            if pts.len() > 2 {
                let ang = match dir {
                    DimDir::Horizontal => 0.0,
                    DimDir::Vertical => PI / 2.0,
                    DimDir::Angle(d) => rad(d),
                };
                let n = perp(p(ang.cos(), ang.sin()));
                // 往远离图形那一侧再隔 g
                let s = if dot(sub(at, *f), n) >= 0.0 { 1.0 } else { -1.0 };
                out.extend(self.dim(*f, *l, dir, add(at, mul(n, g * s))));
            }
        }
        out
    }

    /// 半径・直径寸法：圆 i，寸法线朝 toward 方向
    pub fn dim_radius(&mut self, i: usize, toward: P, diameter: bool) -> Vec<usize> {
        let Some(Curve::Circ { c, r, .. }) = self.curve(i) else { return vec![] };
        let u = norm(sub(toward, c));
        let q2 = add(c, mul(u, r));
        let q1 = if diameter { sub(c, mul(u, r)) } else { c };
        let real = self.real(if diameter { 2.0 * r } else { r });
        let s = if diameter { format!("φ{}", self.fmt_dim(real)) } else { format!("R{}", self.fmt_dim(real)) };
        let a = self.attr();
        let mut la = a.clone();
        la.flag = 0x2000;
        let mut out = vec![self.push_attr(la, Shape::Line { a: q1, b: q2 })];
        out.push(self.dim_text(q1, q2, &s, &a));
        out
    }
}

// ---------- 小工具 ----------

/// 从交点 x 出发、指向点击 pick 那一侧的单位向量（沿线 a-b）
pub(crate) fn side_dir(a: P, b: P, x: P, pick: P) -> P {
    let d = norm(sub(b, a));
    if dot(sub(pick, x), d) >= 0.0 {
        d
    } else {
        mul(d, -1.0)
    }
}

/// 数字：小数 dec 位，末尾的 0 去掉
pub(crate) fn fmt_num(v: f64, dec: usize) -> String {
    let s = format!("{v:.dec$}");
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

/// Catmull-Rom 样条：通过全部点，每段 div 份
pub fn spline_pts(pts: &[P], div: usize) -> Vec<P> {
    let n = pts.len();
    let at = |i: isize| pts[i.clamp(0, n as isize - 1) as usize];
    let mut out = vec![pts[0]];
    for i in 0..n as isize - 1 {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        for k in 1..=div {
            let t = k as f64 / div as f64;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f64, b: f64, c: f64, d: f64| {
                0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            out.push(p(f(p0.x, p1.x, p2.x, p3.x), f(p0.y, p1.y, p2.y, p3.y)));
        }
    }
    out
}

pub fn polygon_pts(c: P, n: usize, size: f64, mode: PolyMode, ang: f64) -> Vec<P> {
    let half = PI / n as f64;
    let r = match mode {
        PolyMode::Vertex => size,
        PolyMode::Side => size / half.cos(),
        PolyMode::Edge => size / (2.0 * half.sin()),
    };
    (0..n).map(|k| polar(c, r, rad(ang) + TAU * k as f64 / n as f64)).collect()
}

/// 偏移 d（正 = 线的左侧 / 圆的外侧）
pub fn offset_signed(e: &Ent, d: f64) -> Option<Shape> {
    match Curve::of(e)? {
        Curve::Seg(a, b) => {
            let n = mul(perp(norm(sub(b, a))), d);
            Some(Shape::Line { a: add(a, n), b: add(b, n) })
        }
        Curve::Circ { c, r, a0, sweep, .. } => {
            let r2 = r + d;
            if r2 <= 1e-9 {
                return None;
            }
            Some(arc_shape(c, r2, a0, sweep))
        }
    }
}

/// 往 side 点所在的一侧偏移 d
pub fn offset_shape(e: &Ent, d: f64, side: P) -> Option<Shape> {
    let s = match Curve::of(e)? {
        Curve::Seg(a, b) => {
            if cross(sub(b, a), sub(side, a)) >= 0.0 {
                1.0
            } else {
                -1.0
            }
        }
        Curve::Circ { c, r, .. } => {
            if dist(side, c) >= r {
                1.0
            } else {
                -1.0
            }
        }
    };
    offset_signed(e, d * s)
}

/// 两圆的公切线（外 2 条 + 内 2 条），返回切点对
pub fn tangents_cc(c1: P, r1: f64, c2: P, r2: f64) -> Vec<(P, P)> {
    let d = dist(c1, c2);
    let mut out = Vec::new();
    if d < EPS {
        return out;
    }
    let base = angle_of(sub(c2, c1));
    for sgn in [1.0, -1.0] {
        // sgn = 1 外切线，-1 内切线
        let k = (r1 - sgn * r2) / d;
        if k.abs() > 1.0 {
            continue;
        }
        let al = k.acos();
        for s in [1.0, -1.0] {
            let a = base + s * al;
            let t1 = polar(c1, r1, a);
            let t2 = polar(c2, sgn * r2, a);
            out.push((t1, t2));
        }
    }
    out
}

/// 文字：起点、内容、字宽・字高・间隔、角度（度）
pub fn make_text(start: P, s: &str, w: f64, h: f64, gap: f64, deg_: f64, kind: u32, font: &str) -> Text {
    let n = s.chars().count();
    let width: f64 = s.chars().map(|c| if jww::is_half(c) { w / 2.0 } else { w }).sum::<f64>()
        + gap * (n.saturating_sub(1)) as f64;
    let a = rad(deg_);
    Text {
        start,
        end: polar(start, width, a),
        kind,
        italic: false,
        bold: false,
        size_x: w,
        size_y: h,
        gap,
        deg: deg_,
        font: font.into(),
        s: s.into(),
    }
}

/// 寸法值：千位逗号，小数最多 2 位、去掉多余的 0
pub fn fmt_len(v: f64) -> String {
    let v = (v * 100.0).round() / 100.0;
    let neg = v < 0.0;
    let v = v.abs();
    let int = v.trunc() as u64;
    let frac = ((v - v.trunc()) * 100.0).round() as u64;
    let s = int.to_string();
    let mut o = String::new();
    for (k, ch) in s.chars().enumerate() {
        if k > 0 && (s.len() - k) % 3 == 0 {
            o.push(',');
        }
        o.push(ch);
    }
    if frac > 0 {
        let f = format!("{frac:02}");
        o.push('.');
        o.push_str(f.trim_end_matches('0'));
    }
    if neg {
        o.insert(0, '-');
    }
    o
}

pub fn translate(s: &mut Shape, d: P) {
    match s {
        Shape::Line { a, b } => {
            *a = add(*a, d);
            *b = add(*b, d);
        }
        Shape::Arc(a) => a.c = add(a.c, d),
        Shape::Point(pt) => pt.p = add(pt.p, d),
        Shape::Text(t) => {
            t.start = add(t.start, d);
            t.end = add(t.end, d);
        }
        Shape::Dim(dm) => {
            translate(&mut dm.line.shape, d);
            translate(&mut dm.text.shape, d);
            if let Some(x) = &mut dm.sxf {
                for l in &mut x.lines {
                    translate(&mut l.shape, d);
                }
                for q in &mut x.points {
                    translate(&mut q.shape, d);
                }
            }
        }
        Shape::Solid { pts, .. } => {
            for q in pts {
                *q = add(*q, d);
            }
        }
        Shape::Block { base, .. } => *base = add(*base, d),
    }
}

/// 相似变换：以 o 为原点放大 k 倍、转 ang（弧度），再把 o 移到 q。
/// 文字大小不变（照 Jw_cad 図形），只改位置和角度
pub fn transform(e: &mut Ent, o: P, q: P, k: f64, ang: f64) {
    let f = |x: P| add(q, rot(mul(sub(x, o), k), ang));
    let style = e.a.style;
    match &mut e.shape {
        Shape::Line { a, b } => {
            *a = f(*a);
            *b = f(*b);
        }
        Shape::Arc(a) => {
            a.c = f(a.c);
            a.r *= k;
            a.tilt += ang;
        }
        Shape::Point(pt) => {
            pt.p = f(pt.p);
            pt.angle += deg(ang);
        }
        Shape::Text(t) => {
            let v = rot(sub(t.end, t.start), ang);
            t.start = f(t.start);
            t.end = add(t.start, v);
            t.deg += deg(ang);
        }
        Shape::Dim(dm) => {
            transform(&mut dm.line, o, q, k, ang);
            transform(&mut dm.text, o, q, k, ang);
            if let Some(x) = &mut dm.sxf {
                for l in x.lines.iter_mut() {
                    transform(l, o, q, k, ang);
                }
                for p_ in x.points.iter_mut() {
                    transform(p_, o, q, k, ang);
                }
            }
        }
        Shape::Solid { pts, .. } => {
            if style >= 101 {
                // 圆实体：pts[0] 中心、pts[1].x 半径、pts[2].x 倾斜角
                pts[0] = f(pts[0]);
                pts[1].x *= k;
                pts[2].x += ang;
            } else {
                for x in pts.iter_mut() {
                    *x = f(*x);
                }
            }
        }
        Shape::Block { base, sx, sy, rot: r, .. } => {
            *base = f(*base);
            *sx *= k;
            *sy *= k;
            *r += ang;
        }
    }
}

/// 图形的代表点（范围选择用：全部在框里才算选中）
pub fn key_points(e: &Ent) -> Vec<P> {
    match &e.shape {
        Shape::Line { a, b } => vec![*a, *b],
        Shape::Arc(_) => match Curve::of(e) {
            Some(Curve::Circ { c, r, a0, sweep, full }) => {
                if full {
                    vec![p(c.x - r, c.y - r), p(c.x + r, c.y + r)]
                } else {
                    (0..=8).map(|k| polar(c, r, a0 + sweep * k as f64 / 8.0)).collect()
                }
            }
            _ => match &e.shape {
                Shape::Arc(a) => {
                    let m = a.r * a.flat.abs().max(1.0);
                    vec![p(a.c.x - m, a.c.y - m), p(a.c.x + m, a.c.y + m)]
                }
                _ => unreachable!(),
            },
        },
        Shape::Point(pt) => vec![pt.p],
        Shape::Text(t) => {
            let up = mul(perp(norm(sub(t.end, t.start))), t.size_y);
            vec![t.start, t.end, add(t.start, up), add(t.end, up)]
        }
        Shape::Dim(d) => {
            let mut v = key_points(&d.line);
            v.extend(key_points(&d.text));
            v
        }
        Shape::Solid { pts, .. } => pts.to_vec(),
        Shape::Block { base, .. } => vec![*base],
    }
}

/// 点到图形的距离（拾取用）
pub fn ent_dist(e: &Ent, q: P) -> f64 {
    if let Some(c) = Curve::of(e) {
        return c.dist(q);
    }
    match &e.shape {
        Shape::Point(pt) => dist(pt.p, q),
        Shape::Text(t) => {
            let k = key_points(e);
            // 文字框里 = 0，否则到四边的距离
            let u = sub(t.end, t.start);
            let along = param(t.start, t.end, q);
            let up = perp(norm(u));
            let h = dot(sub(q, t.start), up);
            if (0.0..=1.0).contains(&along) && (0.0..=t.size_y).contains(&h) {
                0.0
            } else {
                let sides = [(k[0], k[1]), (k[1], k[3]), (k[3], k[2]), (k[2], k[0])];
                sides.iter().map(|(a, b)| dist_seg(*a, *b, q)).fold(f64::MAX, f64::min)
            }
        }
        Shape::Dim(d) => ent_dist(&d.line, q).min(ent_dist(&d.text, q)),
        Shape::Solid { pts, .. } => {
            let o = pts;
            (0..4).map(|k| dist_seg(o[k], o[(k + 1) % 4], q)).fold(f64::MAX, f64::min)
        }
        Shape::Block { base, .. } => dist(*base, q),
        Shape::Arc(a) => (dist(a.c, q) - a.r).abs(),
        Shape::Line { .. } => unreachable!(),
    }
}
