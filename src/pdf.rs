//! 输出 PDF（矢量）。线宽、线种、实点照 Jw_cad 的"打印"设定；
//! 字是字形轮廓，同一个字只写一次（Form XObject），反复引用。

use crate::draw::{dash_cells, layer_key, rgb, Col, Sink, Walker};
use crate::font::{Cmd, Glyph};
use crate::geom::Affine;
use crate::jww::{Attr, Doc, Point, Shape, P};
use std::collections::HashMap;
use std::fmt::Write as _;

pub struct PdfOptions {
    /// false = 全部黑色（Jw 的默认），true = 用打印色
    pub color: bool,
    /// 下标 = layer_key
    pub visible: Vec<bool>,
    /// 打印纸（mm，横 × 纵）。.jww 里不存打印机的纸，所以由使用者选
    pub page: (f64, f64),
    /// 打印框中心（图寸）
    pub center: P,
    /// 打印倍率
    pub scale: f64,
}

/// 常用打印纸（横向，mm）
pub const PAGES: &[(&str, f64, f64)] = &[
    ("A4", 297.0, 210.0),
    ("A3", 420.0, 297.0),
    ("A2", 594.0, 420.0),
    ("A1", 841.0, 594.0),
    ("A0", 1189.0, 841.0),
];

/// 打印框：纸面上哪一块印到纸上。Jw 存的是上次打印的原点（默认是框的中心）和倍率
pub struct Frame {
    /// 图寸 → 纸上 mm 的变换
    pub m: Affine,
    /// 印出来的 1mm 相当于图寸多少（线宽要除掉倍率）
    pub scale: f64,
}

pub fn file_scale(doc: &Doc) -> f64 {
    if doc.prt_scale > 0.0 { doc.prt_scale } else { 1.0 }
}

/// 文件里存的上次打印位置 → 打印框中心
pub fn file_center(doc: &Doc, page: (f64, f64)) -> P {
    let s = file_scale(doc);
    // 十位：原点在框的哪个位置（1 左下 … 5 中中 … 9 右上，0 = 中心）
    let k = doc.prt_set / 10 % 10;
    let (ax, ay) = if k == 0 { (0.5, 0.5) } else { (((k - 1) % 3) as f64 / 2.0, ((k - 1) / 3) as f64 / 2.0) };
    P {
        x: doc.prt_origin.x + (0.5 - ax) * page.0 / s,
        y: doc.prt_origin.y + (0.5 - ay) * page.1 / s,
    }
}

pub fn print_frame(center: P, scale: f64, page: (f64, f64)) -> Frame {
    let m = Affine::translate(page.0 / 2.0, page.1 / 2.0)
        .then(&Affine::scale(scale, scale))
        .then(&Affine::translate(-center.x, -center.y));
    Frame { m, scale }
}

/// 刚打开图时的打印框：纸（PAGES 的下标）、中心、倍率。
/// 先用文件里存的上次打印位置；那里什么都没有（旧数据）时，框住整张图
pub fn initial_frame(doc: &Doc, boxes: &[crate::geom::BBox]) -> (usize, P, f64) {
    let a3 = 1;
    let pg = |i: usize| (PAGES[i].1, PAGES[i].2);
    let scale = file_scale(doc);
    let center = file_center(doc, pg(a3));
    let fb = frame_box(center, scale, pg(a3));
    if boxes.iter().any(|b| b.hits(&fb)) {
        return (a3, center, scale);
    }
    let all = crate::draw::main_box(boxes);
    if all.is_empty() {
        return (a3, center, scale);
    }
    let (w, h) = (all.x1 - all.x0, all.y1 - all.y0);
    let page = PAGES.iter().position(|p| p.1 >= w && p.2 >= h).unwrap_or(PAGES.len() - 1);
    let (pw, ph) = pg(page);
    let c = P { x: (all.x0 + all.x1) / 2.0, y: (all.y0 + all.y1) / 2.0 };
    (page, c, (pw / w).min(ph / h).min(1.0))
}

/// 打印框在图寸上的范围
pub fn frame_box(center: P, scale: f64, page: (f64, f64)) -> crate::geom::BBox {
    let (hw, hh) = (page.0 / 2.0 / scale, page.1 / 2.0 / scale);
    crate::geom::BBox { x0: center.x - hw, y0: center.y - hh, x1: center.x + hw, y1: center.y + hh }
}

struct PdfSink<'a> {
    doc: &'a Doc,
    opt: &'a PdfOptions,
    out: String,
    forms: HashMap<usize, (usize, Vec<Cmd>)>,
    cur_color: Option<(u8, u8, u8)>,
    /// 线宽・虚线・实点要除以打印倍率（Jw 的线宽是印出来的粗细）
    wscale: f64,
}

fn num(v: f64) -> String {
    let s = format!("{:.4}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".into() } else { s.into() }
}

/// 打印线宽（mm）。Jw 的线宽单位：1/100mm 模式，或 打印 dpi 的点数
fn width_mm(doc: &Doc, a: &Attr) -> f64 {
    let pen = doc.pens.get(a.color as usize).copied().unwrap_or_default();
    let w = if a.width > 0 { a.width as u32 } else { pen.prt_width };
    let dot = 25.4 / doc.prt_dpi as f64;
    let mm = if doc.max_draw_wid < 0 { w as f64 / 100.0 } else { w as f64 * dot };
    mm.max(dot)
}

impl<'a> PdfSink<'a> {
    fn color_of(&self, c: Col) -> (u8, u8, u8) {
        if !self.opt.color {
            return (0, 0, 0);
        }
        match c {
            Col::Rgb(v) => rgb(v),
            Col::Pen(i) => rgb(self.doc.pens.get(i as usize).map(|p| p.prt_color).unwrap_or(0)),
        }
    }

    fn set_color(&mut self, c: (u8, u8, u8)) {
        if self.cur_color != Some(c) {
            let f = |v: u8| num(v as f64 / 255.0);
            let _ = writeln!(self.out, "{} {} {} RG {} {} {} rg", f(c.0), f(c.1), f(c.2), f(c.0), f(c.1), f(c.2));
            self.cur_color = Some(c);
        }
    }

    fn vis(&self, a: &Attr) -> bool {
        self.opt.visible.get(layer_key(a)).copied().unwrap_or(true)
    }
}

impl<'a> Sink for PdfSink<'a> {
    fn tol(&self) -> f64 {
        0.005
    }

    fn line(&mut self, pts: &[P], a: &Attr) {
        if pts.len() < 2 || !self.vis(a) {
            return;
        }
        self.set_color(self.color_of(Col::Pen(a.color)));
        let w = width_mm(self.doc, a) * self.wscale;
        let dash = match dash_cells(self.doc, a.style) {
            Some((runs, _, prt_pitch)) => {
                let unit = prt_pitch as f64 * 25.4 / self.doc.prt_dpi as f64 * self.wscale;
                let v: Vec<String> = runs.iter().map(|r| num(*r as f64 * unit)).collect();
                format!("[{}] 0 d", v.join(" "))
            }
            None => "[] 0 d".into(),
        };
        let _ = write!(self.out, "{} w {} {} {} m", num(w), dash, num(pts[0].x), num(pts[0].y));
        for p in &pts[1..] {
            let _ = write!(self.out, " {} {} l", num(p.x), num(p.y));
        }
        self.out.push_str(" S\n");
    }

    fn fill(&mut self, rings: &[Vec<P>], col: Col, a: &Attr) {
        if !self.vis(a) {
            return;
        }
        self.set_color(self.color_of(col));
        for r in rings {
            if r.len() < 3 {
                continue;
            }
            let _ = write!(self.out, "{} {} m", num(r[0].x), num(r[0].y));
            for p in &r[1..] {
                let _ = write!(self.out, " {} {} l", num(p.x), num(p.y));
            }
            self.out.push_str(" h ");
        }
        self.out.push_str("f*\n");
    }

    fn point(&mut self, p: P, a: &Attr, pt: &Point) {
        if pt.temp || !self.vis(a) {
            return; // 仮点不打印
        }
        self.set_color(self.color_of(Col::Pen(a.color)));
        let pen = self.doc.pens.get(a.color as usize).copied().unwrap_or_default();
        let r = if self.doc.draw_prt_point != 0 { pen.prt_point_r } else { width_mm(self.doc, a) / 2.0 } * self.wscale;
        // 用 4 段贝塞尔画圆
        let k = 0.5523 * r;
        let _ = writeln!(
            self.out,
            "{x1} {y} m {x1} {yk1} {xk1} {y1} {x} {y1} c {xk0} {y1} {x0} {yk1} {x0} {y} c {x0} {yk0} {xk0} {y0} {x} {y0} c {xk1} {y0} {x1} {yk0} {x1} {y} c f",
            x = num(p.x), y = num(p.y),
            x0 = num(p.x - r), x1 = num(p.x + r), y0 = num(p.y - r), y1 = num(p.y + r),
            xk0 = num(p.x - k), xk1 = num(p.x + k), yk0 = num(p.y - k), yk1 = num(p.y + k),
        );
    }

    fn glyph(&mut self, g: &Glyph, m: &Affine, a: &Attr) {
        if !self.vis(a) {
            return;
        }
        self.set_color(self.color_of(Col::Pen(a.color)));
        let key = g as *const Glyph as usize;
        let n = self.forms.len();
        let id = self.forms.entry(key).or_insert_with(|| (n, g.cmds.clone())).0;
        let _ = writeln!(
            self.out,
            "q {} {} {} {} {} {} cm /G{} Do Q",
            num(m.a), num(m.b), num(m.c), num(m.d), num(m.e), num(m.f), id
        );
    }
}

fn glyph_stream(cmds: &[Cmd]) -> String {
    let mut s = String::new();
    let (mut cx, mut cy) = (0f32, 0f32);
    let f = |v: f32| num(v as f64);
    for c in cmds {
        match *c {
            Cmd::M(x, y) => {
                let _ = write!(s, "{} {} m ", f(x), f(y));
                (cx, cy) = (x, y);
            }
            Cmd::L(x, y) => {
                let _ = write!(s, "{} {} l ", f(x), f(y));
                (cx, cy) = (x, y);
            }
            Cmd::Q(qx, qy, x, y) => {
                let (c1x, c1y) = (cx + 2.0 / 3.0 * (qx - cx), cy + 2.0 / 3.0 * (qy - cy));
                let (c2x, c2y) = (x + 2.0 / 3.0 * (qx - x), y + 2.0 / 3.0 * (qy - y));
                let _ = write!(s, "{} {} {} {} {} {} c ", f(c1x), f(c1y), f(c2x), f(c2y), f(x), f(y));
                (cx, cy) = (x, y);
            }
            Cmd::C(x1, y1, x2, y2, x, y) => {
                let _ = write!(s, "{} {} {} {} {} {} c ", f(x1), f(y1), f(x2), f(y2), f(x), f(y));
                (cx, cy) = (x, y);
            }
            Cmd::Z => s.push_str("h "),
        }
    }
    s.push('f');
    s
}

fn stream(body: &[u8]) -> Vec<u8> {
    let z = miniz_oxide::deflate::compress_to_vec_zlib(body, 6);
    let mut v = format!("<< /Length {} /Filter /FlateDecode >>\nstream\n", z.len()).into_bytes();
    v.extend_from_slice(&z);
    v.extend_from_slice(b"\nendstream");
    v
}

/// 把整张图按用纸大小输出成一页 PDF
pub fn render(w: &Walker, opt: &PdfOptions) -> Vec<u8> {
    render_counted(w, opt).0
}

/// 同 render，另外返回画了多少笔（线・面・点・字，测试用）
pub fn render_counted(w: &Walker, opt: &PdfOptions) -> (Vec<u8>, usize) {
    let doc = w.doc;
    let (pw, ph) = opt.page;
    let k = 72.0 / 25.4;
    let fr = print_frame(opt.center, opt.scale, opt.page);
    let mut s = PdfSink { doc, opt, out: String::new(), forms: HashMap::new(), cur_color: None, wscale: 1.0 / fr.scale };
    // 图寸 → 纸上 mm → pt
    let m = Affine::scale(k, k).then(&fr.m);
    let _ = writeln!(
        s.out,
        "{} {} {} {} {} {} cm 1 J 1 j",
        num(m.a), num(m.b), num(m.c), num(m.d), num(m.e), num(m.f)
    );
    // Jw「ソリッドを最初に描画」
    let solid_first = doc.bitmap_first % 10 != 0;
    let is_solid = |e: &crate::jww::Ent| matches!(e.shape, Shape::Solid { .. });
    if solid_first {
        for e in doc.ents.iter().filter(|e| is_solid(e)) {
            w.ent(e, &Affine::ID, &e.a, 0, &mut s);
        }
    }
    for e in doc.ents.iter().filter(|e| !solid_first || !is_solid(e)) {
        w.ent(e, &Affine::ID, &e.a, 0, &mut s);
    }

    let mut objs: Vec<Vec<u8>> = Vec::new();
    let mut forms: Vec<(usize, Vec<Cmd>)> = s.forms.into_values().collect();
    forms.sort_by_key(|f| f.0);
    let first_form = 5;
    let mut xo = String::new();
    for (i, _) in forms.iter().enumerate() {
        let _ = write!(xo, "/G{} {} 0 R ", i, first_form + i);
    }
    objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
    objs.push(b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec());
    objs.push(
        format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /XObject << {} >> >> /Contents 4 0 R >>",
            num(pw * k), num(ph * k), xo
        )
        .into_bytes(),
    );
    let drawn = s.out.lines().count().saturating_sub(1);
    objs.push(stream(s.out.as_bytes()));
    for (_, cmds) in &forms {
        let body = glyph_stream(cmds);
        let z = miniz_oxide::deflate::compress_to_vec_zlib(body.as_bytes(), 6);
        let mut v = format!(
            "<< /Type /XObject /Subtype /Form /BBox [-1 -1 2 2] /Length {} /Filter /FlateDecode >>\nstream\n",
            z.len()
        )
        .into_bytes();
        v.extend_from_slice(&z);
        v.extend_from_slice(b"\nendstream");
        objs.push(v);
    }

    let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{:010} 00000 n \n", o).as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n", objs.len() + 1, xref).as_bytes(),
    );
    (out, drawn)
}

/// 按文件里的图层状态决定可见（0 = 不显示）
pub fn file_visibility(doc: &Doc) -> Vec<bool> {
    let mut v = vec![true; 256];
    for (g, gl) in doc.glayers.iter().enumerate() {
        for l in 0..16 {
            v[g * 16 + l] = gl.state != 0 && gl.lay_state[l] != 0;
        }
    }
    v
}
