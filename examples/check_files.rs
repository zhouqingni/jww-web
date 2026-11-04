//! 生成要在 Windows 的 Jw_cad 里打开确认的 .jww（docs/check/）。
//! cargo run --example check_files
use jwwview::cad::p;
use jwwview::edit::Editor;
use jwwview::fill::FillColor;
use jwwview::{jww, jwwsave};
use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let out = root.join("docs/check");
    std::fs::create_dir_all(&out).unwrap();
    // 1. 练习题全部（测试 tests/exercise.rs 先跑一遍会生成）
    let ex = root.join("target/exercise.jww");
    if ex.exists() {
        std::fs::copy(&ex, out.join("1_練習問題.jww")).unwrap();
    } else {
        eprintln!("先跑 cargo test --test exercise");
    }
    // 2. Jw_cad 样例图打开后加几笔再存（改已有的图）
    let d = jww::read(&root.join("tests/fixtures/sample.jww")).unwrap();
    let mut ed = Editor::new(d);
    ed.set_write(0, 0xA);
    ed.pen.color = 6;
    ed.begin();
    ed.rect(p(-150.0, 95.0), p(-110.0, 120.0));
    ed.circle(p(-130.0, 107.5), 800.0);
    ed.text(p(-148.0, 125.0), "JWW Viewer で追記");
    jwwsave::save(&ed.doc, &out.join("2_サンプル追記.jww")).unwrap();
    // 3. 新規図面：レイヤ名・縮尺 1/50
    let mut ed = Editor::blank();
    ed.doc.glayers[0].name = "平面図".into();
    ed.doc.glayers[0].lay_name[1] = "壁".into();
    ed.doc.glayers[0].scale = 50.0;
    ed.set_write(0, 1);
    ed.begin();
    ed.rect_size(p(0.0, 0.0), 3640.0, 2730.0, (0, 0));
    ed.two_lines(0, 60.0, 60.0);
    jwwsave::save(&ed.doc, &out.join("3_新規_縮尺50.jww")).unwrap();
    // 4. F 组：塗り（線色・任意色、凹形）、ハッチ（45°・クロス）、整理
    let mut ed = Editor::blank();
    ed.set_write(0, 1);
    ed.begin();
    let l = [p(0.0, 0.0), p(60.0, 0.0), p(60.0, 30.0), p(30.0, 30.0), p(30.0, 60.0), p(0.0, 60.0)];
    for k in 0..l.len() {
        ed.line(l[k], l[(k + 1) % l.len()]);
    }
    ed.pen.color = 3;
    ed.solid_poly(&l, FillColor::Pen);
    let sq = [p(80.0, 0.0), p(120.0, 0.0), p(120.0, 40.0), p(80.0, 40.0)];
    ed.solid_poly(&sq, FillColor::Rgb(0x00c0c0c0));
    ed.set_write(0, 2);
    ed.pen.color = 1;
    let h1 = [p(0.0, 80.0), p(60.0, 80.0), p(60.0, 120.0), p(0.0, 120.0)];
    ed.hatch(&h1, 45.0, 300.0, false);
    let h2 = [p(80.0, 80.0), p(120.0, 80.0), p(100.0, 120.0)];
    ed.hatch(&h2, 0.0, 300.0, true);
    // 整理：重叠的线连成一条
    ed.line(p(0.0, -20.0), p(40.0, -20.0));
    ed.line(p(30.0, -20.0), p(120.0, -20.0));
    ed.tidy();
    ed.text(p(0.0, -30.0), "塗り・ハッチ・整理（JWW Viewer）");
    jwwsave::save(&ed.doc, &out.join("4_塗り_ハッチ.jww")).unwrap();
    // 5. 図形・曲線・パラメ・角度/円周寸法（1/100）
    let mut ed = Editor::blank();
    ed.set_write(0, 1);
    ed.begin();
    let fig = jww::read_jws(&root.join("tests/fixtures/ev9.jws")).unwrap();
    ed.place_figure(&fig, p(-60.0, 40.0), 1.0, 0.0);
    let fr = jww::read_jws(&root.join("tests/fixtures/fridge60.jws")).unwrap();
    ed.place_figure(&fr, p(-20.0, 40.0), 1.0, 90.0);
    ed.spline(&[p(0.0, 0.0), p(20.0, 20.0), p(40.0, 0.0), p(60.0, 20.0)], 10);
    ed.rect(p(80.0, 0.0), p(100.0, 20.0));
    ed.parametric(p(95.0, -1.0), p(101.0, 21.0), p(20.0, 0.0));
    ed.line(p(0.0, -40.0), p(30.0, -40.0));
    ed.line(p(0.0, -40.0), p(30.0, -10.0));
    ed.dim_angle(p(0.0, -40.0), p(30.0, -40.0), p(30.0, -10.0), p(20.0, -35.0));
    let i = ed.arc(p(70.0, -40.0), 20.0, 0.0, std::f64::consts::PI);
    ed.dim_arclen(i, p(70.0, -15.0));
    ed.text(p(-60.0, -60.0), "図形・曲線・パラメ・角度/円周寸法（JWW Viewer）");
    jwwsave::save(&ed.doc, &out.join("5_図形_曲線_寸法.jww")).unwrap();
    // 7. 記変：建築 1 の記号をいくつか
    let sf = jwwview::symbol::read(&root.join("tests/fixtures/JW_OPT4.DAT")).unwrap();
    let mut ed = Editor::blank();
    ed.begin();
    let mut x = 0.0f64;
    for (k, s) in sf.syms.iter().enumerate().filter(|(_, s)| !s.items.is_empty()) {
        let (cx, cy) = (x % 200.0, -((x / 200.0).floor()) * 40.0);
        if s.needs_line {
            let i = ed.line(p(cx - 15.0, cy), p(cx + 15.0, cy));
            let a = ed.doc.ents[i].a.clone();
            let o = if s.count == 0 { p(cx - 15.0, cy) } else { p(cx, cy) };
            ed.place_symbol(s, o, 0.0, 1.0, Some(&a), &["12".to_string(), "34".to_string()]);
        } else {
            ed.place_symbol(s, p(cx, cy), 0.0, 1.0, None, &["12".to_string(), "34".to_string()]);
        }
        ed.text(p(cx - 15.0, cy - 14.0), &format!("{k}:{}", s.name));
        x += 40.0;
    }
    jwwsave::save(&ed.doc, &out.join("7_記変.jww")).unwrap();
    // 6. 図登：存出来的 .jws（Jw_cad 的「図形」命令能读就算过）
    let mut ed = Editor::blank();
    ed.rect(p(0.0, 0.0), p(20.0, 10.0));
    ed.circle(p(10.0, 5.0), 300.0);
    ed.text(p(2.0, 2.0), "JWW Viewer 図登");
    let idx: Vec<usize> = (0..ed.doc.ents.len()).collect();
    std::fs::write(out.join("6_図登テスト.jws"), ed.figure_bytes(&idx, p(0.0, 0.0))).unwrap();
    for e in std::fs::read_dir(&out).unwrap().flatten() {
        let p = e.path();
        if p.extension().map(|x| x == "bak").unwrap_or(false) {
            std::fs::remove_file(p).unwrap();
        }
    }
    println!("{}", out.display());
}
