//! 存档的根本：tests/fixtures 的每张图「读 → 写」必须和原文件字节完全一致。
//! 这个测试不过，就说明存档会改坏图纸。

use jwwview::{jww, jwwsave};
use std::path::PathBuf;

fn fixtures() -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e == "jww").unwrap_or(false))
        .collect();
    v.sort();
    v
}

#[test]
fn read_write_identical() {
    let mut n = 0;
    for f in fixtures() {
        let orig = std::fs::read(&f).unwrap();
        let d = jww::read_bytes(&orig).unwrap();
        let out = jwwsave::write_bytes(&d);
        if out != orig {
            let at = out.iter().zip(&orig).position(|(a, b)| a != b).unwrap_or(out.len().min(orig.len()));
            panic!("{}: 写出 {} 字节 / 原 {} 字节，第 {at} 字节起不同", f.display(), out.len(), orig.len());
        }
        n += 1;
    }
    assert!(n >= 15, "样例图只有 {n} 张");
}

/// 样例图里没有块（ブロック），自己造一个：块定义 + 块插入，写 → 读 → 写 一致
#[test]
fn blocks_roundtrip() {
    let f = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/site.jww");
    let mut d = jww::read(&f).unwrap();
    let inner: Vec<jww::Ent> = d.ents.iter().take(3).cloned().collect();
    let a = d.ents[0].a.clone();
    d.blocks.push(jww::BlockDef { a: a.clone(), number: 1, referred: true, time: 1_700_000_000, name: "テスト".into(), ents: inner });
    d.ents.push(jww::Ent { a, shape: jww::Shape::Block { base: jww::P { x: 1.0, y: 2.0 }, sx: 1.0, sy: 1.0, rot: 0.0, number: 1 } });
    let b1 = jwwsave::write_bytes(&d);
    let d2 = jww::read_bytes(&b1).unwrap();
    assert_eq!(d2.blocks.len(), 1);
    assert_eq!(d2.blocks[0].name, "テスト");
    assert_eq!(d2.blocks[0].ents.len(), 3);
    assert_eq!(jwwsave::write_bytes(&d2), b1);
}
