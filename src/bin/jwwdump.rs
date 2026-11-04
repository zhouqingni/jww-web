//! 读一批 .jww，报告每张图读到了什么、哪里读不下去。用法：jwwdump <文件或文件夹>...
use jwwview::jww::{self, Shape};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn collect(p: &Path, out: &mut Vec<PathBuf>) {
    if p.is_dir() {
        if let Ok(rd) = std::fs::read_dir(p) {
            for e in rd.flatten() {
                collect(&e.path(), out);
            }
        }
    } else if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
        let ext = ext.to_ascii_lowercase();
        if ext == "jww" || ext == "jw$" {
            out.push(p.to_path_buf());
        }
    }
}

fn kind(s: &Shape) -> &'static str {
    match s {
        Shape::Line { .. } => "line",
        Shape::Arc(_) => "arc",
        Shape::Point(_) => "point",
        Shape::Text(_) => "text",
        Shape::Dim(_) => "dim",
        Shape::Solid { .. } => "solid",
        Shape::Block { .. } => "block",
    }
}

fn main() {
    let mut files = Vec::new();
    for a in std::env::args().skip(1) {
        collect(Path::new(&a), &mut files);
    }
    let (mut ok, mut ng) = (0, 0);
    let mut versions = BTreeMap::new();
    let show_settings = std::env::var_os("JWW_SETTINGS").is_some();
    let check = std::env::var_os("JWW_CHECK").is_some();
    let (mut tn, mut tbad) = (0, 0);
    for f in &files {
        match jww::read(f) {
            Ok(d) if check => {
                // 文字终点 = 起点 + Σ字宽 + 字间距：半角按横宽一半
                for e in &d.ents {
                    if let Shape::Text(t) = &e.shape {
                        if t.s.starts_with("^@") || t.s.is_empty() || e.a.flag & 0x20 != 0 {
                            continue;
                        }
                        let n = t.s.chars().count() as f64;
                        let cells: f64 = t.s.chars().map(|c| if jwwview::jww::is_half(c) { 0.5 } else { 1.0 }).sum();
                        let last = if jwwview::jww::is_half(t.s.chars().last().unwrap()) { 0.5 } else { 1.0 }; let pred = cells * (t.size_x + t.gap) - last * t.gap; let _ = n;
                        let real = ((t.end.x - t.start.x).powi(2) + (t.end.y - t.start.y).powi(2)).sqrt();
                        tn += 1;
                        if (pred - real).abs() > 0.01 * real.max(1.0) {
                            tbad += 1;
                            if tbad <= 8 {
                                println!("text {:?} sx={} gap={} pred={pred:.3} real={real:.3}", t.s, t.size_x, t.gap);
                            }
                        }
                    }
                }
                for b in &d.blocks {
                    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                    for e in &b.ents {
                        if let Shape::Line { a, b } = &e.shape {
                            for p in [a, b] {
                                x0 = x0.min(p.x); y0 = y0.min(p.y); x1 = x1.max(p.x); y1 = y1.max(p.y);
                            }
                        }
                    }
                    let inst: Vec<_> = d.ents.iter().filter_map(|e| match &e.shape {
                        Shape::Block { number, sx, base, .. } if *number == b.number =>
                            Some((e.a.glayer, d.glayers[e.a.glayer as usize].scale, *sx, base.x, base.y)),
                        _ => None }).take(2).collect();
                    println!("block {:?} def-glayer={} bbox=({x0:.1},{y0:.1})-({x1:.1},{y1:.1}) inst={inst:?}", b.name, b.a.glayer);
                }
            }
            Ok(d) if show_settings => {
                println!("{}  paper={} max_draw_wid={} dpi={} color_print={} prt_origin={:?} prt_scale={} prt_set={}", f.display(), d.paper_name(), d.max_draw_wid, d.prt_dpi, d.color_print, d.prt_origin, d.prt_scale, d.prt_set);
                for (i, p) in d.pens.iter().enumerate().take(10) {
                    println!("  pen{i}: {:06x} w{}  prt {:06x} w{} r{}", p.color, p.width, p.prt_color, p.prt_width, p.prt_point_r);
                }
                for (i, t) in d.ltypes.iter().enumerate().take(20) {
                    if t.pattern != 0 {
                        println!("  lt{i}: {:032b} unit{} pitch{} prt{} rand{}/{}", t.pattern, t.unit_dot, t.pitch, t.prt_pitch, t.rand_wide, t.prt_rand_wide);
                    }
                }
                for (i, m) in d.moji.iter().enumerate().skip(1) {
                    println!("  moji{i}: {}x{} gap{} col{}", m.w, m.h, m.gap, m.color);
                }
                let mut fonts = BTreeMap::new();
                let mut styles = BTreeMap::new();
                for e in &d.ents {
                    if let Shape::Text(t) = &e.shape {
                        *fonts.entry(t.font.clone()).or_insert(0) += 1;
                    }
                    if let Shape::Line { .. } = &e.shape {
                        *styles.entry((e.a.style, e.a.color, e.a.width)).or_insert(0) += 1;
                    }
                }
                println!("  fonts {fonts:?}");
                println!("  line (style,color,width) {styles:?}");
            }
            Ok(d) => {
                ok += 1;
                *versions.entry(d.version).or_insert(0) += 1;
                let mut c: BTreeMap<&str, usize> = BTreeMap::new();
                for e in &d.ents {
                    *c.entry(kind(&e.shape)).or_default() += 1;
                }
                let inner: usize = d.blocks.iter().map(|b| b.ents.len()).sum();
                println!(
                    "OK  v{} {} ents={} {:?} blocks={}({}) images={}  {}",
                    d.version,
                    d.paper_name(),
                    d.ents.len(),
                    c,
                    d.blocks.len(),
                    inner,
                    d.images.len(),
                    f.display()
                );
            }
            Err(e) => {
                ng += 1;
                println!("NG  {}  {}", e, f.display());
            }
        }
    }
    if check {
        println!("text length check: {tbad} / {tn} off");
    }
    println!("---- {ok} OK, {ng} NG, versions {versions:?}");
}
