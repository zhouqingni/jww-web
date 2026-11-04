//! F 组：塗り・ハッチ・測定・整理。缩尺 1/100：实寸 100 = 图寸 1mm。

use jwwview::cad::*;
use jwwview::edit::Editor;
use jwwview::fill::{path_len, signed_area, triangulate, FillColor};
use jwwview::jww::{self, Shape, P};
use jwwview::jwwsave;
use jwwview::tools::{Kind, Tools};

fn lines(ed: &Editor) -> Vec<(P, P)> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Line { a, b } = e.shape { Some((a, b)) } else { None }).collect()
}

/// L 字形（凹）：面积 10×10 − 5×5 = 75
fn l_shape() -> Vec<P> {
    vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 5.0), p(5.0, 5.0), p(5.0, 10.0), p(0.0, 10.0)]
}

#[test]
fn triangulate_concave() {
    let t = triangulate(&l_shape());
    assert_eq!(t.len(), 4);
    let sum: f64 = t.iter().map(|x| signed_area(x).abs()).sum();
    assert!((sum - 75.0).abs() < 1e-9);
    // 顺时针给点也一样
    let mut r = l_shape();
    r.reverse();
    let sum: f64 = triangulate(&r).iter().map(|x| signed_area(x).abs()).sum();
    assert!((sum - 75.0).abs() < 1e-9);
}

#[test]
fn solid_saved_and_read_back() {
    let mut ed = Editor::blank();
    ed.set_write(0, 4);
    let n = ed.solid_poly(&l_shape(), FillColor::Rgb(0x00c0c0c0)).len();
    ed.pen.color = 3;
    ed.solid_poly(&[p(20.0, 0.0), p(30.0, 0.0), p(30.0, 10.0), p(20.0, 10.0)], FillColor::Pen);
    let d = jww::read_bytes(&jwwsave::write_bytes(&ed.doc)).unwrap();
    let solids: Vec<_> = d.ents.iter().filter(|e| matches!(e.shape, Shape::Solid { .. })).collect();
    assert_eq!(solids.len(), n + 2);
    assert!(solids[..n].iter().all(|e| e.a.color == 10 && e.a.layer == 4));
    assert!(matches!(solids[0].shape, Shape::Solid { rgb: Some(0x00c0c0c0), .. }));
    assert!(matches!(solids[n].shape, Shape::Solid { rgb: None, .. }) && solids[n].a.color == 3);
    // 面积对得上
    let area: f64 = solids
        .iter()
        .map(|e| if let Shape::Solid { pts, .. } = &e.shape { signed_area(pts).abs() } else { 0.0 })
        .sum();
    assert!((area - 175.0).abs() < 1e-9);
}

#[test]
fn hatch_square() {
    let mut ed = Editor::blank();
    let sq = [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)];
    // 0°、ピッチ 100（= 图寸 1mm）：y = 1〜9 的 9 条，边界上的不画
    let v = ed.hatch(&sq, 0.0, 100.0, false);
    assert_eq!(v.len(), 9);
    for (a, b) in lines(&ed) {
        assert!((a.y - b.y).abs() < 1e-9 && (dist(a, b) - 10.0).abs() < 1e-9);
    }
    // クロス：再加竖的 9 条
    let mut ed = Editor::blank();
    assert_eq!(ed.hatch(&sq, 0.0, 100.0, true).len(), 18);
    // 凹形：横线穿过缺口时断开
    let mut ed = Editor::blank();
    ed.hatch(&l_shape(), 0.0, 100.0, false);
    let total: f64 = lines(&ed).iter().map(|(a, b)| dist(*a, *b)).sum();
    // y=1〜4 长 10，y=5〜9 长 5（y=5 只有 x=5〜10 那段压在边上）
    assert!((total - (4.0 * 10.0 + 5.0 * 5.0)).abs() < 1e-9, "{total}");
}

#[test]
fn tidy_merges_lines() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.line(p(5.0, 0.0), p(20.0, 0.0)); // 重叠
    ed.line(p(30.0, 0.0), p(20.0, 0.0)); // 相接（反方向）
    ed.line(p(0.0, 5.0), p(10.0, 5.0));
    ed.line(p(10.0, 5.0), p(0.0, 5.0)); // 重复
    ed.line(p(3.0, 3.0), p(3.0, 3.0)); // 长度 0
    ed.line(p(40.0, 0.0), p(50.0, 0.0)); // 同一直线但隔开
    ed.pen.color = 5;
    ed.line(p(0.0, 0.0), p(30.0, 0.0)); // 颜色不同，不合并
    ed.begin();
    assert_eq!(ed.tidy(), 4);
    let ls = lines(&ed);
    assert_eq!(ls.len(), 4);
    let has = |a: P, b: P| ls.iter().any(|(x, y)| (dist(*x, a) < 1e-9 && dist(*y, b) < 1e-9) || (dist(*x, b) < 1e-9 && dist(*y, a) < 1e-9));
    assert!(has(p(0.0, 0.0), p(30.0, 0.0)));
    assert!(has(p(0.0, 5.0), p(10.0, 5.0)));
    assert!(has(p(40.0, 0.0), p(50.0, 0.0)));
    assert!(ed.undo());
    assert_eq!(lines(&ed).len(), 8);
}

#[test]
fn measure_and_fill_by_clicks() {
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    // 測定：3000 + 4000
    t.set(Kind::Measure, &mut ed);
    t.click(&mut ed, p(0.0, 0.0), false, None, 0.5);
    t.click(&mut ed, p(30.0, 0.0), false, None, 0.5);
    t.click(&mut ed, p(30.0, 40.0), false, None, 0.5);
    let note = t.note.clone().unwrap();
    assert!(note.contains("4,000") && note.contains("7,000"), "{note}");
    assert!((path_len(&[p(0.0, 0.0), p(30.0, 0.0), p(30.0, 40.0)]) - 70.0).abs() < 1e-9);
    t.measure_area = true;
    t.click(&mut ed, p(0.0, 40.0), false, None, 0.5);
    // 3000×4000 = 12 ㎡
    assert!(t.note.clone().unwrap().contains("12.0000"), "{:?}", t.note);
    assert!(ed.doc.ents.is_empty());

    // 塗り：四个角点完，按「作図」（Jw_cad 多角形・任意）
    t.set(Kind::Solid, &mut ed);
    for q in [p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)] {
        t.click(&mut ed, q, false, None, 0.5);
    }
    t.fill_region(&mut ed);
    assert_eq!(ed.doc.ents.len(), 2);
    // ハッチ：线一条条左键点（矩形 4 边），「実行」
    ed.rect(p(0.0, 20.0), p(10.0, 30.0));
    t.set(Kind::Hatch, &mut ed);
    t.hatch_angle = "90".into();
    for q in [p(5.0, 20.0), p(10.0, 25.0), p(5.0, 30.0), p(0.0, 25.0)] {
        t.click(&mut ed, q, false, None, 0.5);
    }
    t.hatch_run(&mut ed);
    // 间隔 100 = 图寸 1mm：x = 1..9 九条
    assert_eq!(lines(&ed).len(), 4 + 9);
    // 右键点闭合的连续线也行
    t.hatch_angle = "0".into();
    t.click(&mut ed, p(10.0, 22.0), true, None, 0.5);
    t.hatch_run(&mut ed);
    assert_eq!(lines(&ed).len(), 4 + 9 + 9);
}
