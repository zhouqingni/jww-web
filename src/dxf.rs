//! DXF（AutoCAD の交換形式）の読み書き。
//!
//! 読込：線・円・円弧・楕円・ポリライン（ふくらみ付き）・スプライン・点・文字・MTEXT・ソリッド・
//! ブロック挿入（分解して取り込む）・寸法（寸法のブロックを分解）・引出線。
//! 座標は実寸。図面の大きさから縮尺を決めて（A3 に収まる一番小さい縮尺）図寸に直す。
//! DXF の画層は、Jw_cad 形式の名前（「0-1」「_a-1_名前」）ならそのレイヤへ、それ以外は
//! 出てきた順に 0-0, 0-1, … へ割り当ててレイヤ名に DXF の画層名を入れる。
//!
//! 書出：R12（AC1009）、Shift-JIS。座標は実寸（図寸 × レイヤグループの縮尺）。
//! ブロック・寸法は分解して書く。

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::TAU;

use crate::cad::*;
use crate::geom::Affine;
use crate::jww::{Arc, Attr, Doc, Ent, Point, Shape, P};

/// 読込の結果（メッセージ用）
#[derive(Debug, Default)]
pub struct Report {
    /// 決めた縮尺（1/scale）
    pub scale: f64,
    pub count: usize,
    pub layers: usize,
    /// 読めなかった図形の種類と数
    pub skipped: BTreeMap<String, usize>,
}

impl Report {
    pub fn message(&self) -> String {
        let mut s = format!("DXF を読み込みました：{} 図形、{} 画層、縮尺 1/{}", self.count, self.layers, fmt_scale(self.scale));
        if !self.skipped.is_empty() {
            let v: Vec<String> = self.skipped.iter().map(|(k, n)| format!("{k} {n}")).collect();
            s += &format!("（読めなかった図形：{}）", v.join("、"));
        }
        s
    }
}

fn fmt_scale(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

// ---------- 読込：グループコードの列 ----------

#[derive(Clone, Debug, Default)]
struct Raw {
    kind: String,
    g: Vec<(i32, String)>,
    /// POLYLINE の VERTEX、INSERT の ATTRIB
    subs: Vec<Raw>,
}

impl Raw {
    fn s(&self, code: i32) -> Option<&str> {
        self.g.iter().find(|(c, _)| *c == code).map(|(_, v)| v.as_str())
    }
    fn f(&self, code: i32) -> Option<f64> {
        self.s(code).and_then(|v| v.trim().parse().ok())
    }
    fn fd(&self, code: i32, d: f64) -> f64 {
        self.f(code).unwrap_or(d)
    }
    fn i(&self, code: i32) -> i32 {
        self.f(code).map_or(0, |v| v as i32)
    }
    fn pt(&self, code: i32) -> P {
        p(self.fd(code, 0.0), self.fd(code + 10, 0.0))
    }
    fn has(&self, code: i32) -> bool {
        self.s(code).is_some()
    }
}

/// 版と文字コードを先に見る（コードと値は ASCII なので latin-1 で十分）
fn sniff(b: &[u8]) -> (String, String) {
    let t: String = b.iter().take(200_000).map(|&c| c as char).collect();
    let lines: Vec<&str> = t.lines().map(|l| l.trim()).collect();
    let var = |name: &str| lines.iter().position(|l| *l == name).and_then(|i| lines.get(i + 2)).map(|s| s.to_string()).unwrap_or_default();
    (var("$ACADVER"), var("$DWGCODEPAGE").to_ascii_uppercase())
}

fn decode(b: &[u8]) -> String {
    let (ver, cp) = sniff(b);
    let new = ver.as_str() >= "AC1021" && ver.starts_with("AC");
    if new {
        let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b);
        return String::from_utf8_lossy(b).into_owned();
    }
    let enc = match cp.as_str() {
        "ANSI_1252" => encoding_rs::WINDOWS_1252,
        "ANSI_936" => encoding_rs::GBK,
        "ANSI_949" => encoding_rs::EUC_KR,
        "ANSI_950" => encoding_rs::BIG5,
        // ANSI_932、または書いていない（Jw_cad の DXF など）
        _ => encoding_rs::SHIFT_JIS,
    };
    enc.decode(b).0.into_owned()
}

fn pairs(text: &str) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    let mut it = text.lines();
    while let (Some(c), Some(v)) = (it.next(), it.next()) {
        let Ok(code) = c.trim().parse::<i32>() else { continue };
        out.push((code, v.trim_end_matches('\r').to_string()));
    }
    out
}

/// 0 グループで区切った図形の列。POLYLINE→VERTEX…SEQEND、INSERT→ATTRIB…SEQEND はまとめる
fn records(g: &[(i32, String)]) -> Vec<Raw> {
    let mut flat: Vec<Raw> = Vec::new();
    for (c, v) in g {
        if *c == 0 {
            flat.push(Raw { kind: v.trim().to_string(), ..Default::default() });
        } else if let Some(r) = flat.last_mut() {
            r.g.push((*c, v.clone()));
        }
    }
    let mut out: Vec<Raw> = Vec::new();
    let mut open = false;
    for r in flat {
        match r.kind.as_str() {
            "VERTEX" | "ATTRIB" if open => out.last_mut().unwrap().subs.push(r),
            "SEQEND" => open = false,
            "POLYLINE" => {
                open = true;
                out.push(r);
            }
            "INSERT" => {
                open = r.i(66) == 1;
                out.push(r);
            }
            _ => {
                open = false;
                out.push(r);
            }
        }
    }
    out
}

struct Layer {
    color: i32,
    ltype: String,
    off: bool,
}

/// 図形の属性（ブロックの中は親から受け継ぐ）
#[derive(Clone)]
struct Prop {
    layer: String,
    color: i32,
    ltype: String,
}

struct Reader {
    layers: HashMap<String, Layer>,
    blocks: HashMap<String, (P, Vec<Raw>)>,
    jw_ltypes: bool,
    out: Vec<(String, Attr, Shape)>,
    skipped: BTreeMap<String, usize>,
    group: u32,
}

/// 読み込んで d（新規図面）に入れる
pub fn import(b: &[u8], d: &mut Doc) -> Result<Report, String> {
    if b.starts_with(b"AutoCAD Binary DXF") {
        return Err("バイナリ形式の DXF は読めません。テキスト（ASCII）形式で保存し直してください".into());
    }
    let g = pairs(&decode(b));
    if !g.iter().any(|(c, v)| *c == 0 && v.trim() == "SECTION") {
        return Err("DXF ファイルではありません".into());
    }
    // セクションに分ける
    let mut secs: HashMap<String, Vec<(i32, String)>> = HashMap::new();
    let mut i = 0;
    while i < g.len() {
        if g[i].0 == 0 && g[i].1.trim() == "SECTION" {
            let name = g.get(i + 1).map(|x| x.1.trim().to_string()).unwrap_or_default();
            let mut j = i + 2;
            while j < g.len() && !(g[j].0 == 0 && g[j].1.trim() == "ENDSEC") {
                j += 1;
            }
            secs.entry(name).or_default().extend_from_slice(&g[(i + 2).min(j)..j]);
            i = j;
        }
        i += 1;
    }
    let header = secs.get("HEADER").cloned().unwrap_or_default();
    let unit = header
        .iter()
        .position(|(c, v)| *c == 9 && v.trim() == "$INSUNITS")
        .and_then(|k| header.get(k + 1))
        .and_then(|(_, v)| v.trim().parse::<i32>().ok())
        .map_or(1.0, |u| match u {
            1 => 25.4,
            2 => 304.8,
            5 => 10.0,
            6 => 1000.0,
            _ => 1.0,
        });

    let mut rd = Reader { layers: HashMap::new(), blocks: HashMap::new(), jw_ltypes: false, out: Vec::new(), skipped: BTreeMap::new(), group: 1 };
    let mut layer_order: Vec<String> = Vec::new();
    let mut ltypes: Vec<String> = Vec::new();
    for r in records(secs.get("TABLES").map_or(&[][..], |v| v)) {
        match r.kind.as_str() {
            "LAYER" => {
                let name = r.s(2).unwrap_or("0").trim().to_string();
                let c = r.i(62);
                rd.layers.insert(
                    name.clone(),
                    Layer { color: if c == 0 { 7 } else { c.abs() }, ltype: r.s(6).unwrap_or("CONTINUOUS").trim().to_string(), off: c < 0 || r.i(70) & 1 != 0 },
                );
                layer_order.push(name);
            }
            "LTYPE" => ltypes.push(r.s(2).unwrap_or("").trim().to_ascii_uppercase()),
            _ => {}
        }
    }
    rd.jw_ltypes = ["DASHED1", "PHANTOM2", "DOT"].iter().all(|n| ltypes.iter().any(|l| l == n));

    // ブロック
    let brecs = records(secs.get("BLOCKS").map_or(&[][..], |v| v));
    let mut cur: Option<(String, P, Vec<Raw>)> = None;
    for r in brecs {
        match r.kind.as_str() {
            "BLOCK" => cur = Some((r.s(2).unwrap_or("").trim().to_string(), r.pt(10), Vec::new())),
            "ENDBLK" => {
                if let Some((n, base, v)) = cur.take() {
                    rd.blocks.insert(n, (base, v));
                }
            }
            _ => {
                if let Some(c) = &mut cur {
                    c.2.push(r);
                }
            }
        }
    }

    let top = Prop { layer: "0".into(), color: 7, ltype: "CONTINUOUS".into() };
    let m0 = Affine::scale(unit, unit);
    for r in records(secs.get("ENTITIES").map_or(&[][..], |v| v)) {
        if r.i(67) == 1 && r.kind != "VIEWPORT" {
            *rd.skipped.entry("ペーパー空間".into()).or_default() += 1;
            continue;
        }
        rd.ent(&r, &m0, &top, 0);
    }

    // 縮尺：A3（420×297）の 9 割に収まる一番小さい標準縮尺
    let mut bb = crate::geom::BBox::EMPTY;
    for (_, _, s) in &rd.out {
        for q in shape_pts(s) {
            bb.add(q);
        }
    }
    let scale = if bb.is_empty() {
        1.0
    } else {
        let (w, h) = (bb.x1 - bb.x0, bb.y1 - bb.y0);
        const STD: [f64; 24] = [
            1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 30.0, 50.0, 100.0, 150.0, 200.0, 250.0, 300.0, 400.0, 500.0, 600.0, 1000.0, 1500.0, 2000.0,
            2500.0, 3000.0, 5000.0, 10000.0,
        ];
        let fits = |s: f64| w / s <= 420.0 * 0.9 && h / s <= 297.0 * 0.9;
        STD.iter().copied().find(|&s| fits(s)).unwrap_or_else(|| (w / 378.0).max(h / 267.0).ceil())
    };

    // 画層 → レイヤ
    let mut used: Vec<String> = Vec::new();
    for (l, _, _) in &rd.out {
        if !used.contains(l) {
            used.push(l.clone());
        }
    }
    used.sort_by_key(|l| layer_order.iter().position(|x| x == l).unwrap_or(usize::MAX));
    let mut slot: HashMap<String, (usize, usize)> = HashMap::new();
    let mut taken = [[false; 16]; 16];
    let mut names: Vec<(usize, usize, String)> = Vec::new();
    for l in &used {
        if let Some((g, k, name)) = jw_layer_name(l) {
            if !taken[g][k] {
                taken[g][k] = true;
                slot.insert(l.clone(), (g, k));
                names.push((g, k, name));
            }
        }
    }
    let mut next = 0usize;
    for l in &used {
        if slot.contains_key(l) {
            continue;
        }
        while next < 256 && taken[next / 16][next % 16] {
            next += 1;
        }
        let at = if next < 256 { (next / 16, next % 16) } else { (15, 15) };
        if next < 256 {
            taken[at.0][at.1] = true;
        }
        slot.insert(l.clone(), at);
        names.push((at.0, at.1, l.clone()));
    }

    for gl in d.glayers.iter_mut() {
        gl.scale = scale;
    }
    for (g, k, name) in &names {
        let name: String = name.chars().take(31).collect();
        d.glayers[*g].lay_name[*k] = name;
    }
    for (l, (g, k)) in &slot {
        if rd.layers.get(l).map_or(false, |x| x.off) && !(*g == 0 && *k == 0) {
            d.glayers[*g].lay_state[*k] = 0;
        }
    }
    let k = 1.0 / scale;
    // 用紙（原点が中心）に図が掛かっていなければ、図の中心を用紙の中心へ移す
    // （Jw_cad で開いた時に用紙の外にあって見えない、を防ぐ）
    let mut shift = p(0.0, 0.0);
    if !bb.is_empty() {
        let (pw, ph) = d.paper_size();
        let on_paper = bb.x0 * k < pw / 2.0 && bb.x1 * k > -pw / 2.0 && bb.y0 * k < ph / 2.0 && bb.y1 * k > -ph / 2.0;
        if !on_paper {
            shift = p(-(bb.x0 + bb.x1) / 2.0 * k, -(bb.y0 + bb.y1) / 2.0 * k);
        }
        // 印刷枠は図面の真ん中に（等倍）
        d.prt_origin = p((bb.x0 + bb.x1) / 2.0 * k + shift.x, (bb.y0 + bb.y1) / 2.0 * k + shift.y);
        d.prt_set = d.prt_set % 10 + d.prt_set / 100 * 100;
        d.prt_scale = 1.0;
    }
    let n = rd.out.len();
    for (l, mut a, s) in rd.out {
        let (g, k2) = slot[&l];
        a.glayer = g as u16;
        a.layer = k2 as u16;
        d.ents.push(Ent { a, shape: move_shape(scale_shape(s, k), shift) });
    }
    Ok(Report { scale, count: n, layers: names.len(), skipped: rd.skipped })
}

/// 「0-1」「_a-1_名前」「A-F 名前」→ (グループ, レイヤ, 名前)
fn jw_layer_name(s: &str) -> Option<(usize, usize, String)> {
    let t = s.strip_prefix('_').unwrap_or(s);
    let c: Vec<char> = t.chars().collect();
    if c.len() < 3 || c[1] != '-' {
        return None;
    }
    let g = c[0].to_digit(16)? as usize;
    let k = c[2].to_digit(16)? as usize;
    let rest: String = c[3..].iter().collect();
    if !rest.is_empty() && !rest.starts_with(['_', ' ']) {
        return None;
    }
    let name = rest.strip_prefix(['_', ' ']).unwrap_or(&rest);
    Some((g, k, name.to_string()))
}

impl Reader {
    fn skip(&mut self, kind: &str) {
        *self.skipped.entry(kind.to_string()).or_default() += 1;
    }

    fn prop(&self, r: &Raw, parent: &Prop) -> Prop {
        let mut layer = r.s(8).unwrap_or("0").trim().to_string();
        if layer == "0" {
            layer = parent.layer.clone();
        }
        let lay = self.layers.get(&layer);
        let color = match r.f(62).map(|v| v as i32) {
            Some(0) => parent.color,
            Some(c) if c != 256 => c.abs(),
            _ => lay.map_or(7, |l| l.color),
        };
        let lt = r.s(6).map(|s| s.trim().to_string()).unwrap_or_else(|| "BYLAYER".into());
        let ltype = match lt.to_ascii_uppercase().as_str() {
            "BYLAYER" => lay.map_or("CONTINUOUS".into(), |l| l.ltype.clone()),
            "BYBLOCK" => parent.ltype.clone(),
            _ => lt,
        };
        Prop { layer, color, ltype }
    }

    fn attr(&self, pr: &Prop) -> Attr {
        Attr { group: 0, style: self.style(&pr.ltype), color: aci_to_jw(pr.color), width: 0, layer: 0, glayer: 0, flag: 0 }
    }

    fn style(&self, lt: &str) -> u8 {
        let u = lt.to_ascii_uppercase();
        if self.jw_ltypes {
            match u.as_str() {
                "DASHED1" => return 2,
                "DASHED2" => return 3,
                "DASHED3" => return 4,
                "CENTER1" => return 5,
                "CENTER2" => return 6,
                "PHANTOM1" => return 7,
                "PHANTOM2" => return 8,
                "DOT" => return 9,
                _ => {}
            }
        }
        if u.contains("PHANTOM") || u.contains("DIVIDE") || u.contains("DOUBLE-DOT") || u.contains("DOUBLE_DOT") || u.contains("二点") {
            7
        } else if u.contains("CENTER") || u.contains("DASHDOT") || u.contains("DASH_DOT") || u.contains("DASHED_DOTTED") || u.contains("CHAIN") || u.contains("一点") {
            5
        } else if u.contains("DASH") || u.contains("HIDDEN") || u.contains("DOT") || u.contains("点線") {
            2
        } else {
            1
        }
    }

    fn push(&mut self, pr: &Prop, a: Attr, s: Shape) {
        self.out.push((pr.layer.clone(), a, s));
    }

    fn line(&mut self, pr: &Prop, a: &Attr, m: &Affine, x: P, y: P) {
        let (x, y) = (m.apply(x), m.apply(y));
        if dist(x, y) > 1e-12 {
            self.push(pr, a.clone(), Shape::Line { a: x, b: y });
        }
    }

    /// 折れ線（曲線属性で一本にまとめる）
    fn polyline(&mut self, pr: &Prop, a: &Attr, m: &Affine, pts: &[P]) {
        let mut a = a.clone();
        a.group = self.group;
        self.group += 1;
        for w in pts.windows(2) {
            self.line(pr, &a, m, w[0], w[1]);
        }
    }

    /// 円・円弧（ローカル座標）。m で一様拡大・回転・鏡映なら円弧のまま、ゆがむなら楕円
    fn arc(&mut self, pr: &Prop, a: &Attr, m: &Affine, c: P, r: f64, a0: f64, sweep: f64) {
        self.ellipse(pr, a, m, c, p(r, 0.0), 1.0, a0, sweep);
    }

    /// 楕円（中心、長軸ベクトル、短/長、パラメータ角）
    fn ellipse(&mut self, pr: &Prop, a: &Attr, m: &Affine, c: P, major: P, ratio: f64, t0: f64, sweep: f64) {
        let full = sweep >= TAU - 1e-9;
        let minor = mul(perp(major), ratio);
        let lin = |v: P| p(m.a * v.x + m.c * v.y, m.b * v.x + m.d * v.y);
        let (ma, mi) = (lin(major), lin(minor));
        let det = m.a * m.d - m.b * m.c;
        // 変換後も直交なら楕円（円）のまま
        if dot(ma, mi).abs() <= 1e-9 * len(ma).max(1e-12) * len(mi).max(1e-12) && len(ma) > 1e-12 {
            let r = len(ma);
            let flat = len(mi) / r;
            let tilt = angle_of(ma);
            let (start, sw) = if det >= 0.0 { (t0, sweep) } else { (-(t0 + sweep), sweep) };
            let c2 = m.apply(c);
            let arc = if (flat - 1.0).abs() < 1e-9 {
                // 円：傾きを開始角に入れる
                match arc_shape(c2, r, start + tilt, if full { TAU } else { sw }) {
                    Shape::Arc(x) => x,
                    _ => unreachable!(),
                }
            } else {
                Arc { c: c2, r, start: if full { 0.0 } else { norm_ang(start) }, sweep: if full { TAU } else { sw }, tilt, flat, full }
            };
            self.push(pr, a.clone(), Shape::Arc(arc));
            return;
        }
        // ゆがむ時は折れ線
        let n = ((sweep.abs() / TAU * 72.0).ceil() as usize).max(8);
        let pts: Vec<P> = (0..=n)
            .map(|k| {
                let t = t0 + sweep * k as f64 / n as f64;
                add(c, add(mul(major, t.cos()), mul(minor, t.sin())))
            })
            .collect();
        self.polyline(pr, a, m, &pts);
    }

    /// ふくらみ（bulge）付きの頂点列
    fn bulged(&mut self, pr: &Prop, a: &Attr, m: &Affine, v: &[(P, f64)], closed: bool) {
        let n = v.len();
        if n == 0 {
            return;
        }
        if n == 1 {
            return;
        }
        let segs = if closed { n } else { n - 1 };
        for k in 0..segs {
            let (p1, b) = v[k];
            let p2 = v[(k + 1) % n].0;
            if b.abs() < 1e-12 {
                self.line(pr, a, m, p1, p2);
                continue;
            }
            let th = 4.0 * b.atan();
            let d = dist(p1, p2);
            if d < 1e-12 {
                continue;
            }
            let u = norm(sub(p2, p1));
            let h = (d / 2.0) / (th / 2.0).tan();
            let c = add(mid(p1, p2), mul(perp(u), h));
            let r = dist(c, p1);
            let (s, sw) = if th > 0.0 { (angle_of(sub(p1, c)), th) } else { (angle_of(sub(p2, c)), -th) };
            self.arc(pr, a, m, c, r, s, sw);
        }
    }

    fn text(&mut self, pr: &Prop, a: &Attr, m: &Affine, r: &Raw, mtext: bool) {
        let mut a = a.clone();
        a.style = 1;
        let h0 = r.fd(40, 2.5);
        let (raw, mut ang) = if mtext {
            let mut s = String::new();
            for (c, v) in &r.g {
                if *c == 3 || *c == 1 {
                    s += v;
                }
            }
            let ang = if r.has(11) { angle_of(r.pt(11)) } else { rad(r.fd(50, 0.0)) };
            (s, ang)
        } else {
            (r.s(1).unwrap_or("").to_string(), rad(r.fd(50, 0.0)))
        };
        let lines: Vec<String> = if mtext { mtext_lines(&raw) } else { vec![text_escapes(&raw)] };
        // 変換：位置と向き、文字の大きさ
        let lin = |v: P| p(m.a * v.x + m.c * v.y, m.b * v.x + m.d * v.y);
        let wf = if mtext { 1.0 } else { r.fd(41, 1.0) };
        let half = |s: &str, w: f64| s.chars().map(|c| if crate::jww::is_half(c) { w / 2.0 } else { w }).sum::<f64>();
        let (h, w) = (h0, h0 * wf);
        // 左下, 文字, 幅, 高さ, 角度（ローカル）
        let mut items: Vec<(P, String, f64, f64, f64)> = Vec::new();
        if mtext {
            let dir = p(ang.cos(), ang.sin());
            let up = perp(dir);
            let at = r.i(71).max(1);
            let (col, row) = ((at - 1) % 3, (at - 1) / 3);
            let gap = h * 5.0 / 3.0;
            let total = h + gap * (lines.len().saturating_sub(1)) as f64;
            let ins = r.pt(10);
            // 挿入点から一行目の上端まで
            let top = match row {
                0 => 0.0,
                1 => total / 2.0,
                _ => total,
            };
            for (k, s) in lines.iter().enumerate() {
                if s.trim().is_empty() {
                    continue;
                }
                let wid = half(s, w);
                let dx = match col {
                    0 => 0.0,
                    1 => -wid / 2.0,
                    _ => -wid,
                };
                let dy = top - h - gap * k as f64;
                items.push((add(ins, add(mul(dir, dx), mul(up, dy))), s.clone(), w, h, ang));
            }
        } else {
            let s = lines.into_iter().next().unwrap_or_default();
            if s.trim().is_empty() {
                return;
            }
            let (ha, va) = (r.i(72), r.i(73));
            let p10 = r.pt(10);
            let p11 = if r.has(11) { r.pt(11) } else { p10 };
            let wid = half(&s, w);
            let (mut ww, mut hh) = (w, h);
            let mut base = p10;
            if (ha == 3 || ha == 5) && dist(p10, p11) > 1e-12 {
                // 両端合わせ（3 は高さも比例、5 は幅だけ）
                ang = angle_of(sub(p11, p10));
                let k = dist(p10, p11) / wid.max(1e-12);
                ww *= k;
                if ha == 3 {
                    hh *= k;
                }
            } else if ha == 1 || ha == 2 || ha == 4 {
                let f = if ha == 2 { 1.0 } else { 0.5 };
                base = sub(p11, mul(p(ang.cos(), ang.sin()), wid * f));
            } else if va != 0 {
                base = p11;
            }
            let up = perp(p(ang.cos(), ang.sin()));
            let dy = match (ha, va) {
                (4, _) | (_, 2) => -hh / 2.0,
                (_, 3) => -hh,
                _ => 0.0,
            };
            items.push((add(base, mul(up, dy)), s, ww, hh, ang));
        }
        let det = m.a * m.d - m.b * m.c;
        let k = det.abs().sqrt();
        for (q, s, ww, hh, an) in items {
            let start = m.apply(q);
            let d2 = lin(p(an.cos(), an.sin()));
            let deg_ = deg(angle_of(d2)).rem_euclid(360.0);
            let t = crate::edit::make_text(start, &s, ww * k, hh * k, 0.0, deg_, 0, "ＭＳ ゴシック");
            self.push(pr, a.clone(), Shape::Text(t));
        }
    }

    /// HATCH：境界を多角形にして、塗り潰し（ソリッド）か模様の線にする
    fn hatch(&mut self, pr: &Prop, a: &Attr, m: &Affine, r: &Raw) {
        let Some(h) = parse_hatch(r) else {
            self.skip("HATCH");
            return;
        };
        let loops: Vec<Vec<P>> = h.loops.iter().map(|v| v.iter().map(|q| m.apply(*q)).collect::<Vec<P>>()).filter(|v| v.len() >= 3).collect();
        if loops.is_empty() {
            self.skip("HATCH");
            return;
        }
        if h.solid {
            // 偶奇規則で横に切った台形の列（穴あきも正しく）
            let mut ys: Vec<f64> = loops.iter().flatten().map(|q| q.y).collect();
            ys.sort_by(|x, y| x.total_cmp(y));
            ys.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
            let mut a = a.clone();
            a.style = 1;
            for w in ys.windows(2) {
                let (y0, y1) = (w[0], w[1]);
                let ym = (y0 + y1) / 2.0;
                // 中央の高さで交わる辺（下端・上端での x も）
                let mut xs: Vec<(f64, f64, f64)> = Vec::new();
                for v in &loops {
                    for k in 0..v.len() {
                        let (s, e) = (v[k], v[(k + 1) % v.len()]);
                        if (s.y > ym) != (e.y > ym) {
                            let at = |y: f64| s.x + (e.x - s.x) * (y - s.y) / (e.y - s.y);
                            xs.push((at(ym), at(y0), at(y1)));
                        }
                    }
                }
                xs.sort_by(|x, y| x.0.total_cmp(&y.0));
                for c in xs.chunks_exact(2) {
                    let q = [p(c[0].1, y0), p(c[1].1, y0), p(c[1].2, y1), p(c[0].2, y1)];
                    self.push(pr, a.clone(), Shape::Solid { pts: q, rgb: None });
                }
            }
            return;
        }
        // 模様：各模様線を偶奇規則で切り、破線に
        let det = m.a * m.d - m.b * m.c;
        let k = det.abs().sqrt();
        let mut n = 0usize;
        let (mut lo, mut hi) = (p(f64::MAX, f64::MAX), p(f64::MIN, f64::MIN));
        for q in loops.iter().flatten() {
            lo = p(lo.x.min(q.x), lo.y.min(q.y));
            hi = p(hi.x.max(q.x), hi.y.max(q.y));
        }
        let corners = [lo, p(hi.x, lo.y), hi, p(lo.x, hi.y)];
        for pl in &h.pattern {
            let base = m.apply(pl.base);
            let u = norm(p(m.a * pl.ang.cos() + m.c * pl.ang.sin(), m.b * pl.ang.cos() + m.d * pl.ang.sin()));
            let off = p(m.a * pl.off.x + m.c * pl.off.y, m.b * pl.off.x + m.d * pl.off.y);
            let nrm = perp(u);
            let step = dot(off, nrm);
            if step.abs() < 1e-9 * k.max(1e-12) {
                continue;
            }
            let shift = dot(off, u);
            let dashes: Vec<f64> = pl.dashes.iter().map(|d| d * k).collect();
            let period: f64 = dashes.iter().map(|d| d.abs()).sum();
            let ds: Vec<f64> = corners.iter().map(|q| dot(sub(*q, base), nrm) / step).collect();
            let (i0, i1) = (ds.iter().cloned().fold(f64::MAX, f64::min).floor() as i64, ds.iter().cloned().fold(f64::MIN, f64::max).ceil() as i64);
            if i1 - i0 > 5000 {
                self.skip("HATCH（模様が細かすぎ）");
                return;
            }
            for i in i0..=i1 {
                let a0 = add(base, add(mul(nrm, step * i as f64), mul(u, shift * i as f64)));
                let mut ts: Vec<f64> = loops.iter().flat_map(|v| crate::fill::cross_params(v, a0, u)).collect();
                ts.sort_by(|x, y| x.total_cmp(y));
                for c in ts.chunks_exact(2) {
                    let (t0, t1) = (c[0], c[1]);
                    if t1 - t0 < 1e-9 {
                        continue;
                    }
                    if dashes.is_empty() || period < 1e-9 {
                        self.line(pr, a, &Affine::ID, add(a0, mul(u, t0)), add(a0, mul(u, t1)));
                        n += 1;
                        continue;
                    }
                    // 破線：線の原点（a0）から周期で並ぶ
                    let mut t = (t0 / period).floor() * period;
                    while t < t1 && n < 200_000 {
                        for d in &dashes {
                            let (s, e) = (t, t + d.abs());
                            if *d >= 0.0 {
                                let (s2, e2) = (s.max(t0), e.min(t1));
                                if e2 > s2 + 1e-9 || (*d == 0.0 && s >= t0 && s <= t1) {
                                    if *d == 0.0 {
                                        let q = add(a0, mul(u, s));
                                        let mut pa = a.clone();
                                        pa.style = 1;
                                        self.push(pr, pa, Shape::Point(Point { p: q, temp: false, code: 0, angle: 0.0, scale: 1.0 }));
                                    } else {
                                        self.line(pr, a, &Affine::ID, add(a0, mul(u, s2)), add(a0, mul(u, e2)));
                                    }
                                    n += 1;
                                }
                            }
                            t = e;
                        }
                    }
                }
            }
        }
    }

    fn ent(&mut self, r: &Raw, m: &Affine, parent: &Prop, depth: u32) {
        let pr = self.prop(r, parent);
        let a = self.attr(&pr);
        // OCS（押し出し方向が -Z）：X を反転
        let ocs = r.f(230).map_or(false, |z| z < 0.0);
        let mo = if ocs { m.then(&Affine::scale(-1.0, 1.0)) } else { *m };
        match r.kind.as_str() {
            "LINE" => self.line(&pr, &a, m, r.pt(10), r.pt(11)),
            "POINT" => {
                let mut a = a;
                a.style = 1;
                let q = m.apply(r.pt(10));
                self.push(&pr, a, Shape::Point(Point { p: q, temp: false, code: 0, angle: 0.0, scale: 1.0 }));
            }
            "CIRCLE" => self.arc(&pr, &a, &mo, r.pt(10), r.fd(40, 0.0), 0.0, TAU),
            "ARC" => {
                let (s, e) = (rad(r.fd(50, 0.0)), rad(r.fd(51, 360.0)));
                let mut sw = norm_ang(e - s);
                if sw < 1e-12 {
                    sw = TAU;
                }
                self.arc(&pr, &a, &mo, r.pt(10), r.fd(40, 0.0), s, sw);
            }
            "ELLIPSE" => {
                let (t0, t1) = (r.fd(41, 0.0), r.fd(42, TAU));
                let mut sw = t1 - t0;
                if sw <= 1e-12 {
                    sw += TAU;
                }
                let me = if ocs { m.then(&Affine::scale(-1.0, 1.0)) } else { *m };
                self.ellipse(&pr, &a, &me, r.pt(10), r.pt(11), r.fd(40, 1.0), t0, sw.min(TAU));
            }
            "LWPOLYLINE" => {
                let mut v: Vec<(P, f64)> = Vec::new();
                for (c, val) in &r.g {
                    let x = val.trim().parse::<f64>().unwrap_or(0.0);
                    match c {
                        10 => v.push((p(x, 0.0), 0.0)),
                        20 => {
                            if let Some(l) = v.last_mut() {
                                l.0.y = x;
                            }
                        }
                        42 => {
                            if let Some(l) = v.last_mut() {
                                l.1 = x;
                            }
                        }
                        _ => {}
                    }
                }
                self.bulged(&pr, &a, &mo, &v, r.i(70) & 1 != 0);
            }
            "POLYLINE" => {
                let fl = r.i(70);
                if fl & (16 | 64) != 0 {
                    self.skip("メッシュ");
                    return;
                }
                let v: Vec<(P, f64)> = r.subs.iter().filter(|x| x.i(70) & 16 == 0).map(|x| (x.pt(10), x.fd(42, 0.0))).collect();
                let m2 = if fl & 8 != 0 { *m } else { mo };
                self.bulged(&pr, &a, &m2, &v, fl & 1 != 0);
            }
            "SPLINE" => {
                let pts = spline_points(r);
                if pts.len() >= 2 {
                    self.polyline(&pr, &a, m, &pts);
                }
            }
            "LEADER" => {
                let v: Vec<P> = pts_of(r, 10);
                for w in v.windows(2) {
                    self.line(&pr, &a, m, w[0], w[1]);
                }
            }
            "SOLID" | "TRACE" => {
                let (p0, p1, p2, p3) = (r.pt(10), r.pt(11), r.pt(12), if r.has(13) { r.pt(13) } else { r.pt(12) });
                let q = [p0, p1, p3, p2].map(|x| mo.apply(x));
                self.push(&pr, a, Shape::Solid { pts: q, rgb: None });
            }
            "TEXT" | "ATTRIB" => {
                if r.i(70) & 1 != 0 && r.kind == "ATTRIB" {
                    return; // 非表示の属性
                }
                self.text(&pr, &a, &mo, r, false)
            }
            "MTEXT" => self.text(&pr, &a, m, r, true),
            "INSERT" => {
                if depth > 16 {
                    return;
                }
                let name = r.s(2).unwrap_or("").trim().to_string();
                let Some((base, ents)) = self.blocks.get(&name).cloned() else {
                    self.skip("INSERT（ブロック定義なし）");
                    return;
                };
                let (sx, sy, rot) = (r.fd(41, 1.0), r.fd(42, 1.0), rad(r.fd(50, 0.0)));
                let (nc, nr) = (r.i(70).max(1), r.i(71).max(1));
                let (dc, dr) = (r.fd(44, 0.0), r.fd(45, 0.0));
                for ir in 0..nr {
                    for ic in 0..nc {
                        let off = rot_v(p(dc * ic as f64, dr * ir as f64), rot);
                        let ins = add(r.pt(10), off);
                        let mm = mo
                            .then(&Affine::translate(ins.x, ins.y))
                            .then(&Affine::rotate(rot))
                            .then(&Affine::scale(sx, sy))
                            .then(&Affine::translate(-base.x, -base.y));
                        for c in &ents {
                            self.ent(c, &mm, &pr, depth + 1);
                        }
                    }
                }
                for at in &r.subs {
                    self.ent(at, m, parent, depth + 1);
                }
            }
            "DIMENSION" => {
                let name = r.s(2).unwrap_or("").trim().to_string();
                let Some((_, ents)) = self.blocks.get(&name).cloned() else {
                    self.skip("DIMENSION");
                    return;
                };
                // 寸法のブロックは WCS のまま原点に置かれている
                for c in &ents {
                    self.ent(c, m, &pr, depth + 1);
                }
            }
            "HATCH" => self.hatch(&pr, &a, &mo, r),
            "ATTDEF" | "VIEWPORT" | "SEQEND" | "VERTEX" | "ENDBLK" | "BLOCK" => {}
            k => self.skip(k),
        }
    }
}

struct PatLine {
    ang: f64,
    base: P,
    off: P,
    dashes: Vec<f64>,
}

struct Hatch {
    solid: bool,
    loops: Vec<Vec<P>>,
    pattern: Vec<PatLine>,
}

/// HATCH のグループを順に読む
fn parse_hatch(r: &Raw) -> Option<Hatch> {
    let g = &r.g;
    let mut i = g.iter().position(|(c, _)| *c == 91)?;
    let num = |i: usize| g.get(i).and_then(|(_, v)| v.trim().parse::<f64>().ok()).unwrap_or(0.0);
    let code = |i: usize| g.get(i).map_or(-1, |x| x.0);
    let npaths = num(i) as usize;
    i += 1;
    let mut loops = Vec::new();
    // 次に code が来る所まで進める
    let seek = |i: &mut usize, c: i32| {
        while *i < g.len() && g[*i].0 != c {
            *i += 1;
        }
    };
    for _ in 0..npaths {
        seek(&mut i, 92);
        if i >= g.len() {
            break;
        }
        let flags = num(i) as i32;
        i += 1;
        let mut pts: Vec<P> = Vec::new();
        if flags & 2 != 0 {
            // ポリライン
            seek(&mut i, 72);
            let bulge = num(i) != 0.0;
            i += 1;
            seek(&mut i, 93);
            let n = num(i) as usize;
            i += 1;
            let mut v: Vec<(P, f64)> = Vec::new();
            for _ in 0..n {
                seek(&mut i, 10);
                let q = p(num(i), num(i + 1));
                i += 2;
                let b = if bulge && code(i) == 42 {
                    i += 1;
                    num(i - 1)
                } else {
                    0.0
                };
                v.push((q, b));
            }
            for k in 0..v.len() {
                let (p1, b) = v[k];
                let p2 = v[(k + 1) % v.len()].0;
                pts.push(p1);
                if b.abs() > 1e-12 && dist(p1, p2) > 1e-12 {
                    let th = 4.0 * b.atan();
                    let d = dist(p1, p2);
                    let c = add(mid(p1, p2), mul(perp(norm(sub(p2, p1))), (d / 2.0) / (th / 2.0).tan()));
                    let a0 = angle_of(sub(p1, c));
                    let rr = dist(c, p1);
                    let n = ((th.abs() / TAU * 72.0).ceil() as usize).max(4);
                    for s in 1..n {
                        pts.push(polar(c, rr, a0 + th * s as f64 / n as f64));
                    }
                }
            }
        } else {
            seek(&mut i, 93);
            let ne = num(i) as usize;
            i += 1;
            for _ in 0..ne {
                seek(&mut i, 72);
                let t = num(i) as i32;
                i += 1;
                let f = |c: i32, i: &mut usize| -> f64 {
                    seek(i, c);
                    *i += 1;
                    num(*i - 1)
                };
                match t {
                    1 => {
                        let (x0, y0, x1, y1) = (f(10, &mut i), f(20, &mut i), f(11, &mut i), f(21, &mut i));
                        if pts.last().map_or(true, |q| dist(*q, p(x0, y0)) > 1e-9) {
                            pts.push(p(x0, y0));
                        }
                        pts.push(p(x1, y1));
                    }
                    2 | 3 => {
                        let c = p(f(10, &mut i), f(20, &mut i));
                        let (major, ratio) = if t == 3 {
                            let mj = p(f(11, &mut i), f(21, &mut i));
                            (mj, f(40, &mut i))
                        } else {
                            (p(f(40, &mut i), 0.0), 1.0)
                        };
                        let (s, e) = (rad(f(50, &mut i)), rad(f(51, &mut i)));
                        let ccw = if code(i) == 73 {
                            i += 1;
                            num(i - 1) != 0.0
                        } else {
                            true
                        };
                        let mut sw = e - s;
                        if sw <= 0.0 {
                            sw += TAU;
                        }
                        let n = ((sw / TAU * 72.0).ceil() as usize).max(4);
                        let minor = mul(perp(major), ratio);
                        for k in 0..=n {
                            let mut tt = s + sw * k as f64 / n as f64;
                            if !ccw {
                                tt = -tt;
                            }
                            pts.push(add(c, add(mul(major, tt.cos()), mul(minor, tt.sin()))));
                        }
                    }
                    4 => {
                        // スプライン：制御点で近似
                        seek(&mut i, 95);
                        let nk = num(i) as usize;
                        seek(&mut i, 96);
                        let nc = num(i) as usize;
                        i += 1 + nk;
                        for _ in 0..nc {
                            seek(&mut i, 10);
                            pts.push(p(num(i), num(i + 1)));
                            i += 2;
                        }
                        while i < g.len() && matches!(g[i].0, 42) {
                            i += 1;
                        }
                        if code(i) == 97 {
                            let nf = num(i) as usize;
                            i += 1 + nf * 2;
                        }
                        while i < g.len() && matches!(g[i].0, 12 | 22 | 13 | 23) {
                            i += 1;
                        }
                    }
                    _ => return None,
                }
            }
        }
        // 境界オブジェクトの参照
        if code(i) == 97 {
            let n = num(i) as usize;
            i += 1 + n;
        }
        loops.push(pts);
    }
    let rest = &g[i.min(g.len())..];
    let after = |c: i32| rest.iter().find(|(x, _)| *x == c).and_then(|(_, v)| v.trim().parse::<f64>().ok());
    let solid = r.i(70) == 1 || after(70) == Some(1.0);
    let mut pattern = Vec::new();
    if !solid {
        // 模様線：53 角度、43/44 基点、45/46 オフセット、79 個数、49 破線
        let mut k = 0;
        while k < rest.len() {
            if rest[k].0 == 53 {
                let v = |j: usize| rest.get(k + j).and_then(|(_, s)| s.trim().parse::<f64>().ok()).unwrap_or(0.0);
                let ang = rad(v(0));
                let base = p(v(1), v(2));
                let off = p(v(3), v(4));
                let nd = v(5) as usize;
                let dashes = (0..nd).map(|j| v(6 + j)).collect();
                pattern.push(PatLine { ang, base, off, dashes });
                k += 6 + nd;
            } else {
                k += 1;
            }
        }
    }
    Some(Hatch { solid, loops, pattern })
}

fn rot_v(v: P, a: f64) -> P {
    let (s, c) = a.sin_cos();
    p(v.x * c - v.y * s, v.x * s + v.y * c)
}

fn pts_of(r: &Raw, code: i32) -> Vec<P> {
    let mut v: Vec<P> = Vec::new();
    for (c, val) in &r.g {
        let x = val.trim().parse::<f64>().unwrap_or(0.0);
        if *c == code {
            v.push(p(x, 0.0));
        } else if *c == code + 10 {
            if let Some(l) = v.last_mut() {
                l.y = x;
            }
        }
    }
    v
}

/// スプライン（NURBS）を折れ線に
fn spline_points(r: &Raw) -> Vec<P> {
    let deg_ = r.i(71).max(1) as usize;
    let knots: Vec<f64> = r.g.iter().filter(|(c, _)| *c == 40).filter_map(|(_, v)| v.trim().parse().ok()).collect();
    let ctrl = pts_of(r, 10);
    let w: Vec<f64> = r.g.iter().filter(|(c, _)| *c == 41).filter_map(|(_, v)| v.trim().parse().ok()).collect();
    if ctrl.len() > deg_ && knots.len() == ctrl.len() + deg_ + 1 {
        let wt = |i: usize| if w.len() == ctrl.len() { w[i] } else { 1.0 };
        let (t0, t1) = (knots[deg_], knots[ctrl.len()]);
        let n = (ctrl.len() * 8).clamp(16, 400);
        return (0..=n)
            .map(|k| {
                let t = t0 + (t1 - t0) * k as f64 / n as f64;
                // de Boor
                let mut s = deg_;
                while s + 1 < ctrl.len() && knots[s + 1] <= t {
                    s += 1;
                }
                let mut d: Vec<(f64, f64, f64)> = (0..=deg_).map(|j| {
                    let c = ctrl[j + s - deg_];
                    let ww = wt(j + s - deg_);
                    (c.x * ww, c.y * ww, ww)
                }).collect();
                for rr in 1..=deg_ {
                    for j in (rr..=deg_).rev() {
                        let i = j + s - deg_;
                        let den = knots[i + deg_ + 1 - rr] - knots[i];
                        let al = if den.abs() < 1e-15 { 0.0 } else { (t - knots[i]) / den };
                        d[j] = (
                            (1.0 - al) * d[j - 1].0 + al * d[j].0,
                            (1.0 - al) * d[j - 1].1 + al * d[j].1,
                            (1.0 - al) * d[j - 1].2 + al * d[j].2,
                        );
                    }
                }
                let (x, y, ww) = d[deg_];
                p(x / ww, y / ww)
            })
            .collect();
    }
    // 制御点がない時は通過点
    let fit = pts_of(r, 11);
    if fit.len() >= 2 {
        return crate::edit::spline_pts(&fit, 8);
    }
    ctrl
}

/// TEXT の制御コード
fn text_escapes(s: &str) -> String {
    let mut out = String::new();
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < c.len() {
        if c[i] == '%' && c.get(i + 1) == Some(&'%') && i + 2 < c.len() {
            match c[i + 2].to_ascii_lowercase() {
                'c' => out.push('φ'),
                'd' => out.push('°'),
                'p' => out.push('±'),
                '%' => out.push('%'),
                'u' | 'o' | 'k' => {}
                d if d.is_ascii_digit() => {
                    // %%nnn = 文字コード
                    let mut j = i + 2;
                    let mut n = 0u32;
                    while j < c.len() && j < i + 5 && c[j].is_ascii_digit() {
                        n = n * 10 + c[j].to_digit(10).unwrap();
                        j += 1;
                    }
                    if let Some(ch) = char::from_u32(n) {
                        out.push(ch);
                    }
                    i = j;
                    continue;
                }
                _ => {}
            }
            i += 3;
            continue;
        }
        if c[i] == '\\' && matches!(c.get(i + 1), Some('U') | Some('u')) && c.get(i + 2) == Some(&'+') {
            let hex: String = c[i + 3..(i + 7).min(c.len())].iter().collect();
            if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                out.push(ch);
                i += 7;
                continue;
            }
        }
        if c[i] == '\\' && matches!(c.get(i + 1), Some('M') | Some('m')) && c.get(i + 2) == Some(&'+') && i + 8 <= c.len() {
            // \M+nXXXX：2 バイト文字（Shift-JIS とみなす）
            let hex: String = c[i + 4..i + 8].iter().collect();
            if let Ok(v) = u16::from_str_radix(&hex, 16) {
                let b = v.to_be_bytes();
                out += &encoding_rs::SHIFT_JIS.decode(&b).0;
                i += 8;
                continue;
            }
        }
        out.push(c[i]);
        i += 1;
    }
    out
}

/// MTEXT の書式を外して行に分ける
fn mtext_lines(s: &str) -> Vec<String> {
    let s = text_escapes(s);
    let c: Vec<char> = s.chars().collect();
    let mut lines = vec![String::new()];
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        match ch {
            '\\' if i + 1 < c.len() => {
                let n = c[i + 1];
                match n {
                    'P' => {
                        lines.push(String::new());
                        i += 2;
                    }
                    '~' => {
                        lines.last_mut().unwrap().push(' ');
                        i += 2;
                    }
                    '\\' | '{' | '}' => {
                        lines.last_mut().unwrap().push(n);
                        i += 2;
                    }
                    'L' | 'l' | 'O' | 'o' | 'K' | 'k' | 'N' => i += 2,
                    'S' => {
                        // \Sa^b; → a/b
                        let mut j = i + 2;
                        let mut t = String::new();
                        while j < c.len() && c[j] != ';' {
                            t.push(if c[j] == '^' || c[j] == '#' { '/' } else { c[j] });
                            j += 1;
                        }
                        lines.last_mut().unwrap().push_str(t.trim());
                        i = j + 1;
                    }
                    _ => {
                        // \f…; \H…; \C…; \A…; \W…; \Q…; \T…; など：; まで飛ばす
                        let mut j = i + 2;
                        while j < c.len() && c[j] != ';' {
                            j += 1;
                        }
                        i = j + 1;
                    }
                }
            }
            '{' | '}' => i += 1,
            _ => {
                lines.last_mut().unwrap().push(ch);
                i += 1;
            }
        }
    }
    lines
}

fn shape_pts(s: &Shape) -> Vec<P> {
    match s {
        Shape::Line { a, b } => vec![*a, *b],
        Shape::Arc(x) => vec![p(x.c.x - x.r, x.c.y - x.r), p(x.c.x + x.r, x.c.y + x.r)],
        Shape::Point(x) => vec![x.p],
        Shape::Text(t) => vec![t.start, t.end],
        Shape::Solid { pts, .. } => pts.to_vec(),
        _ => vec![],
    }
}

fn scale_shape(s: Shape, k: f64) -> Shape {
    let sp = |q: P| p(q.x * k, q.y * k);
    match s {
        Shape::Line { a, b } => Shape::Line { a: sp(a), b: sp(b) },
        Shape::Arc(mut x) => {
            x.c = sp(x.c);
            x.r *= k;
            Shape::Arc(x)
        }
        Shape::Point(mut x) => {
            x.p = sp(x.p);
            Shape::Point(x)
        }
        Shape::Text(mut t) => {
            // 文字の大きさは図寸のまま（DXF の高さは実寸なので縮尺で割る）
            t.start = sp(t.start);
            t.end = sp(t.end);
            t.size_x *= k;
            t.size_y *= k;
            t.gap *= k;
            Shape::Text(t)
        }
        Shape::Solid { pts, rgb } => Shape::Solid { pts: pts.map(sp), rgb },
        other => other,
    }
}

fn move_shape(s: Shape, d: P) -> Shape {
    if d.x == 0.0 && d.y == 0.0 {
        return s;
    }
    match s {
        Shape::Line { a, b } => Shape::Line { a: add(a, d), b: add(b, d) },
        Shape::Arc(mut x) => {
            x.c = add(x.c, d);
            Shape::Arc(x)
        }
        Shape::Point(mut x) => {
            x.p = add(x.p, d);
            Shape::Point(x)
        }
        Shape::Text(mut t) => {
            t.start = add(t.start, d);
            t.end = add(t.end, d);
            Shape::Text(t)
        }
        Shape::Solid { pts, rgb } => Shape::Solid { pts: pts.map(|q| add(q, d)), rgb },
        other => other,
    }
}

/// AutoCAD の色番号 → Jw_cad の線色（色相で一番近いもの）
pub fn aci_to_jw(c: i32) -> u16 {
    match c {
        1 => return 8,
        2 => return 4,
        3 => return 3,
        4 => return 1,
        5 => return 6,
        6 => return 5,
        7 | 0 | 256 => return 2,
        8 | 9 | 250..=255 => return 9,
        _ => {}
    }
    if !(10..=249).contains(&c) {
        return 2;
    }
    let hue = ((c / 10 - 1) * 15) as f64;
    // 奇数番は淡い色。どちらも色相で分ける
    match hue {
        h if !(15.0..345.0).contains(&h) => 8,
        h if h < 75.0 => 4,
        h if h < 150.0 => 3,
        h if h < 210.0 => 1,
        h if h < 270.0 => 6,
        _ => 5,
    }
}

/// Jw_cad の線色 → AutoCAD の色番号
pub fn jw_to_aci(c: u16) -> i32 {
    match c {
        1 => 4,
        2 => 7,
        3 => 3,
        4 => 2,
        5 => 6,
        6 => 5,
        7 => 7,
        8 => 1,
        9 => 8,
        _ => 7,
    }
}

// ---------- 書出 ----------

const LTYPES: [(&str, &str, &[f64]); 9] = [
    ("CONTINUOUS", "Solid line", &[]),
    ("DASHED1", "点線1", &[1.0, -1.0]),
    ("DASHED2", "点線2", &[2.0, -1.0]),
    ("DASHED3", "点線3", &[4.0, -1.5]),
    ("CENTER1", "一点鎖1", &[6.0, -1.0, 0.5, -1.0]),
    ("CENTER2", "一点鎖2", &[12.0, -1.5, 1.0, -1.5]),
    ("PHANTOM1", "二点鎖1", &[6.0, -1.0, 0.5, -1.0, 0.5, -1.0]),
    ("PHANTOM2", "二点鎖2", &[12.0, -1.5, 1.0, -1.5, 1.0, -1.5]),
    ("DOT", "補助線", &[0.5, -1.0]),
];

fn ltype_name(style: u8) -> &'static str {
    match style {
        2..=9 => LTYPES[style as usize - 1].0,
        _ => "CONTINUOUS",
    }
}

struct W {
    s: String,
}

impl W {
    fn g(&mut self, code: i32, v: impl std::fmt::Display) {
        self.s += &format!("{code:>3}\r\n{v}\r\n");
    }
    fn f(&mut self, code: i32, v: f64) {
        let v = if v.abs() < 1e-12 { 0.0 } else { v };
        self.g(code, format!("{}", (v * 1e8).round() / 1e8));
    }
    fn pt(&mut self, code: i32, q: P) {
        self.f(code, q.x);
        self.f(code + 10, q.y);
    }
    fn head(&mut self, kind: &str, layer: &str, a: &Attr) {
        self.g(0, kind);
        self.g(8, layer);
        if a.style > 1 {
            self.g(6, ltype_name(a.style));
        }
        self.g(62, jw_to_aci(a.color));
    }
}

/// DXF 用の文字列：Shift-JIS で書けない文字は \U+XXXX
fn dxf_str(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        let mut b = [0u8; 4];
        let (_, _, bad) = encoding_rs::SHIFT_JIS.encode(ch.encode_utf8(&mut b));
        if bad {
            out += &format!("\\U+{:04X}", ch as u32);
        } else {
            out.push(ch);
        }
    }
    out
}

fn layer_name(d: &Doc, g: usize, l: usize) -> String {
    let name = d.glayers.get(g).map(|x| x.lay_name[l].trim().to_string()).unwrap_or_default();
    let name: String = name.chars().map(|c| if "<>/\\\":;?*|,=`".contains(c) { '_' } else { c }).collect();
    // Jw_cad の「レイヤ名に番号を付加する」と同じ形（_グループ-レイヤ_名前）。Jw_cad で読むと元のレイヤに戻る
    format!("_{g:x}-{l:x}_{name}")
}

/// 図面を DXF（R12）に。返り値は Shift-JIS のバイト列
pub fn export(d: &Doc) -> Vec<u8> {
    let mut body = W { s: String::new() };
    let mut bb = crate::geom::BBox::EMPTY;
    let mut ex = Exporter { d, w: &mut body, bb: &mut bb };
    for e in &d.ents {
        let g = (e.a.glayer as usize).min(d.glayers.len().saturating_sub(1));
        let sc = d.glayers.get(g).map_or(1.0, |x| if x.scale > 0.0 { x.scale } else { 1.0 });
        let layer = layer_name(d, g, (e.a.layer as usize).min(15));
        ex.ent(e, &Affine::scale(sc, sc), sc, &layer, None, 0);
    }
    if bb.is_empty() {
        bb.add(p(0.0, 0.0));
    }
    let main_scale = d.glayers.get(d.write_glay as usize).map_or(1.0, |x| if x.scale > 0.0 { x.scale } else { 1.0 });

    let mut w = W { s: String::new() };
    w.g(0, "SECTION");
    w.g(2, "HEADER");
    w.g(9, "$ACADVER");
    w.g(1, "AC1009");
    w.g(9, "$DWGCODEPAGE");
    w.g(3, "ANSI_932");
    w.g(9, "$INSBASE");
    w.pt(10, p(0.0, 0.0));
    w.f(30, 0.0);
    w.g(9, "$EXTMIN");
    w.pt(10, p(bb.x0, bb.y0));
    w.f(30, 0.0);
    w.g(9, "$EXTMAX");
    w.pt(10, p(bb.x1, bb.y1));
    w.f(30, 0.0);
    w.g(9, "$LTSCALE");
    w.f(40, main_scale);
    w.g(0, "ENDSEC");

    w.g(0, "SECTION");
    w.g(2, "TABLES");
    w.g(0, "TABLE");
    w.g(2, "LTYPE");
    w.g(70, LTYPES.len());
    for (name, desc, pat) in LTYPES {
        w.g(0, "LTYPE");
        w.g(2, name);
        w.g(70, 0);
        w.g(3, dxf_str(desc));
        w.g(72, 65);
        w.g(73, pat.len());
        w.f(40, pat.iter().map(|x| x.abs()).sum());
        for x in pat {
            w.f(49, *x);
        }
    }
    w.g(0, "ENDTAB");
    w.g(0, "TABLE");
    w.g(2, "LAYER");
    let mut lays: Vec<(String, bool)> = Vec::new();
    for (g, gl) in d.glayers.iter().enumerate().take(16) {
        for l in 0..16 {
            let used = d.ents.iter().any(|e| e.a.glayer as usize == g && e.a.layer as usize == l);
            if used || !gl.lay_name[l].trim().is_empty() {
                lays.push((layer_name(d, g, l), gl.state == 0 || gl.lay_state[l] == 0));
            }
        }
    }
    w.g(70, lays.len() + 1);
    w.g(0, "LAYER");
    w.g(2, "0");
    w.g(70, 0);
    w.g(62, 7);
    w.g(6, "CONTINUOUS");
    for (name, off) in &lays {
        w.g(0, "LAYER");
        w.g(2, dxf_str(name));
        w.g(70, 0);
        w.g(62, if *off { -7 } else { 7 });
        w.g(6, "CONTINUOUS");
    }
    w.g(0, "ENDTAB");
    w.g(0, "TABLE");
    w.g(2, "STYLE");
    w.g(70, 1);
    w.g(0, "STYLE");
    w.g(2, "STANDARD");
    w.g(70, 0);
    w.f(40, 0.0);
    w.f(41, 1.0);
    w.f(50, 0.0);
    w.g(71, 0);
    w.f(42, 2.5);
    w.g(3, "txt");
    w.g(4, "");
    w.g(0, "ENDTAB");
    w.g(0, "ENDSEC");

    w.g(0, "SECTION");
    w.g(2, "ENTITIES");
    w.s += &body.s;
    w.g(0, "ENDSEC");
    w.g(0, "EOF");
    encoding_rs::SHIFT_JIS.encode(&w.s).0.into_owned()
}

struct Exporter<'a> {
    d: &'a Doc,
    w: &'a mut W,
    bb: &'a mut crate::geom::BBox,
}

impl Exporter<'_> {
    /// m：図寸 → 実寸（ブロックの中はさらに挿入の変換）。sc：文字の大きさに掛ける倍率
    fn ent(&mut self, e: &Ent, m: &Affine, sc: f64, layer: &str, top: Option<&Attr>, depth: u32) {
        let a = top.map_or(e.a.clone(), |t| Attr { color: if e.a.color == 0 { t.color } else { e.a.color }, ..e.a.clone() });
        let layer = dxf_str(layer);
        let det = m.a * m.d - m.b * m.c;
        match &e.shape {
            Shape::Line { a: x, b: y } => {
                let (x, y) = (m.apply(*x), m.apply(*y));
                self.bb.add(x);
                self.bb.add(y);
                self.w.head("LINE", &layer, &a);
                self.w.pt(10, x);
                self.w.pt(11, y);
            }
            Shape::Arc(arc) => {
                let uniform = (m.a.hypot(m.b) - m.c.hypot(m.d)).abs() < 1e-9 * m.max_scale() && (m.a * m.c + m.b * m.d).abs() < 1e-9 * m.max_scale() * m.max_scale();
                if (arc.flat - 1.0).abs() < 1e-9 && uniform {
                    let c = m.apply(arc.c);
                    let k = det.abs().sqrt();
                    let r = arc.r * k;
                    self.bb.add(p(c.x - r, c.y - r));
                    self.bb.add(p(c.x + r, c.y + r));
                    if arc.full || arc.sweep >= TAU - 1e-9 {
                        self.w.head("CIRCLE", &layer, &a);
                        self.w.pt(10, c);
                        self.w.f(40, r);
                    } else {
                        let ps = m.apply(polar(arc.c, arc.r, arc.start));
                        let pe = m.apply(polar(arc.c, arc.r, arc.start + arc.sweep));
                        let (s, e2) = if det >= 0.0 { (ps, pe) } else { (pe, ps) };
                        self.w.head("ARC", &layer, &a);
                        self.w.pt(10, c);
                        self.w.f(40, r);
                        self.w.f(50, deg(norm_ang(angle_of(sub(s, c)))));
                        self.w.f(51, deg(norm_ang(angle_of(sub(e2, c)))));
                    }
                } else {
                    // 楕円は R12 にないので折れ線
                    let (st, sw) = if arc.full { (0.0, TAU) } else { (arc.start, arc.sweep) };
                    let n = ((sw.abs() / TAU * 96.0).ceil() as usize).max(8);
                    let pts: Vec<P> = (0..=n).map(|k| m.apply(crate::geom::ellipse_pt(arc.c, arc.r, arc.flat, arc.tilt, st + sw * k as f64 / n as f64))).collect();
                    for q in &pts {
                        self.bb.add(*q);
                    }
                    self.w.head("POLYLINE", &layer, &a);
                    self.w.g(66, 1);
                    self.w.pt(10, p(0.0, 0.0));
                    self.w.g(70, 0);
                    for q in pts {
                        self.w.g(0, "VERTEX");
                        self.w.g(8, &layer);
                        self.w.pt(10, q);
                    }
                    self.w.g(0, "SEQEND");
                    self.w.g(8, &layer);
                }
            }
            Shape::Point(pt) => {
                if pt.temp {
                    return;
                }
                let q = m.apply(pt.p);
                self.bb.add(q);
                self.w.head("POINT", &layer, &a);
                self.w.pt(10, q);
            }
            Shape::Text(t) => {
                let q = m.apply(t.start);
                let qe = m.apply(t.end);
                self.bb.add(q);
                self.bb.add(qe);
                let k = det.abs().sqrt() / sc.max(1e-12);
                let ang = if dist(q, qe) > 1e-12 { deg(angle_of(sub(qe, q))) } else { t.deg };
                self.w.head("TEXT", &layer, &a);
                self.w.pt(10, q);
                self.w.f(40, t.size_y * sc * k);
                if t.size_y > 1e-12 && (t.size_x / t.size_y - 1.0).abs() > 1e-6 {
                    self.w.f(41, t.size_x / t.size_y);
                }
                if ang.abs() > 1e-9 {
                    self.w.f(50, ang.rem_euclid(360.0));
                }
                self.w.g(1, dxf_str(&t.s));
            }
            Shape::Solid { pts, .. } => {
                if a.style >= 101 {
                    return; // 円ソリッドは書かない
                }
                let q = pts.map(|x| m.apply(x));
                for x in &q {
                    self.bb.add(*x);
                }
                self.w.head("SOLID", &layer, &a);
                self.w.pt(10, q[0]);
                self.w.pt(11, q[1]);
                self.w.pt(12, q[3]);
                self.w.pt(13, q[2]);
            }
            Shape::Dim(dm) => {
                self.ent(&dm.line, m, sc, &layer, Some(&a), depth);
                self.ent(&dm.text, m, sc, &layer, Some(&a), depth);
                if let Some(x) = &dm.sxf {
                    if x.mode != 0 {
                        for l in x.lines.iter() {
                            self.ent(l, m, sc, &layer, Some(&a), depth);
                        }
                        for l in x.points.iter() {
                            self.ent(l, m, sc, &layer, Some(&a), depth);
                        }
                    }
                }
            }
            Shape::Block { base, sx, sy, rot, number } => {
                if depth > 16 {
                    return;
                }
                let Some(b) = self.d.block(*number) else { return };
                let mm = m.then(&Affine::translate(base.x, base.y)).then(&Affine::rotate(*rot)).then(&Affine::scale(*sx, *sy));
                for c in &b.ents {
                    self.ent(c, &mm, sc, &layer, Some(&a), depth + 1);
                }
            }
        }
    }
}

