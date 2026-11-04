//! 記変（線記号変形）：Jw_cad 付属の JW_OPT4.DAT（建築 1）

use jwwview::cad::*;
use jwwview::edit::Editor;
use jwwview::jww::{Shape, P};
use jwwview::symbol;
use jwwview::tools::{Kind, Tools};

fn file() -> symbol::SymFile {
    symbol::read(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/JW_OPT4.DAT")).unwrap()
}
fn idx(f: &symbol::SymFile, name: &str) -> usize {
    f.syms.iter().position(|s| s.name.starts_with(name)).unwrap_or_else(|| panic!("{name} が無い"))
}
fn texts(ed: &Editor) -> Vec<String> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Text(t) = &e.shape { Some(t.s.clone()) } else { None }).collect()
}

#[test]
fn read_symbol_file() {
    let f = file();
    assert_eq!(f.name, "建築 1");
    assert_eq!(f.syms.len(), 33);
    let s = &f.syms[0];
    assert_eq!((s.name.as_str(), s.count, s.needs_line), ("方位(40mm)", 0, true));
    // 08（表示用）・10 09（指示線を消すだけ）は作図しない
    assert_eq!(s.items.len(), 7);
    let k = &f.syms[idx(&f, "線端に1/4円弧")];
    assert_eq!(k.mag, 0.5);
}

#[test]
fn place_symbol_with_input() {
    let f = file();
    let s = &f.syms[idx(&f, "建具記号(AW)")];
    assert!(!s.needs_line);
    let mut ed = Editor::blank();
    ed.place_symbol(s, p(100.0, 100.0), 0.0, 1.0, None, &["3".to_string()]);
    let t = texts(&ed);
    assert!(t.contains(&"ＡＷ".to_string()) && t.contains(&"3".to_string()), "{t:?}");
    let arc = ed.doc.ents.iter().find_map(|e| if let Shape::Arc(a) = &e.shape { Some(a.clone()) } else { None }).unwrap();
    assert_eq!(arc.c, p(100.0, 100.0));
    assert!((arc.r - 4.0).abs() < 1e-9 && arc.full);
    // 文字が無ければ INPUT は作図しない
    let mut ed = Editor::blank();
    ed.place_symbol(s, p(0.0, 0.0), 0.0, 2.0, None, &[]);
    assert_eq!(texts(&ed), vec!["ＡＷ".to_string()]);
}

#[test]
fn symbol_on_line_by_clicks() {
    let f = file();
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(0.0, 100.0)); // 縦線
    let mut t = Tools::default();
    t.set(Kind::Symbol, &mut ed);
    t.sym = Some(f.clone());
    // 指示回数 1：線を指示 → 位置
    t.sym_idx = idx(&f, "幅 [1mm]");
    t.click(&mut ed, p(0.2, 50.0), false, None, 0.5);
    t.click(&mut ed, p(3.0, 40.0), false, None, 0.5);
    let ls: Vec<(P, P)> = ed.doc.ents.iter().skip(1).filter_map(|e| if let Shape::Line { a, b } = e.shape { Some((a, b)) } else { None }).collect();
    assert_eq!(ls.len(), 3);
    // 線の上（x=0）に、縦向きに置かれる：中央の斜線 (-.5,1)-(.5,-1) → 90° 回転
    assert!(ls.iter().any(|(a, b)| dist(*a, p(-1.0, 39.5)) < 1e-9 && dist(*b, p(1.0, 40.5)) < 1e-9), "{ls:?}");
    // 指示回数 0：線端に作図（方位）
    t.sym_idx = idx(&f, "方位(40mm)");
    t.click(&mut ed, p(0.1, 95.0), false, None, 0.5);
    assert!(t.note.as_deref().unwrap_or("").contains("方位"));
}
