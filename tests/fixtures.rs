//! 用 Jw_cad 自带的样例图（tests/fixtures）检查：全部能读、能出 PDF、不崩。
//! 改了读写或画图的代码，跑 `cargo test` 就知道有没有把哪张图弄坏。

use jwwview::{draw::{bboxes, Walker}, font::Fonts, jww, pdf};
use std::path::PathBuf;

fn fixtures() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("tests/fixtures が無い")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "jww").unwrap_or(false))
        .collect();
    v.sort();
    assert!(v.len() >= 10, "样例图太少：{}", v.len());
    v
}

#[test]
fn all_fixtures_read() {
    for f in fixtures() {
        let d = jww::read(&f).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert!(!d.ents.is_empty(), "{}: 一个图形都没有", f.display());
        assert_eq!(d.glayers.len(), 16);
    }
}

#[test]
fn all_fixtures_render_pdf() {
    let fonts = Fonts::system();
    for f in fixtures() {
        let d = jww::read(&f).unwrap();
        let w = Walker::new(&d, &fonts);
        let (boxes, _) = bboxes(&w);
        let (pi, center, scale) = pdf::initial_frame(&d, &boxes);
        let p = pdf::PAGES[pi];
        let opt = pdf::PdfOptions { color: true, visible: pdf::file_visibility(&d), page: (p.1, p.2), center, scale };
        let (bytes, drawn) = pdf::render_counted(&w, &opt);
        assert!(bytes.starts_with(b"%PDF-1.4"), "{}", f.display());
        // 打印框里至少要印出东西（框错位置会是空白页）
        assert!(drawn >= d.ents.len().min(5), "{}: 只画了 {drawn} 笔", f.display());
    }
}
