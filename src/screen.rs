//! 画面显示用的 Sink：线宽・虚线按像素（和 Jw_cad 的画面一样，不随缩放变化）

use crate::draw::{dash_cells, layer_key, rgb, Col, Sink};
use eframe::egui;
use crate::font::Glyph;
use crate::geom::{Affine, BBox};
use crate::jww::{Attr, Doc, Point, P};
use eframe::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{Color32, Pos2, Rect};
use lyon_tessellation::geom::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};

/// 画面坐标 ↔ 图寸
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// 画面中心对应的图寸坐标
    pub center: P,
    /// 每 mm 多少像素
    pub zoom: f64,
    pub rect: Rect,
}

impl View {
    pub fn to_screen(&self, p: P) -> Pos2 {
        let c = self.rect.center();
        Pos2::new(
            c.x + ((p.x - self.center.x) * self.zoom) as f32,
            c.y - ((p.y - self.center.y) * self.zoom) as f32,
        )
    }
    pub fn to_paper(&self, s: Pos2) -> P {
        let c = self.rect.center();
        P {
            x: self.center.x + (s.x - c.x) as f64 / self.zoom,
            y: self.center.y - (s.y - c.y) as f64 / self.zoom,
        }
    }
    pub fn paper_rect(&self) -> BBox {
        let a = self.to_paper(self.rect.left_top());
        let b = self.to_paper(self.rect.right_bottom());
        BBox { x0: a.x.min(b.x), y0: a.y.min(b.y), x1: a.x.max(b.x), y1: a.y.max(b.y) }
    }
    /// 让范围 bb 刚好放进画面
    pub fn fit(&mut self, bb: &BBox) {
        if bb.is_empty() {
            return;
        }
        let w = (bb.x1 - bb.x0).max(1.0);
        let h = (bb.y1 - bb.y0).max(1.0);
        self.center = P { x: (bb.x0 + bb.x1) / 2.0, y: (bb.y0 + bb.y1) / 2.0 };
        self.zoom = ((self.rect.width() as f64 / w).min(self.rect.height() as f64 / h)) * 0.95;
    }
}

pub struct ScreenSink<'a> {
    pub doc: &'a Doc,
    pub view: View,
    pub visible: &'a [bool],
    pub fills: Mesh,
    pub lines: Mesh,
    pub text: Mesh,
}

fn color32(c: u32) -> Color32 {
    let (r, g, b) = rgb(c);
    Color32::from_rgb(r, g, b)
}

impl<'a> ScreenSink<'a> {
    pub fn new(doc: &'a Doc, view: View, visible: &'a [bool]) -> Self {
        ScreenSink { doc, view, visible, fills: Mesh::default(), lines: Mesh::default(), text: Mesh::default() }
    }

    fn pen(&self, i: u16) -> Color32 {
        color32(self.doc.pens.get(i as usize).map(|p| p.color).unwrap_or(0xFFFFFF))
    }

    fn vis(&self, a: &Attr) -> bool {
        self.visible.get(layer_key(a)).copied().unwrap_or(true)
    }

    fn seg(&mut self, a: Pos2, b: Pos2, w: f32, c: Color32) {
        let d = b - a;
        let len = d.length();
        if len < 1e-3 {
            return;
        }
        let n = egui::vec2(-d.y, d.x) / len * (w / 2.0);
        // 两端各延长半个线宽，折线拐角不缺口
        let t = d / len * (w / 2.0).min(1.0) * 0.5;
        let i = self.lines.vertices.len() as u32;
        for p in [a - t + n, a - t - n, b + t - n, b + t + n] {
            self.lines.vertices.push(Vertex { pos: p, uv: WHITE_UV, color: c });
        }
        self.lines.indices.extend_from_slice(&[i, i + 1, i + 2, i, i + 2, i + 3]);
    }
}

impl<'a> Sink for ScreenSink<'a> {
    fn tol(&self) -> f64 {
        0.3 / self.view.zoom
    }

    fn wants(&self, bb: &BBox) -> bool {
        self.view.paper_rect().hits(bb)
    }

    fn line(&mut self, pts: &[P], a: &Attr) {
        if !self.vis(a) || pts.len() < 2 {
            return;
        }
        let c = self.pen(a.color);
        let w = self.doc.pens.get(a.color as usize).map(|p| p.width).unwrap_or(1).clamp(1, 16) as f32;
        let sp: Vec<Pos2> = pts.iter().map(|p| self.view.to_screen(*p)).collect();
        match dash_cells(self.doc, a.style) {
            None => {
                for k in 1..sp.len() {
                    self.seg(sp[k - 1], sp[k], w, c);
                }
            }
            Some((runs, pitch, _)) => {
                let runs: Vec<f32> = runs.iter().map(|r| (*r * pitch) as f32).collect();
                let period: f32 = runs.iter().sum();
                if period <= 0.0 {
                    return;
                }
                // 沿折线连续走虚线图案
                let mut phase = 0.0f32;
                for k in 1..sp.len() {
                    let (a0, b0) = (sp[k - 1], sp[k]);
                    let len = (b0 - a0).length();
                    if len < 1e-3 {
                        continue;
                    }
                    let dir = (b0 - a0) / len;
                    let mut t = 0.0;
                    while t < len {
                        let mut acc = 0.0;
                        let mut idx = 0;
                        let ph = phase % period;
                        while acc + runs[idx] <= ph {
                            acc += runs[idx];
                            idx = (idx + 1) % runs.len();
                        }
                        let remain = acc + runs[idx] - ph;
                        let step = remain.min(len - t);
                        if idx % 2 == 0 {
                            self.seg(a0 + dir * t, a0 + dir * (t + step), w, c);
                        }
                        t += step;
                        phase += step;
                    }
                }
            }
        }
    }

    fn fill(&mut self, rings: &[Vec<P>], col: Col, a: &Attr) {
        if !self.vis(a) {
            return;
        }
        let c = match col {
            Col::Rgb(v) => color32(v),
            Col::Pen(i) => self.pen(i),
        };
        let mut b = Path::builder();
        for r in rings {
            if r.len() < 3 {
                continue;
            }
            let s = self.view.to_screen(r[0]);
            b.begin(point(s.x, s.y));
            for p in &r[1..] {
                let s = self.view.to_screen(*p);
                b.line_to(point(s.x, s.y));
            }
            b.end(true);
        }
        let mut geom: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
        let _ = FillTessellator::new().tessellate_path(
            &b.build(),
            &FillOptions::tolerance(0.5).with_fill_rule(FillRule::EvenOdd),
            &mut BuffersBuilder::new(&mut geom, |v: FillVertex| v.position().to_array()),
        );
        let base = self.fills.vertices.len() as u32;
        for v in geom.vertices {
            self.fills.vertices.push(Vertex { pos: Pos2::new(v[0], v[1]), uv: WHITE_UV, color: c });
        }
        self.fills.indices.extend(geom.indices.iter().map(|i| i + base));
    }

    fn point(&mut self, p: P, a: &Attr, _pt: &Point) {
        if !self.vis(a) {
            return;
        }
        let s = self.view.to_screen(p);
        let c = self.pen(a.color);
        self.seg(s - egui::vec2(1.0, 0.0), s + egui::vec2(1.0, 0.0), 2.0, c);
    }

    fn glyph(&mut self, g: &Glyph, m: &Affine, a: &Attr) {
        if !self.vis(a) {
            return;
        }
        // 太小的字画不出来，跳过（省时间）
        let h = m.c.hypot(m.d) * self.view.zoom;
        if h < 1.5 {
            return;
        }
        let c = self.pen(a.color);
        let base = self.text.vertices.len() as u32;
        for v in &g.verts {
            let p = m.apply(P { x: v[0] as f64, y: v[1] as f64 });
            self.text.vertices.push(Vertex { pos: self.view.to_screen(p), uv: WHITE_UV, color: c });
        }
        self.text.indices.extend(g.idx.iter().map(|i| i + base));
    }
}
