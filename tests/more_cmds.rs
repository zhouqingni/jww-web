//! パラメ・曲線・寸法（角度・円周）・寸解

use jwwview::cad::*;
use jwwview::edit::Editor;
use jwwview::jww::{self, Shape, P};

fn lines(ed: &Editor) -> Vec<(P, P)> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Line { a, b } = e.shape { Some((a, b)) } else { None }).collect()
}
fn texts(ed: &Editor) -> Vec<String> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Text(t) = &e.shape { Some(t.s.clone()) } else { None }).collect()
}

#[test]
fn parametric_stretches_rect() {
    let mut ed = Editor::blank();
    ed.rect(p(0.0, 0.0), p(20.0, 10.0));
    ed.circle(p(18.0, 5.0), 100.0); // 半径 1mm，整个在框里 → 整个移动
    ed.circle(p(5.0, 5.0), 100.0); // 框外 → 不动
    // 框住右边，往右 5mm：矩形变宽到 25
    let n = ed.parametric(p(15.0, -1.0), p(21.0, 11.0), p(5.0, 0.0));
    assert_eq!(n, 4); // 上下两条伸长、右边线整条移动、右边的圆整个移动
    let ls = lines(&ed);
    assert!(ls.iter().any(|(a, b)| (a.y == 0.0 && b.y == 0.0) && (a.x.max(b.x) - 25.0).abs() < 1e-9));
    assert!(ls.iter().any(|(a, b)| (a.x - 25.0).abs() < 1e-9 && (b.x - 25.0).abs() < 1e-9));
    let cs: Vec<P> = ed.doc.ents.iter().filter_map(|e| if let Shape::Arc(a) = &e.shape { Some(a.c) } else { None }).collect();
    assert!(cs.contains(&p(23.0, 5.0)) && cs.contains(&p(5.0, 5.0)));
}

#[test]
fn spline_passes_points() {
    let mut ed = Editor::blank();
    let pts = [p(0.0, 0.0), p(10.0, 10.0), p(20.0, 0.0)];
    let v = ed.spline(&pts, 8);
    assert_eq!(v.len(), 16);
    let ls = lines(&ed);
    // 通过各点，首尾相接
    assert_eq!(ls[0].0, p(0.0, 0.0));
    assert!(dist(ls[7].1, p(10.0, 10.0)) < 1e-9);
    assert!(dist(ls[15].1, p(20.0, 0.0)) < 1e-9);
    // 同一个曲線属性
    let g = ed.doc.ents[0].a.group;
    assert!(g > 0 && ed.doc.ents.iter().all(|e| e.a.group == g));
}

#[test]
fn angle_and_arc_dims() {
    let mut ed = Editor::blank();
    ed.dim_angle(p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(5.0, 2.0));
    assert!(texts(&ed).contains(&"45°".to_string()));
    // 位置在另一侧 → 量 315°
    let mut ed = Editor::blank();
    ed.dim_angle(p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(-5.0, -2.0));
    assert!(texts(&ed).contains(&"315°".to_string()), "{:?}", texts(&ed));
    // 半径 10mm（=1000）的半圆：弧长 3141.59
    let mut ed = Editor::blank();
    let i = ed.arc(p(0.0, 0.0), 10.0, 0.0, std::f64::consts::PI);
    ed.dim_arclen(i, p(0.0, 12.0));
    assert!(texts(&ed).iter().any(|s| s == "⌒3,141.59"), "{:?}", texts(&ed));
}

#[test]
fn explode_dim_entity() {
    // Jw_cad 的寸法图形（寸法線＋寸法值一体）
    let mut ed = Editor::blank();
    let a = ed.attr();
    ed.line(p(0.0, 0.0), p(1.0, 0.0));
    let line = jww::Ent { a: a.clone(), shape: Shape::Line { a: p(0.0, 5.0), b: p(30.0, 5.0) } };
    let i = ed.text(p(10.0, 6.0), "3,000");
    let text = ed.doc.ents.remove(i);
    ed.doc.ents.push(jww::Ent { a, shape: Shape::Dim(jww::Dim { line: Box::new(line), text: Box::new(text), sxf: None }) });
    assert_eq!(ed.explode_dims(&[1]), 1);
    assert_eq!(ed.doc.ents.len(), 3);
    assert!(!ed.doc.ents.iter().any(|e| matches!(e.shape, Shape::Dim(_))));
    assert!(texts(&ed).contains(&"3,000".to_string()));
    assert!(lines(&ed).contains(&(p(0.0, 5.0), p(30.0, 5.0))));
}

fn has_seg(ls: &[(P, P)], a: P, b: P) -> bool {
    ls.iter().any(|(x, y)| (dist(*x, a) < 1e-6 && dist(*y, b) < 1e-6) || (dist(*x, b) < 1e-6 && dist(*y, a) < 1e-6))
}

#[test]
fn envelope_t_junction() {
    // 横墙 y=0〜2，竖墙 x=9〜11 顶到横墙的里侧线
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(0.0, 2.0), p(20.0, 2.0));
    ed.line(p(9.0, 2.0), p(9.0, 12.0));
    ed.line(p(11.0, 2.0), p(11.0, 12.0));
    ed.begin();
    ed.envelope(p(-1.0, -1.0), p(21.0, 13.0));
    let ls = lines(&ed);
    // 里侧线在两道竖线之间断开，墙端闭合
    assert!(has_seg(&ls, p(0.0, 2.0), p(9.0, 2.0)) && has_seg(&ls, p(11.0, 2.0), p(20.0, 2.0)));
    assert!(!ls.iter().any(|(a, b)| dist(mid(*a, *b), p(10.0, 2.0)) < 1e-6));
    assert!(has_seg(&ls, p(0.0, 0.0), p(20.0, 0.0)));
    assert!(has_seg(&ls, p(0.0, 0.0), p(0.0, 2.0)) && has_seg(&ls, p(9.0, 12.0), p(11.0, 12.0)));
    assert_eq!(ls.len(), 8);
}

#[test]
fn envelope_cross_and_single_line() {
    let mut ed = Editor::blank();
    // 十字：两道墙交叉 → 12 条边的轮廓
    ed.line(p(-10.0, 0.0), p(10.0, 0.0));
    ed.line(p(-10.0, 2.0), p(10.0, 2.0));
    ed.line(p(0.0, -10.0), p(0.0, 10.0));
    ed.line(p(2.0, -10.0), p(2.0, 10.0));
    ed.envelope(p(-11.0, -11.0), p(11.0, 11.0));
    let ls = lines(&ed);
    assert_eq!(ls.len(), 12, "{ls:?}");
    // 交叉处里面没有线
    assert!(!ls.iter().any(|(a, b)| { let m = mid(*a, *b); m.x > 0.0 && m.x < 2.0 && m.y > 0.0 && m.y < 2.0 }));
    // 单独一条线穿过墙：墙里的那段去掉
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(0.0, 2.0), p(20.0, 2.0));
    ed.line(p(5.0, -5.0), p(5.0, 5.0));
    ed.envelope(p(-1.0, -6.0), p(21.0, 6.0));
    let ls = lines(&ed);
    assert!(has_seg(&ls, p(5.0, -5.0), p(5.0, 0.0)) && has_seg(&ls, p(5.0, 2.0), p(5.0, 5.0)));
}

#[test]
fn figure_register_and_read_back() {
    let mut ed = Editor::blank();
    ed.doc.glayers[0].scale = 50.0;
    ed.rect(p(0.0, 0.0), p(12.0, 6.0));
    ed.circle(p(6.0, 3.0), 100.0);
    let idx: Vec<usize> = (0..ed.doc.ents.len()).collect();
    let b = ed.figure_bytes(&idx, p(0.0, 0.0));
    let j = jww::read_jws_bytes(&b).unwrap();
    assert_eq!(j.version, 700);
    assert_eq!(j.ents.len(), 5);
    assert_eq!(j.base, p(0.0, 0.0));
    assert_eq!(j.scales[0], 50.0);
    // 放进 1/100 的图纸：实寸不变 → 图寸减半
    let mut ed2 = Editor::blank();
    ed2.place_figure(&j, p(100.0, 100.0), 1.0, 0.0);
    assert!(has_seg(&lines(&ed2), p(100.0, 100.0), p(106.0, 100.0)));
}
