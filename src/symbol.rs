//! 線記号変形（記変）：Jw_cad の線記号変形データ（JW_OPT4*.DAT など、Shift-JIS のテキスト）を読む。
//!
//! 書式は Jw_cad 付属「変更建具データ/JW_OPT4.DAT」の説明に従う。いま対応しているのは基本の部分：
//! - 線・円弧（e0〜e3）・文字。座標は図面寸法（mm），倍率をかけて置く
//! - 制御コード 10（指示線１）があれば線を指示し、記号はその線の角度に合わせる。00 は水平のまま
//! - 100 の位 1/2：線色・線種を指示線と同じに。08・09（表示用・ダミー）、10・20（指示線そのもの）は作図しない
//! - 文字の「INPUT」「INPUT(n)」は入力した文字に置き換える
//! - 未対応：指示線２、指示線の切断・伸縮、ソリッド（a・b 指定）、制御コード 300 以上の特殊指定

use crate::cad::*;
use crate::edit::{make_text, Editor};
use crate::jww::{Attr, Shape, P};

#[derive(Clone, Debug)]
pub enum Item {
    Line { c1: i64, a: P, b: P, color: u16, style: u8 },
    /// e 指定：0 円弧のみ、1 中心→始点の線も、2 中心→終点の線も、3 扇形
    Arc { c1: i64, c: P, r: f64, flat: f64, tilt: f64, a0: f64, a1: f64, kind: u8, color: u16, style: u8 },
    /// 文字：基準点 at（0 左下〜9 右上），文字種 kind
    Text { c1: i64, at: P, s: String, kind: u32, base: u32 },
}

#[derive(Clone, Debug)]
pub struct Sym {
    pub name: String,
    /// 指示回数（0 = 線を指示した位置で作図）
    pub count: i64,
    /// 「700 倍率」の指定
    pub mag: f64,
    pub items: Vec<Item>,
    /// 指示線１（制御コード 10）が要る
    pub needs_line: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SymFile {
    pub name: String,
    pub syms: Vec<Sym>,
}

fn num(t: &str) -> Option<f64> {
    t.trim_start_matches('+').parse::<f64>().ok()
}

/// 制御コードの基本部分（100 の位・10000 の位を除いた下 2 桁）
fn base_code(c: i64) -> i64 {
    c.abs() % 100
}

fn parse_item(line: &str) -> Option<Item> {
    // 文字：「"」から後ろが文字列（$<フォント> や /! は外す）
    if let Some(q) = line.find('"') {
        let head: Vec<f64> = line[..q].split_whitespace().filter_map(num).collect();
        if head.len() < 8 || head[6] != 10000.0 {
            return None;
        }
        let mut s = &line[q + 1..];
        if let Some(e) = s.find('"') {
            s = &s[..e];
        }
        if let Some(e) = s.find("$<") {
            s = &s[..e];
        }
        let s = s.trim_end_matches("/!").trim().to_string();
        let st = head[7] as u32;
        return Some(Item::Text { c1: head[0] as i64, at: p(head[2], head[3]), s, kind: st % 100, base: (st / 100) % 10 });
    }
    let toks: Vec<&str> = line.split_whitespace().collect();
    let nums: Vec<f64> = toks.iter().map_while(|t| num(t)).collect();
    if nums.len() < 6 {
        return None;
    }
    let c1 = nums[0] as i64;
    let color = nums.get(6).map(|v| *v as u16).filter(|c| (1..=9).contains(c)).unwrap_or(0);
    let style = nums.get(7).map(|v| (*v as i64 % 100) as u8).unwrap_or(0);
    // 円弧：数字の後に e/E
    if let Some(t) = toks.get(nums.len()) {
        let lower = t.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix('e') {
            let after: Vec<f64> = toks[nums.len() + 1..].iter().map_while(|t| num(t)).collect();
            let kind: u8 = rest.parse().unwrap_or(0);
            let r = *after.first()?;
            let (r, flat) = if r < 0.0 { (-r, after.get(1).copied().unwrap_or(1.0)) } else { (r, after.get(1).copied().unwrap_or(1.0)) };
            let tilt = after.get(2).copied().unwrap_or(0.0);
            return Some(Item::Arc { c1, c: p(nums[2], nums[3]), r, flat, tilt, a0: nums[4], a1: nums[5], kind, color, style });
        }
        // a・b 指定（ソリッド）は今は無し
        if lower.starts_with('a') || lower.starts_with('b') {
            return None;
        }
    }
    Some(Item::Line { c1, a: p(nums[2], nums[3]), b: p(nums[4], nums[5]), color, style })
}

pub fn parse(bytes: &[u8], name: &str) -> SymFile {
    let (text, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
    let mut f = SymFile { name: name.to_string(), syms: Vec::new() };
    let mut lines = text.lines().map(|l| l.trim_end()).filter(|l| !l.trim().is_empty());
    // 先頭：#グループ名、データ数、999
    for l in lines.by_ref() {
        let t = l.trim();
        if let Some(g) = t.strip_prefix('#') {
            if f.name.is_empty() || f.name == name {
                f.name = g.trim().to_string();
            }
            continue;
        }
        if t.starts_with("99") {
            break;
        }
    }
    let mut cur: Option<Sym> = None;
    for l in lines {
        let t = l.trim();
        if t.starts_with('#') {
            continue;
        }
        let first = t.split_whitespace().next().unwrap_or("");
        let code = first.parse::<i64>().ok();
        match (&mut cur, code) {
            // 部材データの区切り 990〜999
            (Some(_), Some(c)) if (990..=999).contains(&c) => {
                let s = cur.take().unwrap();
                f.syms.push(s);
            }
            (None, Some(c)) => {
                // 記号の見出し：指示回数＋名前
                let name = t[first.len()..].trim().to_string();
                cur = Some(Sym { name, count: c, mag: 1.0, items: Vec::new(), needs_line: false });
            }
            (Some(s), Some(700)) => {
                s.mag = t.split_whitespace().nth(1).and_then(num).unwrap_or(1.0);
            }
            (Some(s), Some(_)) => {
                if let Some(it) = parse_item(t) {
                    let (c1, c2) = match &it {
                        Item::Line { c1, .. } | Item::Arc { c1, .. } | Item::Text { c1, .. } => {
                            (*c1, t.split_whitespace().nth(1).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0))
                        }
                    };
                    if base_code(c1) == 10 || base_code(c2) == 10 {
                        s.needs_line = true;
                    }
                    if !skipped(c1, c2) {
                        s.items.push(it);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some(s) = cur {
        f.syms.push(s);
    }
    f
}

pub fn read(path: &std::path::Path) -> std::io::Result<SymFile> {
    let b = std::fs::read(path)?;
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(parse(&b, &name))
}

/// 作図しない部材：表示用（08）・ダミー（09）・指示線そのもの（10・20）・特殊指定（300〜999）
fn skipped(c1: i64, c2: i64) -> bool {
    let (b1, b2) = (base_code(c1), base_code(c2));
    matches!(b1, 8 | 9 | 10 | 20) || matches!(b2, 8 | 9 | 10 | 20) || ((300..1000).contains(&c1.abs()) && c1.abs() / 100 > 2)
}

impl Editor {
    /// 記号を置く：原点 o、角度 ang（度）、倍率 mag。line_attr = 指示線の属性（100・200 の位の指定用）。
    /// inputs = 「INPUT」「INPUT(n)」に入れる文字。返回置いた図形
    pub fn place_symbol(&mut self, sym: &Sym, o: P, ang: f64, mag: f64, line_attr: Option<&Attr>, inputs: &[String]) -> Vec<Shape> {
        let shapes = symbol_shapes(self, sym, o, ang, mag, inputs);
        for (c1, color, style, s) in &shapes {
            let mut a = self.attr();
            if (100..300).contains(&c1.abs()) {
                if let Some(la) = line_attr {
                    a.color = la.color;
                    a.style = la.style;
                }
            } else if !matches!(s, Shape::Text(_)) {
                if *color != 0 {
                    a.color = *color;
                }
                if *style != 0 {
                    a.style = *style;
                }
            }
            if let Shape::Text(t) = s {
                a.style = 1;
                a.color = self.doc.moji[(t.kind as usize).clamp(1, 10)].color as u16;
            }
            self.push_attr(a, s.clone());
        }
        shapes.into_iter().map(|x| x.3).collect()
    }
}

/// 記号の図形（置く前の形，プレビューにも使う）：(制御コード, 線色, 線種, 図形)
pub fn symbol_shapes(ed: &Editor, sym: &Sym, o: P, ang: f64, mag: f64, inputs: &[String]) -> Vec<(i64, u16, u8, Shape)> {
    let m = mag * sym.mag;
    let th = rad(ang);
    let mut out = Vec::new();
    let tf = |c1: i64, q: P| {
        let v = mul(q, m);
        if base_code(c1) == 0 {
            add(o, v)
        } else {
            add(o, rot(v, th))
        }
    };
    let rot_of = |c1: i64| if base_code(c1) == 0 { 0.0 } else { th };
    for it in &sym.items {
        match it {
            Item::Line { c1, a, b, color, style } => {
                out.push((*c1, *color, *style, Shape::Line { a: tf(*c1, *a), b: tf(*c1, *b) }));
            }
            Item::Arc { c1, c, r, flat, tilt, a0, a1, kind, color, style } => {
                if *kind > 3 {
                    continue;
                }
                let cc = tf(*c1, *c);
                let r2 = r * m;
                let mut sw = rad(a1 - a0);
                if sw <= 0.0 {
                    sw += std::f64::consts::TAU;
                }
                let full = (sw - std::f64::consts::TAU).abs() < 1e-9;
                let tilt2 = rad(*tilt) + rot_of(*c1);
                let start = if full { 0.0 } else { rad(*a0) };
                let arc = crate::jww::Arc { c: cc, r: r2, start, sweep: if full { std::f64::consts::TAU } else { sw }, tilt: tilt2, flat: *flat, full };
                let pt = |t: f64| add(cc, rot(p(r2 * t.cos(), r2 * flat * t.sin()), tilt2));
                if *kind == 1 || *kind == 3 {
                    out.push((*c1, *color, *style, Shape::Line { a: cc, b: pt(rad(*a0)) }));
                }
                if *kind == 2 || *kind == 3 {
                    out.push((*c1, *color, *style, Shape::Line { a: cc, b: pt(rad(*a1)) }));
                }
                out.push((*c1, *color, *style, Shape::Arc(arc)));
            }
            Item::Text { c1, at, s, kind, base } => {
                // INPUT / INPUT(n)
                let s = if let Some(rest) = s.strip_prefix("INPUT") {
                    let n: usize = rest.trim_start_matches('(').trim_end_matches(')').parse().unwrap_or(1);
                    match inputs.get(n.saturating_sub(1)) {
                        Some(x) if !x.is_empty() => x.clone(),
                        _ => continue,
                    }
                } else {
                    s.clone()
                };
                let k = (*kind as usize).clamp(1, 10);
                let mj = ed.doc.moji[k];
                let deg_ = deg(rot_of(*c1));
                let mut t = make_text(p(0.0, 0.0), &s, mj.w, mj.h, mj.gap, deg_, k as u32, &ed.font);
                // 基準点（0 左下 1 中下 2 右下 3 左中 4 中中 5 右中 7 左上 8 中上 9 右上）→ 左下へ
                let w = dist(t.start, t.end);
                let (fx, fy) = match base {
                    1 => (0.5, 0.0),
                    2 => (1.0, 0.0),
                    3 => (0.0, 0.5),
                    4 => (0.5, 0.5),
                    5 => (1.0, 0.5),
                    6 | 7 => (0.0, 1.0),
                    8 => (0.5, 1.0),
                    9 => (1.0, 1.0),
                    _ => (0.0, 0.0),
                };
                let anchor = tf(*c1, *at);
                let u = p(rad(deg_).cos(), rad(deg_).sin());
                let start = sub(sub(anchor, mul(u, w * fx)), mul(perp(u), mj.h * fy));
                t.start = start;
                t.end = add(start, mul(u, w));
                out.push((*c1, 0, 0, Shape::Text(t)));
            }
        }
    }
    out
}
