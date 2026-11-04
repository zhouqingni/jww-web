//! 字体：从系统里找日文字体，按字取轮廓。
//!
//! Jw_cad 的字会被拉伸成「横宽 × 高」的格子，普通的文字渲染做不到横竖分开缩放，
//! 所以这里直接取字形轮廓，画面上三角化后填色，PDF 里原样写成路径。

use lyon_tessellation::geom::point;
use lyon_tessellation::path::Path;
use lyon_tessellation::{BuffersBuilder, FillOptions, FillRule, FillTessellator, FillVertex, VertexBuffers};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// 候选字体文件（按优先顺序）。Mac 上没有 MS 字体时用冬青（ヒラギノ）
const GOTHIC: &[(&str, u32)] = &[
    ("C:/Windows/Fonts/msgothic.ttc", 0),
    ("/Library/Fonts/msgothic.ttc", 0),
    ("/Library/Fonts/MS Gothic.ttf", 0),
    ("/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc", 0),
    ("/System/Library/Fonts/ヒラギノ角ゴ ProN W3.otf", 0),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", 0),
    ("/Library/Fonts/Osaka.ttf", 0),
    // Linux（云端测试用）
    ("/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc", 0),
];
const MINCHO: &[(&str, u32)] = &[
    ("C:/Windows/Fonts/msmincho.ttc", 0),
    ("/Library/Fonts/msmincho.ttc", 0),
    ("/Library/Fonts/MS Mincho.ttf", 0),
    ("/System/Library/Fonts/ヒラギノ明朝 ProN.ttc", 0),
    ("/System/Library/Fonts/ヒラギノ明朝 ProN W3.otf", 0),
];

#[derive(Clone, Copy, Debug)]
pub enum Cmd {
    M(f32, f32),
    L(f32, f32),
    Q(f32, f32, f32, f32),
    C(f32, f32, f32, f32, f32, f32),
    Z,
}

/// 一个字形，坐标已经除以 em（1.0 = 一个全角格）
pub struct Glyph {
    pub adv: f32,
    pub cmds: Vec<Cmd>,
    pub verts: Vec<[f32; 2]>,
    pub idx: Vec<u32>,
}

struct Face {
    face: ttf_parser::Face<'static>,
    em: f32,
    /// 下降部分占整格高的比例（基线离格子底边的距离）
    desc: f32,
}

pub struct Fonts {
    faces: Vec<Face>,
    pub gothic: Option<usize>,
    pub mincho: Option<usize>,
    /// 给界面（egui）用的字体文件
    pub ui_font: Option<(Vec<u8>, u32)>,
    cache: RefCell<HashMap<(usize, char), Option<Rc<Glyph>>>>,
}

fn parse(data: Vec<u8>, idx: u32) -> Option<(Vec<u8>, u32, Face)> {
    let leaked: &'static [u8] = Box::leak(data.clone().into_boxed_slice());
    let face = ttf_parser::Face::parse(leaked, idx).ok()?;
    let em = face.units_per_em() as f32;
    let asc = face.ascender() as f32;
    let desc = -(face.descender() as f32);
    let f = Face { face, em, desc: desc / (asc + desc) };
    Some((data, idx, f))
}

fn load(list: &[(&str, u32)]) -> Option<(Vec<u8>, u32, Face)> {
    list.iter().find_map(|&(path, idx)| parse(std::fs::read(path).ok()?, idx))
}

struct Outline {
    cmds: Vec<Cmd>,
    s: f32,
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.cmds.push(Cmd::M(x * self.s, y * self.s));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.cmds.push(Cmd::L(x * self.s, y * self.s));
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let s = self.s;
        self.cmds.push(Cmd::Q(x1 * s, y1 * s, x * s, y * s));
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let s = self.s;
        self.cmds.push(Cmd::C(x1 * s, y1 * s, x2 * s, y2 * s, x * s, y * s));
    }
    fn close(&mut self) {
        self.cmds.push(Cmd::Z);
    }
}

fn tessellate(cmds: &[Cmd]) -> (Vec<[f32; 2]>, Vec<u32>) {
    let mut b = Path::builder();
    let mut open = false;
    for c in cmds {
        match *c {
            Cmd::M(x, y) => {
                if open {
                    b.end(true);
                }
                b.begin(point(x, y));
                open = true;
            }
            Cmd::L(x, y) => {
                b.line_to(point(x, y));
            }
            Cmd::Q(x1, y1, x, y) => {
                b.quadratic_bezier_to(point(x1, y1), point(x, y));
            }
            Cmd::C(x1, y1, x2, y2, x, y) => {
                b.cubic_bezier_to(point(x1, y1), point(x2, y2), point(x, y));
            }
            Cmd::Z => {
                if open {
                    b.end(true);
                    open = false;
                }
            }
        }
    }
    if open {
        b.end(true);
    }
    let path = b.build();
    let mut geom: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let opt = FillOptions::tolerance(0.004).with_fill_rule(FillRule::NonZero);
    let _ = FillTessellator::new().tessellate_path(
        &path,
        &opt,
        &mut BuffersBuilder::new(&mut geom, |v: FillVertex| v.position().to_array()),
    );
    (geom.vertices, geom.indices)
}

impl Fonts {
    pub fn system() -> Fonts {
        let mut faces = Vec::new();
        let mut ui_font = None;
        let mut gothic = None;
        let mut mincho = None;
        if let Some((data, idx, f)) = load(GOTHIC) {
            ui_font = Some((data, idx));
            gothic = Some(faces.len());
            faces.push(f);
        }
        if let Some((data, idx, f)) = load(MINCHO) {
            if ui_font.is_none() {
                ui_font = Some((data, idx));
            }
            mincho = Some(faces.len());
            faces.push(f);
        }
        Fonts { faces, gothic, mincho, ui_font, cache: RefCell::new(HashMap::new()) }
    }

    /// 网页版：用随程序带的字体（ゴシック・明朝都用它）
    pub fn embedded(data: Vec<u8>) -> Fonts {
        let mut faces = Vec::new();
        let mut ui_font = None;
        let mut gothic = None;
        if let Some((data, idx, f)) = parse(data, 0) {
            ui_font = Some((data, idx));
            gothic = Some(0);
            faces.push(f);
        }
        Fonts { faces, gothic, mincho: None, ui_font, cache: RefCell::new(HashMap::new()) }
    }

    /// Jw 的字体名 → 用哪个字体
    pub fn pick(&self, jw_name: &str) -> Option<usize> {
        if jw_name.contains("明朝") || jw_name.to_lowercase().contains("mincho") {
            self.mincho.or(self.gothic)
        } else {
            self.gothic.or(self.mincho)
        }
    }

    /// 基线离格子底边的比例
    pub fn desc(&self, face: usize) -> f32 {
        self.faces[face].desc
    }

    pub fn glyph(&self, face: usize, ch: char) -> Option<Rc<Glyph>> {
        if let Some(g) = self.cache.borrow().get(&(face, ch)) {
            return g.clone();
        }
        let g = self.build(face, ch).or_else(|| {
            // 这个字体里没有的字，去别的字体里找
            (0..self.faces.len()).filter(|&i| i != face).find_map(|i| self.build(i, ch))
        });
        let g = g.map(Rc::new);
        self.cache.borrow_mut().insert((face, ch), g.clone());
        g
    }

    fn build(&self, face: usize, ch: char) -> Option<Glyph> {
        let f = &self.faces[face];
        let id = f.face.glyph_index(ch)?;
        let s = 1.0 / f.em;
        let adv = f.face.glyph_hor_advance(id).unwrap_or(0) as f32 * s;
        let mut o = Outline { cmds: Vec::new(), s };
        f.face.outline_glyph(id, &mut o);
        let (verts, idx) = if o.cmds.is_empty() { (Vec::new(), Vec::new()) } else { tessellate(&o.cmds) };
        Some(Glyph { adv, cmds: o.cmds, verts, idx })
    }
}
