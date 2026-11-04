//! DXF の読み書き

use jwwview::cad::*;
use jwwview::dxf;
use jwwview::edit::Editor;
use jwwview::jww::{Shape, P};

fn dxf_text(ents: &str, tables: &str, blocks: &str) -> Vec<u8> {
    let s = format!(
        "0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1015\n9\n$DWGCODEPAGE\n3\nANSI_932\n0\nENDSEC\n\
         0\nSECTION\n2\nTABLES\n{tables}0\nENDSEC\n0\nSECTION\n2\nBLOCKS\n{blocks}0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n{ents}0\nENDSEC\n0\nEOF\n"
    );
    encoding_rs::SHIFT_JIS.encode(&s).0.into_owned()
}

fn import(b: &[u8]) -> (Editor, dxf::Report) {
    let mut ed = Editor::blank();
    let r = dxf::import(b, &mut ed.doc).expect("読めない");
    (ed, r)
}

fn near(a: P, b: P) -> bool {
    dist(a, b) < 1e-6
}

#[test]
fn lines_arcs_scale_and_layers() {
    // 実寸 20000 × 10000 → A3 に入る 1/100
    let b = dxf_text(
        "0\nLINE\n8\n壁\n62\n1\n10\n0\n20\n0\n11\n20000\n21\n0\n\
         0\nARC\n8\n壁\n10\n0\n20\n0\n40\n10000\n50\n0\n51\n90\n\
         0\nCIRCLE\n8\n0-3_通り芯\n6\nCENTER\n10\n5000\n20\n5000\n40\n500\n",
        "0\nTABLE\n2\nLAYER\n0\nLAYER\n2\n壁\n62\n3\n6\nCONTINUOUS\n0\nENDTAB\n",
        "",
    );
    let (ed, r) = import(&b);
    assert_eq!(r.scale, 100.0);
    assert_eq!(r.count, 3);
    let e = &ed.doc.ents;
    match e[0].shape {
        Shape::Line { a, b } => assert!(near(a, p(0.0, 0.0)) && near(b, p(200.0, 0.0))),
        _ => panic!(),
    }
    assert_eq!(e[0].a.color, 8); // 赤
    match &e[1].shape {
        Shape::Arc(x) => assert!((x.r - 100.0).abs() < 1e-9 && x.start.abs() < 1e-9 && (x.sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-9),
        _ => panic!(),
    }
    assert_eq!(e[1].a.color, 3); // 画層の色（緑）
    // 「0-3_通り芯」はレイヤ 0-3、それ以外は空いているところ
    assert_eq!((e[2].a.glayer, e[2].a.layer), (0, 3));
    assert_eq!(e[2].a.style, 5); // 一点鎖
    assert_eq!(ed.doc.glayers[0].lay_name[3], "通り芯");
    assert_eq!((e[0].a.glayer, e[0].a.layer), (0, 0));
    assert_eq!(ed.doc.glayers[0].lay_name[0], "壁");
    assert!(ed.doc.glayers.iter().all(|g| g.scale == 100.0));
}

#[test]
fn polyline_bulge_and_block_mirror() {
    // ふくらみ 1 = 半円
    let b = dxf_text(
        "0\nLWPOLYLINE\n8\nA\n90\n2\n70\n0\n10\n0\n20\n0\n42\n1\n10\n100\n20\n0\n\
         0\nINSERT\n8\nB\n2\nBK\n10\n500\n20\n0\n41\n-1\n42\n1\n",
        "",
        "0\nBLOCK\n8\n0\n2\nBK\n70\n0\n10\n0\n20\n0\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n0\nARC\n8\n0\n10\n0\n20\n0\n40\n10\n50\n0\n51\n90\n0\nENDBLK\n",
    );
    let (ed, r) = import(&b);
    assert_eq!(r.scale, 2.0);
    let e = &ed.doc.ents;
    match &e[0].shape {
        Shape::Arc(x) => {
            assert!(near(x.c, p(25.0, 0.0)) && (x.r - 25.0).abs() < 1e-9);
            assert!((x.sweep - std::f64::consts::PI).abs() < 1e-9);
        }
        _ => panic!(),
    }
    // 鏡映したブロック：線は左へ、円弧は第 2 象限（90°〜180°）
    match e[1].shape {
        Shape::Line { a, b } => assert!(near(a, p(250.0, 0.0)) && near(b, p(245.0, 0.0))),
        _ => panic!(),
    }
    match &e[2].shape {
        Shape::Arc(x) => {
            assert!(near(x.c, p(250.0, 0.0)));
            assert!((x.start - std::f64::consts::FRAC_PI_2).abs() < 1e-9, "{}", x.start);
            assert!((x.sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        }
        _ => panic!(),
    }
    // ブロックの中の画層 0 は挿入の画層になる
    assert_eq!(e[1].a.layer, e[2].a.layer);
    assert_ne!(e[0].a.layer, e[1].a.layer);
}

#[test]
fn texts() {
    let b = dxf_text(
        "0\nTEXT\n8\nT\n10\n0\n20\n0\n40\n250\n1\n%%c100 寸法\n\
         0\nTEXT\n8\nT\n10\n0\n20\n0\n11\n1000\n21\n500\n40\n250\n72\n2\n1\nAB\n\
         0\nMTEXT\n8\nT\n10\n0\n20\n3000\n40\n250\n71\n1\n1\n{\\fMS Gothic|b0;一行目}\\P二行目\n",
        "",
        "",
    );
    let (ed, r) = import(&b);
    let ts: Vec<(String, P, f64)> = ed
        .doc
        .ents
        .iter()
        .filter_map(|e| match &e.shape {
            Shape::Text(t) => Some((t.s.clone(), t.start, t.size_y)),
            _ => None,
        })
        .collect();
    let k = 1.0 / r.scale;
    assert_eq!(ts[0].0, "φ100 寸法");
    assert!((ts[0].2 - 250.0 * k).abs() < 1e-9);
    // 右寄せ：終点が合わせ点
    let t1 = ed.doc.ents.iter().filter_map(|e| if let Shape::Text(t) = &e.shape { Some(t.clone()) } else { None }).nth(1).unwrap();
    assert_eq!(t1.s, "AB");
    assert!(near(t1.end, p(1000.0 * k, 500.0 * k)), "{:?}", t1.end);
    assert_eq!(ts[2].0, "一行目");
    assert_eq!(ts[3].0, "二行目");
    assert!(ts[2].1.y > ts[3].1.y);
    // 左上合わせ：一行目の上端が挿入点
    assert!((ts[2].1.y + 250.0 * k - 3000.0 * k).abs() < 1e-9);
}

#[test]
fn export_then_import() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(100.0, 0.0));
    ed.arc(p(0.0, 0.0), 50.0, 0.0, 1.0);
    ed.text(p(10.0, 10.0), "テスト①");
    ed.doc.ents[0].a.color = 8;
    ed.doc.ents[0].a.style = 5;
    ed.doc.glayers[0].lay_name[0] = "壁".into();
    let b = dxf::export(&ed.doc);
    let s = encoding_rs::SHIFT_JIS.decode(&b).0.into_owned();
    assert!(s.contains("AC1009") && s.contains("_0-0_壁") && s.contains("CENTER1"));
    // 実寸（1/100）で書いている
    assert!(s.contains("10000"));
    let (back, r) = import(&b);
    assert_eq!(r.count, 3);
    assert_eq!(back.doc.glayers[0].lay_name[0], "壁");
    let e = &back.doc.ents;
    assert_eq!((e[0].a.color, e[0].a.style), (8, 5));
    // 縮尺は図面の大きさで決まるので、実寸で比べる
    let k = r.scale / 100.0;
    match e[0].shape {
        Shape::Line { a, b } => assert!(near(mul(a, k), p(0.0, 0.0)) && near(mul(b, k), p(100.0, 0.0))),
        _ => panic!(),
    }
    match &e[1].shape {
        Shape::Arc(x) => assert!((x.r * k - 50.0).abs() < 1e-6 && (x.sweep - 1.0).abs() < 1e-6),
        _ => panic!(),
    }
    match &e[2].shape {
        Shape::Text(t) => assert_eq!(t.s, "テスト①"),
        _ => panic!(),
    }
}

#[test]
fn not_dxf() {
    let mut ed = Editor::blank();
    assert!(dxf::import(b"hello", &mut ed.doc).is_err());
    assert!(dxf::import(b"AutoCAD Binary DXF\r\n\x1a\0", &mut ed.doc).is_err());
}

/// 手元の DXF を全部読む：DXF_DIR=… cargo test --release --test dxf -- --ignored
#[test]
#[ignore]
fn all_local_dxf() {
    let Some(dir) = std::env::var_os("DXF_DIR") else { return };
    let mut files = Vec::new();
    let mut stack = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().map_or(false, |x| x.eq_ignore_ascii_case("dxf")) {
                files.push(p);
            }
        }
    }
    let mut bad = 0;
    for f in &files {
        let b = std::fs::read(f).unwrap();
        let t = std::time::Instant::now();
        let mut ed = Editor::blank();
        match dxf::import(&b, &mut ed.doc) {
            Ok(r) => {
                let ms = t.elapsed().as_millis();
                let back = jwwview::jww::read_bytes(&jwwview::jwwsave::write_bytes(&ed.doc)).expect("jww に保存できない");
                assert_eq!(back.ents.len(), ed.doc.ents.len());
                if r.count == 0 || ms > 2000 || !r.skipped.is_empty() {
                    println!("{} : {} ({ms} ms)", f.display(), r.message());
                }
            }
            Err(e) => {
                bad += 1;
                println!("{} : ERR {e}", f.display());
            }
        }
    }
    println!("{} files, {bad} errors", files.len());
}
