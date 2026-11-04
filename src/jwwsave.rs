//! .jww 写出。和 `jww.rs` 的读取一一对应，读 → 写 必须字节完全一致（tests/roundtrip.rs）。
//!
//! MFC CArchive 的规则：
//! - 某个类第一次出现：0xFFFF、schema、类名长度、类名；类和紧跟的对象各占一个号（从 1 起）
//! - 以后再出现：0x8000 | 类号（类号 ≥ 0x7FFF 时写 0x7FFF + 0x80000000 | 类号）
//! - 每写一个对象号码加一
//! - 个数：< 0xFFFF 写 u16，否则 0xFFFF + u32

use crate::jww::*;
use std::collections::HashMap;

struct Wr {
    b: Vec<u8>,
    ver: u32,
    /// 已登记的类 → 类号
    classes: HashMap<&'static str, u32>,
    /// 下一个号
    next: u32,
    schemas: HashMap<String, u16>,
}

impl Wr {
    fn u8(&mut self, v: u8) {
        self.b.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    fn pt(&mut self, p: P) {
        self.f64(p.x);
        self.f64(p.y);
    }

    /// CString（ANSI = Shift-JIS）
    fn cstr(&mut self, s: &str) {
        let raw = encoding_rs::SHIFT_JIS.encode(s).0;
        let n = raw.len();
        if n < 0xFF {
            self.u8(n as u8);
        } else if n < 0xFFFE {
            self.u8(0xFF);
            self.u16(n as u16);
        } else {
            self.u8(0xFF);
            self.u16(0xFFFF);
            self.u32(n as u32);
        }
        self.b.extend_from_slice(&raw);
    }

    fn count(&mut self, n: usize) {
        if n < 0xFFFF {
            self.u16(n as u16);
        } else {
            self.u16(0xFFFF);
            self.u32(n as u32);
        }
    }

    fn klass(&mut self, nm: &'static str) {
        if let Some(&idx) = self.classes.get(nm) {
            if idx < 0x7FFF {
                self.u16(0x8000 | idx as u16);
            } else {
                self.u16(0x7FFF);
                self.u32(0x8000_0000 | idx);
            }
            self.next += 1;
        } else {
            let schema = self.schemas.get(nm).copied().unwrap_or(self.ver as u16);
            self.u16(0xFFFF);
            self.u16(schema);
            self.u16(nm.len() as u16);
            self.b.extend_from_slice(nm.as_bytes());
            self.classes.insert(nm, self.next);
            self.next += 2;
        }
    }

    fn attr(&mut self, a: &Attr) {
        self.u32(a.group);
        self.u8(a.style);
        self.u16(a.color);
        if self.ver >= 351 {
            self.u16(a.width);
        }
        self.u16(a.layer);
        self.u16(a.glayer);
        self.u16(a.flag);
    }

    /// 图形本体（不带类标记）
    fn body(&mut self, e: &Ent) {
        self.attr(&e.a);
        match &e.shape {
            Shape::Line { a, b } => {
                self.pt(*a);
                self.pt(*b);
            }
            Shape::Point(p) => {
                self.pt(p.p);
                self.u32(p.temp as u32);
                if e.a.style == 100 {
                    self.u32(p.code);
                    self.f64(p.angle);
                    self.f64(p.scale);
                }
            }
            Shape::Arc(c) => {
                self.pt(c.c);
                self.f64(c.r);
                self.f64(c.start);
                self.f64(c.sweep);
                self.f64(c.tilt);
                self.f64(c.flat);
                self.u32(c.full as u32);
            }
            Shape::Text(t) => {
                self.pt(t.start);
                self.pt(t.end);
                let k = t.kind + 10000 * (t.italic as u32 + 2 * t.bold as u32);
                self.u32(k);
                self.f64(t.size_x);
                self.f64(t.size_y);
                self.f64(t.gap);
                self.f64(t.deg);
                self.cstr(&t.font);
                self.cstr(&t.s);
            }
            Shape::Dim(d) => {
                self.body(&d.line);
                self.body(&d.text);
                if self.ver >= 420 {
                    // Ver.4.20 以后必有 SXF 部分；读入时没有（旧版图）就补上空的
                    match &d.sxf {
                        Some(x) => {
                            self.u16(x.mode);
                            for l in &x.lines {
                                self.body(l);
                            }
                            for p in &x.points {
                                self.body(p);
                            }
                        }
                        None => {
                            self.u16(0);
                            let l = Ent { a: d.line.a.clone(), shape: Shape::Line { a: P::default(), b: P::default() } };
                            self.body(&l);
                            self.body(&l);
                            let p = Ent {
                                a: d.line.a.clone(),
                                shape: Shape::Point(Point { p: P::default(), temp: false, code: 0, angle: 0.0, scale: 1.0 }),
                            };
                            for _ in 0..4 {
                                self.body(&p);
                            }
                        }
                    }
                }
            }
            Shape::Solid { pts, rgb } => {
                for p in pts {
                    self.pt(*p);
                }
                if e.a.color == 10 {
                    self.u32(rgb.unwrap_or(0));
                }
            }
            Shape::Block { base, sx, sy, rot, number } => {
                self.pt(*base);
                self.f64(*sx);
                self.f64(*sy);
                self.f64(*rot);
                self.u32(*number);
            }
        }
    }

    fn ent(&mut self, e: &Ent) {
        let nm = match e.shape {
            Shape::Line { .. } => "CDataSen",
            Shape::Point(_) => "CDataTen",
            Shape::Arc(_) => "CDataEnko",
            Shape::Text(_) => "CDataMoji",
            Shape::Dim(_) => "CDataSunpou",
            Shape::Solid { .. } => "CDataSolid",
            Shape::Block { .. } => "CDataBlock",
        };
        self.klass(nm);
        self.body(e);
    }

    fn list(&mut self, v: &[Ent]) {
        self.count(v.len());
        for e in v {
            self.ent(e);
        }
    }
}

/// 设定部分开头：用纸、写入图层组、16 个图层组（状态・写入图层・缩尺・保护・各图层状态）。
/// 位置是固定的，直接按字段改写原始字节
fn patch_settings(d: &Doc) -> Vec<u8> {
    let mut s = d.settings_raw.clone();
    let mut put = |at: usize, v: &[u8]| s[at..at + v.len()].copy_from_slice(v);
    put(0, &d.paper.to_le_bytes());
    put(4, &d.write_glay.to_le_bytes());
    let mut at = 8;
    for g in d.glayers.iter().take(16) {
        put(at, &g.state.to_le_bytes());
        put(at + 4, &g.write_lay.to_le_bytes());
        put(at + 8, &g.scale.to_le_bytes());
        put(at + 16, &g.protect.to_le_bytes());
        at += 20;
        for k in 0..16 {
            put(at, &g.lay_state[k].to_le_bytes());
            put(at + 4, &g.lay_protect[k].to_le_bytes());
            at += 8;
        }
    }
    // 图层名・图层组名（长度可变，整段重写）
    let (n0, n1) = d.names_range;
    let mut w = Wr { b: Vec::new(), ver: d.version, classes: HashMap::new(), next: 1, schemas: HashMap::new() };
    for g in d.glayers.iter().take(16) {
        for k in 0..16 {
            w.cstr(&g.lay_name[k]);
        }
    }
    for g in d.glayers.iter().take(16) {
        w.cstr(&g.name);
    }
    s.splice(n0..n1, w.b);
    s
}

pub fn write_bytes(d: &Doc) -> Vec<u8> {
    let mut w = Wr {
        b: Vec::with_capacity(d.settings_raw.len() + d.ents.len() * 80),
        ver: d.version,
        classes: HashMap::new(),
        next: 1,
        schemas: d.schemas.iter().cloned().collect(),
    };
    w.b.extend_from_slice(b"JwwData.");
    w.u32(d.version);
    w.cstr(&d.memo);
    w.b.extend_from_slice(&patch_settings(d));
    w.list(&d.ents);
    w.count(d.blocks.len());
    for bl in &d.blocks {
        w.klass("CDataList");
        w.attr(&bl.a);
        w.u32(bl.number);
        w.u32(bl.referred as u32);
        w.u32(bl.time as i32 as u32); // CTime 32 位
        w.cstr(&bl.name);
        w.list(&bl.ents);
    }
    if d.version >= 700 {
        w.u32(d.images.len() as u32);
        for im in &d.images {
            w.cstr(&im.name);
            w.u32(im.data.len() as u32);
            w.b.extend_from_slice(&im.data);
        }
    }
    w.b
}

/// 図登用的模板（Jw_cad Ver.7 的图形文件，去掉图形部分只用文件头）
pub const JWS_TEMPLATE: &[u8] = include_bytes!("../assets/template.jws");

/// 外框用的点：圆弧取外沿（端点和经过的上下左右极点），其余用代表点
fn extent_points(e: &Ent) -> Vec<P> {
    match &e.shape {
        Shape::Arc(a) => {
            let at = |t: f64| {
                let (x, y) = (a.r * t.cos(), a.r * a.flat * t.sin());
                let (s, c) = a.tilt.sin_cos();
                P { x: a.c.x + x * c - y * s, y: a.c.y + x * s + y * c }
            };
            let (t0, sw) = if a.full { (0.0, std::f64::consts::TAU) } else { (a.start, a.sweep) };
            // 端点 + 细分（扁平・倾斜的椭圆也够准）
            (0..=360).map(|k| at(t0 + sw * k as f64 / 360.0)).collect()
        }
        Shape::Dim(d) => {
            let mut v = extent_points(&d.line);
            v.extend(extent_points(&d.text));
            v
        }
        _ => crate::edit::key_points(e),
    }
}

/// 図形ファイル（.jws）：模板 .jws 的文件头（版本・设定）里改基准点・缩尺・包围盒，
/// 后面写图形列表，再写空的ブロック定義（.jws 没有同梱图像那一段）
pub fn write_jws(tpl: &Jws, ents: &[Ent], base: P, scales: &[f64; 16]) -> Vec<u8> {
    let mut head = tpl.head.clone();
    let bbox_at = head.len() - 32;
    let mut put = |at: usize, v: f64| head[at..at + 8].copy_from_slice(&v.to_le_bytes());
    put(0xCC, base.x);
    put(0xD4, base.y);
    for (k, s) in scales.iter().enumerate() {
        put(0xDC + 8 * k, *s);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for e in ents {
        for q in extent_points(e) {
            (x0, y0, x1, y1) = (x0.min(q.x), y0.min(q.y), x1.max(q.x), y1.max(q.y));
        }
    }
    if x0 <= x1 {
        for (k, v) in [x0, y0, x1, y1].into_iter().enumerate() {
            put(bbox_at + 8 * k, v);
        }
    }
    let mut w = Wr { b: head, ver: tpl.version, classes: HashMap::new(), next: 1, schemas: tpl.schemas.iter().cloned().collect() };
    w.list(ents);
    w.count(0);
    w.b
}

/// 保存。目标已存在时先留一份 `.bak`（从不直接覆盖原图）
pub fn save(d: &Doc, path: &std::path::Path) -> std::io::Result<()> {
    if path.exists() {
        let bak = path.with_extension("bak");
        std::fs::copy(path, bak)?;
    }
    let tmp = path.with_extension("jww.tmp");
    std::fs::write(&tmp, write_bytes(d))?;
    std::fs::rename(&tmp, path)
}
