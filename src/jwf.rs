//! Jw_cad 的环境设定文件（.jwf）：公司的作图习惯和快捷键。
//!
//! 用到的部分：图层组名・缩尺（LAYNAM_x 第一项「S=1/100」）、图层名、
//! 图层别线色・线种（LAYCOL_x / LAYTYP_x，换写入图层时线属性跟着换，0 = 不换）、
//! 文字种（MWIDE / MHIGH / MDIST / MPEN）、画面色・打印色（LCOLLOR_n / PCOLLOR_n）、画面背景色（LCOLLOR_B）、
//! 快捷键（KEY_A〜Z：直接按 / Shift，KEYF2〜F9，KEYSP = 空格）、
//! 时钟菜单（LD_AM / LD_PM / RD_AM / RD_PM = 左・右拖动的 AM・PM 0〜11 时，LD2_ 等 = 拖得远时的(2)）。
//! 命令号照 Jw_cad 基本设定 KEY 页的「コマンド一覧」。

use crate::jww::Doc;

/// 默认：公司的「1カラー書き込み用zhou2.jwf」
pub const COMPANY: &[u8] = include_bytes!("../assets/company.jwf");

#[derive(Clone, Debug)]
pub struct Env {
    pub name: String,
    pub glayer_names: [String; 16],
    pub scales: [f64; 16],
    pub lay_names: [[String; 16]; 16],
    /// 图层别线色・线种（0 = 不换）
    pub lay_color: [[u16; 16]; 16],
    pub lay_style: [[u8; 16]; 16],
    /// A〜Z：（直接按, Shift+键）的命令号，0 = 无
    pub keys: [(u32, u32); 26],
    /// F2〜F9
    pub fkeys: [u32; 8],
    pub space: u32,
    /// 文字种 1〜10：（幅, 高, 间隔, 色）
    pub moji: [(f64, f64, f64, u32); 10],
    /// 线色 1〜8：画面（RGB, 线宽）、打印（RGB, 线宽, 点半径）
    pub screen: [Option<(u32, u32)>; 8],
    pub print: [Option<(u32, u32, f64)>; 8],
    /// 画面の背景色（LCOLLOR_B）
    pub background: Option<u32>,
    /// 时钟菜单的命令号：[左/右][AM/PM][0〜11 时]，clock2 = 拖得远时的(2)。0 = 标准功能
    pub clock: [[[i32; 12]; 2]; 2],
    pub clock2: [[[i32; 12]; 2]; 2],
}

fn rgb(r: f64, g: f64, b: f64) -> u32 {
    // COLORREF 0x00BBGGRR
    (r as u32 & 255) | (g as u32 & 255) << 8 | (b as u32 & 255) << 16
}

fn hex_idx(c: char) -> Option<usize> {
    c.to_digit(16).map(|d| d as usize)
}

/// 「LD_AM」「RD2_PM」→（0 = 左 / 1 = 右, 0 = AM / 1 = PM, (2) か）
fn clock_key(k: &str) -> Option<(usize, usize, bool)> {
    let (lr, rest) = if let Some(r) = k.strip_prefix("LD") { (0, r) } else { (1, k.strip_prefix("RD")?) };
    let (far, rest) = match rest.strip_prefix('2') {
        Some(r) => (true, r),
        None => (false, rest),
    };
    let pm = match rest {
        "_AM" => 0,
        "_PM" => 1,
        _ => return None,
    };
    Some((lr, pm, far))
}

fn trim_name(s: &str) -> String {
    s.trim_matches(|c: char| c.is_whitespace() || c == '　').to_string()
}

impl Env {
    pub fn company() -> Env {
        Env::parse(COMPANY, "1カラー書き込み用zhou2.jwf")
    }

    pub fn parse(bytes: &[u8], name: &str) -> Env {
        let (text, _, _) = encoding_rs::SHIFT_JIS.decode(bytes);
        let mut e = Env {
            name: name.to_string(),
            glayer_names: Default::default(),
            scales: [0.0; 16],
            lay_names: Default::default(),
            lay_color: [[0; 16]; 16],
            lay_style: [[0; 16]; 16],
            keys: [(0, 0); 26],
            fkeys: [0; 8],
            space: 0,
            moji: [(0.0, 0.0, 0.0, 0); 10],
            screen: [None; 8],
            print: [None; 8],
            background: None,
            clock: [[[0; 12]; 2]; 2],
            clock2: [[[0; 12]; 2]; 2],
        };
        let (mut mw, mut mh, mut md, mut mp) = (vec![], vec![], vec![], vec![]);
        for line in text.lines() {
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let k = k.trim();
            let nums: Vec<f64> = v.split_whitespace().filter_map(|t| t.parse().ok()).collect();
            let n = |i: usize| nums.get(i).copied().unwrap_or(0.0);
            if let Some(g) = k.strip_prefix("LAYNAM_").and_then(|s| s.chars().next()).and_then(hex_idx) {
                let items: Vec<&str> = v.split(',').collect();
                let gname = trim_name(items.first().copied().unwrap_or(""));
                if let Some(d) = gname.strip_prefix("S=1/").and_then(|s| s.trim().parse::<f64>().ok()) {
                    e.scales[g] = d;
                }
                e.glayer_names[g] = gname;
                for l in 0..16 {
                    e.lay_names[g][l] = trim_name(items.get(l + 1).copied().unwrap_or(""));
                }
            } else if let Some(g) = k.strip_prefix("LAYCOL_").and_then(|s| s.chars().next()).and_then(hex_idx) {
                for l in 0..16 {
                    e.lay_color[g][l] = n(l) as u16;
                }
            } else if let Some(g) = k.strip_prefix("LAYTYP_").and_then(|s| s.chars().next()).and_then(hex_idx) {
                for l in 0..16 {
                    e.lay_style[g][l] = n(l) as u8;
                }
            } else if let Some(c) = k.strip_prefix("KEY_").filter(|s| s.len() == 1).and_then(|s| s.chars().next()) {
                if c.is_ascii_uppercase() {
                    e.keys[(c as u8 - b'A') as usize] = (n(0) as u32, n(1) as u32);
                }
            } else if let Some(f) = k.strip_prefix("KEYF").and_then(|s| s.parse::<usize>().ok()) {
                if (2..=9).contains(&f) {
                    e.fkeys[f - 2] = n(0) as u32;
                }
            } else if k == "KEYSP" {
                e.space = n(0) as u32;
            } else if k == "LCOLLOR_B" && nums.len() >= 3 {
                e.background = Some(rgb(n(0), n(1), n(2)));
            } else if let Some(c) = k.strip_prefix("LCOLLOR_").and_then(|s| s.parse::<usize>().ok()) {
                if (1..=8).contains(&c) && nums.len() >= 4 {
                    e.screen[c - 1] = Some((rgb(n(0), n(1), n(2)), n(3) as u32));
                }
            } else if let Some(c) = k.strip_prefix("PCOLLOR_").and_then(|s| s.parse::<usize>().ok()) {
                if (1..=8).contains(&c) && nums.len() >= 5 {
                    e.print[c - 1] = Some((rgb(n(0), n(1), n(2)), n(3) as u32, n(4)));
                }
            } else if let Some((lr, pm, far)) = clock_key(k) {
                let t = if far { &mut e.clock2 } else { &mut e.clock };
                for h in 0..12 {
                    t[lr][pm][h] = n(h) as i32;
                }
            } else {
                match k {
                    "MWIDE" => mw = nums,
                    "MHIGH" => mh = nums,
                    "MDIST" => md = nums,
                    "MPEN" => mp = nums,
                    _ => {}
                }
            }
        }
        for i in 0..10 {
            if let (Some(&w), Some(&h)) = (mw.get(i), mh.get(i)) {
                e.moji[i] = (w, h, md.get(i).copied().unwrap_or(0.0), mp.get(i).copied().unwrap_or(1.0) as u32);
            }
        }
        e
    }

    /// 新图纸套用：图层组名・缩尺、图层名、文字种、线色
    pub fn apply_new(&self, d: &mut Doc) {
        for g in 0..16.min(d.glayers.len()) {
            let gl = &mut d.glayers[g];
            if !self.glayer_names[g].is_empty() {
                gl.name = self.glayer_names[g].clone();
            }
            if self.scales[g] > 0.0 {
                gl.scale = self.scales[g];
            }
            for l in 0..16 {
                gl.lay_name[l] = self.lay_names[g][l].clone();
            }
        }
        for i in 0..10 {
            let (w, h, gap, c) = self.moji[i];
            if w > 0.0 && h > 0.0 {
                let m = &mut d.moji[i + 1];
                m.w = w;
                m.h = h;
                m.gap = gap;
                m.color = c;
            }
        }
        if let (Some(bg), Some(pen)) = (self.background, d.pens.get_mut(0)) {
            pen.color = bg;
        }
        for c in 0..8 {
            let Some(pen) = d.pens.get_mut(c + 1) else { continue };
            if let Some((col, w)) = self.screen[c] {
                pen.color = col;
                pen.width = w;
            }
            if let Some((col, w, r)) = self.print[c] {
                pen.prt_color = col;
                pen.prt_width = w;
                pen.prt_point_r = r;
            }
        }
    }

    /// 图层别线色・线种（换写入图层时用）
    pub fn layer_pens(&self) -> [[(u16, u8); 16]; 16] {
        let mut v = [[(0, 0); 16]; 16];
        for g in 0..16 {
            for l in 0..16 {
                v[g][l] = (self.lay_color[g][l], self.lay_style[g][l]);
            }
        }
        v
    }
}
