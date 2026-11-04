//! Jw_cad 帮助（Jw_cad.chm）写的鼠标操作：L = 左，R = 右，LL / RR = 不动再点一下。
//! 缩尺 1/100：实寸 1000 = 图寸 10mm。

use jwwview::cad::*;
use jwwview::edit::Editor;
use jwwview::jww::{Shape, P};
use jwwview::tools::{Btn, Kind, Tools};

const TOL: f64 = 0.5;

fn click(t: &mut Tools, ed: &mut Editor, q: P, right: bool, dbl: bool, shift: bool) {
    let read = if right { ed.read_point(q, TOL) } else { None };
    t.click_btn(ed, q, Btn { right, dbl, shift, ctrl: false }, read, TOL);
}
fn l(t: &mut Tools, ed: &mut Editor, q: P) {
    click(t, ed, q, false, false, false);
}
fn r(t: &mut Tools, ed: &mut Editor, q: P) {
    click(t, ed, q, true, false, false);
}
fn ll(t: &mut Tools, ed: &mut Editor, q: P) {
    click(t, ed, q, false, true, false);
}
fn rr(t: &mut Tools, ed: &mut Editor, q: P) {
    click(t, ed, q, true, true, false);
}
fn lines(ed: &Editor) -> Vec<(P, P)> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Line { a, b } = e.shape { Some((a, b)) } else { None }).collect()
}
fn has(ed: &Editor, a: P, b: P) -> bool {
    lines(ed).iter().any(|(x, y)| (dist(*x, a) < 1e-6 && dist(*y, b) < 1e-6) || (dist(*x, b) < 1e-6 && dist(*y, a) < 1e-6))
}
fn arcs(ed: &Editor) -> Vec<(P, f64)> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Arc(a) = &e.shape { Some((a.c, a.r)) } else { None }).collect()
}

#[test]
fn stretch_like_jwcad() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(30.0, -10.0), p(30.0, 10.0));
    ed.line(p(10.0, 5.0), p(10.0, 20.0));
    let mut t = Tools::default();
    t.set(Kind::Stretch, &mut ed);
    // L で線、L = 任意点まで（点を指示した側が残る）
    l(&mut t, &mut ed, p(2.0, 0.0));
    l(&mut t, &mut ed, p(25.0, 3.0));
    assert!(has(&ed, p(0.0, 0.0), p(25.0, 0.0)));
    // 線の内側の点 = 縮む
    l(&mut t, &mut ed, p(2.0, 0.0));
    l(&mut t, &mut ed, p(15.0, 1.0));
    assert!(has(&ed, p(0.0, 0.0), p(15.0, 0.0)));
    // RR = 基準線（x=30）、その後 L の線は基準線まで
    rr(&mut t, &mut ed, p(30.0, 0.0));
    l(&mut t, &mut ed, p(2.0, 0.0));
    assert!(has(&ed, p(0.0, 0.0), p(30.0, 0.0)));
    // 基準線までの伸縮を続けて
    ed.line(p(0.0, -5.0), p(5.0, -5.0));
    l(&mut t, &mut ed, p(1.0, -5.0));
    assert!(has(&ed, p(0.0, -5.0), p(30.0, -5.0)));
    // R = 切断
    t.reset(&mut ed);
    let n = ed.doc.ents.len();
    r(&mut t, &mut ed, p(10.0, 12.0));
    assert_eq!(ed.doc.ents.len(), n + 1);
    assert!(has(&ed, p(10.0, 5.0), p(10.0, 12.0)) && has(&ed, p(10.0, 12.0), p(10.0, 20.0)));
    // 突出寸法 500：基準線から 5mm 出る
    t.protrude = "500".into();
    rr(&mut t, &mut ed, p(30.0, 2.0));
    ed.line(p(0.0, -8.0), p(5.0, -8.0));
    l(&mut t, &mut ed, p(1.0, -8.0));
    assert!(has(&ed, p(0.0, -8.0), p(35.0, -8.0)));
}

#[test]
fn stretch_arc_and_end_move() {
    let mut ed = Editor::blank();
    // 0°〜90° の円弧、半径 10
    ed.arc(p(0.0, 0.0), 10.0, 0.0, std::f64::consts::FRAC_PI_2);
    let mut t = Tools::default();
    t.set(Kind::Stretch, &mut ed);
    // 終点側を 135° 方向まで伸ばす
    l(&mut t, &mut ed, polar(p(0.0, 0.0), 10.0, 0.2));
    l(&mut t, &mut ed, polar(p(0.0, 0.0), 15.0, rad(135.0)));
    let Shape::Arc(a) = &ed.doc.ents[0].shape else { panic!() };
    assert!(a.start.abs() < 1e-9 && (a.sweep - rad(135.0)).abs() < 1e-9, "{a:?}");
    // 端点移動：左ドラッグ 8 時
    ed.line(p(0.0, 20.0), p(10.0, 20.0));
    assert!(t.left_drag_special(&mut ed, p(9.0, 20.0), 8, TOL));
    l(&mut t, &mut ed, p(12.0, 25.0));
    assert!(has(&ed, p(0.0, 20.0), p(12.0, 25.0)));
}

#[test]
fn stretch_batch() {
    let mut ed = Editor::blank();
    for k in 0..3 {
        ed.line(p(0.0, k as f64 * 5.0), p(10.0, k as f64 * 5.0));
    }
    ed.line(p(20.0, -5.0), p(20.0, 20.0));
    let mut t = Tools::default();
    t.set(Kind::Stretch, &mut ed);
    t.batch = true;
    l(&mut t, &mut ed, p(20.0, 0.0)); // 基準線
    l(&mut t, &mut ed, p(5.0, 0.0)); // 始めの線
    l(&mut t, &mut ed, p(5.0, 10.0)); // 終わりの線
    assert_eq!(ed.sel.len(), 3);
    r(&mut t, &mut ed, p(50.0, 50.0)); // R = 実行
    for k in 0..3 {
        assert!(has(&ed, p(0.0, k as f64 * 5.0), p(20.0, k as f64 * 5.0)));
    }
}

#[test]
fn corner_and_chamfer_cut() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    let mut t = Tools::default();
    t.set(Kind::Corner, &mut ed);
    r(&mut t, &mut ed, p(7.0, 0.1));
    assert!(has(&ed, p(0.0, 0.0), p(7.0, 0.0)) && has(&ed, p(7.0, 0.0), p(20.0, 0.0)));
    // 円の切断 = 一周の円弧
    ed.circle(p(50.0, 0.0), 1000.0);
    t.set(Kind::Chamfer, &mut ed);
    r(&mut t, &mut ed, p(60.0, 0.0));
    let e = ed.doc.ents.iter().find(|e| matches!(e.shape, Shape::Arc(_))).unwrap();
    let Shape::Arc(a) = &e.shape else { panic!() };
    assert!(!a.full && (a.sweep - std::f64::consts::TAU).abs() < 1e-9);
}

#[test]
fn two_line_with_bend_and_envelope() {
    let mut ed = Editor::blank();
    // L 字の芯：横 0..50、縦 x=50
    ed.line(p(0.0, 0.0), p(50.0, 0.0));
    ed.line(p(50.0, 0.0), p(50.0, 50.0));
    // 包絡する壁（x = -20 の縦線）
    ed.line(p(-20.0, -20.0), p(-20.0, 20.0));
    let mut t = Tools::default();
    t.set(Kind::TwoLine, &mut ed);
    t.twoline = "100,100".into(); // 1mm ずつ
    l(&mut t, &mut ed, p(25.0, 0.0)); // 基準線
    rr(&mut t, &mut ed, p(-20.0, 10.0)); // 始点 = 壁と包絡（2線の外側を指示 = 壁に開口）
    ll(&mut t, &mut ed, p(50.0, 25.0)); // 基準線変更（折れ曲がり）
    l(&mut t, &mut ed, p(48.0, 40.0)); // 終点
    assert!(has(&ed, p(-20.0, -1.0), p(51.0, -1.0)) && has(&ed, p(51.0, -1.0), p(51.0, 40.0)));
    assert!(has(&ed, p(-20.0, 1.0), p(49.0, 1.0)) && has(&ed, p(49.0, 1.0), p(49.0, 40.0)));
    assert!(has(&ed, p(-20.0, -20.0), p(-20.0, -1.0)) && has(&ed, p(-20.0, 1.0), p(-20.0, 20.0)));
    // 基準線は最後の縦線が残り、すぐ次の始点
    l(&mut t, &mut ed, p(30.0, 60.0));
    l(&mut t, &mut ed, p(30.0, 70.0));
    assert!(has(&ed, p(49.0, 60.0), p(49.0, 70.0)) && has(&ed, p(51.0, 60.0), p(51.0, 70.0)));

    // 2線の内側を指示 = 間だけ残して端を閉じる
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(0.0, 50.0));
    ed.line(p(-30.0, 40.0), p(30.0, 40.0));
    t.set(Kind::TwoLine, &mut ed);
    l(&mut t, &mut ed, p(0.0, 10.0));
    l(&mut t, &mut ed, p(0.0, 0.0));
    rr(&mut t, &mut ed, p(0.2, 40.0));
    assert!(has(&ed, p(-1.0, 40.0), p(1.0, 40.0)) || has(&ed, p(1.0, 40.0), p(-1.0, 40.0)));
    assert_eq!(lines(&ed).len(), 4);

    // 留線：留線出 200 = 2mm 伸ばして閉じる
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(50.0, 0.0));
    t.set(Kind::TwoLine, &mut ed);
    t.tl_cap = true;
    t.tl_cap_out = "200".into();
    l(&mut t, &mut ed, p(25.0, 0.0));
    l(&mut t, &mut ed, p(10.0, 3.0));
    l(&mut t, &mut ed, p(40.0, 3.0));
    assert!(has(&ed, p(8.0, 1.0), p(42.0, 1.0)) || has(&ed, p(42.0, 1.0), p(8.0, 1.0)));
    assert!(lines(&ed).iter().any(|(a, b)| (a.x - 8.0).abs() < 1e-9 && (b.x - 8.0).abs() < 1e-9));
    assert!(!t.tl_cap);
}

#[test]
fn range_select_like_jwcad() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.line(p(5.0, -5.0), p(5.0, 50.0)); // 枠から出る線
    ed.text(p(1.0, 1.0), "文字");
    let mut t = Tools::default();
    t.set(Kind::Range, &mut ed);
    // 終点 L = 文字を除く
    l(&mut t, &mut ed, p(-1.0, -1.0));
    l(&mut t, &mut ed, p(11.0, 11.0));
    assert_eq!(ed.sel, vec![0]);
    // 選択後：R で文字の追加、L で線の除外
    r(&mut t, &mut ed, p(2.0, 2.0));
    assert!(ed.sel.contains(&2));
    l(&mut t, &mut ed, p(8.0, 0.0));
    assert_eq!(ed.sel, vec![2]);
    // 何もない所の L では範囲が始まらない（「追加範囲」で足す）
    l(&mut t, &mut ed, p(30.0, 30.0));
    assert_eq!(ed.sel, vec![2]);
    // 終点 R = 文字を含む、RR = 枠と交差するものも
    t.reset(&mut ed);
    l(&mut t, &mut ed, p(-1.0, -1.0));
    r(&mut t, &mut ed, p(11.0, 11.0));
    assert_eq!(ed.sel.len(), 2);
    t.reset(&mut ed);
    l(&mut t, &mut ed, p(-1.0, -1.0));
    rr(&mut t, &mut ed, p(11.0, 11.0));
    assert_eq!(ed.sel.len(), 3);
    // 始点 R で連続線
    t.reset(&mut ed);
    ed.rect(p(100.0, 0.0), p(110.0, 10.0));
    r(&mut t, &mut ed, p(105.0, 0.0));
    assert_eq!(ed.sel.len(), 4);
}

#[test]
fn envelope_right_and_shift() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(30.0, 0.0));
    ed.line(p(0.0, 10.0), p(30.0, 10.0));
    let mut t = Tools::default();
    t.set(Kind::Envelope, &mut ed);
    // R = 範囲内消去
    l(&mut t, &mut ed, p(10.0, -1.0));
    r(&mut t, &mut ed, p(20.0, 1.0));
    assert!(has(&ed, p(0.0, 0.0), p(10.0, 0.0)) && has(&ed, p(20.0, 0.0), p(30.0, 0.0)));
    // Shift+L = 中間線消去（枠を通り抜ける線だけ）
    l(&mut t, &mut ed, p(12.0, 9.0));
    click(&mut t, &mut ed, p(18.0, 11.0), false, false, true);
    assert!(has(&ed, p(0.0, 10.0), p(12.0, 10.0)) && has(&ed, p(18.0, 10.0), p(30.0, 10.0)));
}

#[test]
fn offset_mouse_distance_and_connect() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(20.0, 0.0), p(20.0, 20.0));
    let mut t = Tools::default();
    t.set(Kind::Offset, &mut ed);
    // 間隔なし：マウス位置が間隔（y=3 → 300）
    l(&mut t, &mut ed, p(10.0, 0.0));
    l(&mut t, &mut ed, p(10.0, 3.0));
    assert!(has(&ed, p(0.0, 3.0), p(20.0, 3.0)));
    // R で線 = 前回値、方向も R = 前の複線と連結
    r(&mut t, &mut ed, p(20.0, 10.0));
    assert_eq!(t.offset, "300");
    r(&mut t, &mut ed, p(15.0, 10.0));
    assert!(has(&ed, p(0.0, 3.0), p(17.0, 3.0)) && has(&ed, p(17.0, 3.0), p(17.0, 20.0)));
}

#[test]
fn center_line_line_and_point() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    let mut t = Tools::default();
    t.set(Kind::Center, &mut ed);
    l(&mut t, &mut ed, p(10.0, 0.0)); // 線
    ed.point(p(5.0, 10.0));
    r(&mut t, &mut ed, p(5.0, 10.1)); // 点
    l(&mut t, &mut ed, p(0.0, 7.0));
    l(&mut t, &mut ed, p(20.0, 3.0));
    assert!(has(&ed, p(0.0, 5.0), p(20.0, 5.0)));
    // 中心線寸法 1000：始点から指示した方向へ 10mm
    t.center_len = "1000".into();
    l(&mut t, &mut ed, p(10.0, 0.0));
    r(&mut t, &mut ed, p(5.0, 10.1));
    l(&mut t, &mut ed, p(30.0, 0.0));
    l(&mut t, &mut ed, p(-50.0, 0.0));
    assert!(has(&ed, p(30.0, 5.0), p(20.0, 5.0)));
}

#[test]
fn divide_combinations() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(30.0, 0.0));
    ed.point(p(0.0, 30.0));
    let mut t = Tools::default();
    t.set(Kind::Divide, &mut ed);
    t.div_n = 3;
    // 線と読取点 = 平行線
    l(&mut t, &mut ed, p(10.0, 0.0));
    r(&mut t, &mut ed, p(0.1, 30.0));
    assert!(has(&ed, p(0.0, 10.0), p(30.0, 10.0)) && has(&ed, p(0.0, 20.0), p(30.0, 20.0)));
    // ２点 → L で線上
    let pts = |ed: &Editor| ed.doc.ents.iter().filter(|e| matches!(e.shape, Shape::Point(_))).count();
    let n0 = pts(&ed);
    r(&mut t, &mut ed, p(0.1, 0.1));
    r(&mut t, &mut ed, p(29.9, 0.1));
    l(&mut t, &mut ed, p(15.0, 0.0));
    assert_eq!(pts(&ed), n0 + 2);
    // ２点 → R で２点間
    r(&mut t, &mut ed, p(0.1, 0.1));
    r(&mut t, &mut ed, p(0.1, 29.9));
    r(&mut t, &mut ed, p(40.0, 40.0));
    assert_eq!(pts(&ed), n0 + 4);
    // RR = 連続点分割：0,0 → 30,0 → 30,30（総長 60 を 3 等分）、同じ点で実行
    ed.point(p(30.0, 30.0));
    rr(&mut t, &mut ed, p(0.1, 0.1));
    r(&mut t, &mut ed, p(29.9, 0.1));
    r(&mut t, &mut ed, p(29.9, 29.9));
    r(&mut t, &mut ed, p(29.9, 29.9));
    let ps: Vec<P> = ed.doc.ents.iter().filter_map(|e| if let Shape::Point(x) = &e.shape { Some(x.p) } else { None }).collect();
    assert!(ps.iter().any(|x| dist(*x, p(20.0, 0.0)) < 1e-9) && ps.iter().any(|x| dist(*x, p(30.0, 10.0)) < 1e-9));
}

#[test]
fn tangent_circle_three_and_radius() {
    let mut ed = Editor::blank();
    // 正方形の 3 辺に接する円：中心 (10,10)、半径 10
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(20.0, 0.0), p(20.0, 20.0));
    ed.line(p(0.0, 0.0), p(0.0, 20.0));
    let mut t = Tools::default();
    t.set(Kind::TanCircle, &mut ed);
    l(&mut t, &mut ed, p(10.0, 0.0));
    l(&mut t, &mut ed, p(20.0, 10.0));
    l(&mut t, &mut ed, p(0.0, 10.0));
    let c = arcs(&ed);
    assert_eq!(c.len(), 1);
    assert!(dist(c[0].0, p(10.0, 10.0)) < 1e-6 && (c[0].1 - 10.0).abs() < 1e-6, "{c:?}");
    // 半径 500：2 本の線、位置を指示
    t.tan_r = "500".into();
    l(&mut t, &mut ed, p(15.0, 0.0));
    l(&mut t, &mut ed, p(0.0, 15.0));
    l(&mut t, &mut ed, p(6.0, 6.0));
    let c = arcs(&ed);
    assert!(c.iter().any(|(o, r)| dist(*o, p(5.0, 5.0)) < 1e-6 && (*r - 5.0).abs() < 1e-6), "{c:?}");
}

#[test]
fn dim_radius_side_and_angle() {
    let mut ed = Editor::blank();
    ed.circle(p(0.0, 0.0), 1000.0);
    let mut t = Tools::default();
    t.set(Kind::Dim, &mut ed);
    t.dim_kind = jwwview::tools::DimKind::Radius;
    // R = 寸法値を円の外側
    r(&mut t, &mut ed, p(10.0, 0.0));
    let txt = ed.doc.ents.iter().find_map(|e| if let Shape::Text(x) = &e.shape { Some(x.clone()) } else { None }).unwrap();
    assert_eq!(txt.s, "R1,000");
    assert!(txt.start.x > 10.0);
    // 2 線間の角度：LL で 1 本目
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(0.0, 0.0), p(0.0, 20.0));
    t.set(Kind::Dim, &mut ed);
    t.dim_kind = jwwview::tools::DimKind::Angle;
    ll(&mut t, &mut ed, p(10.0, 0.0));
    l(&mut t, &mut ed, p(0.0, 10.0));
    l(&mut t, &mut ed, p(5.0, 5.0));
    assert!(ed.doc.ents.iter().any(|e| matches!(&e.shape, Shape::Text(x) if x.s == "90°")));
}

#[test]
fn text_left_moves_right_copies() {
    let mut ed = Editor::blank();
    ed.text(p(0.0, 0.0), "ABC");
    let mut t = Tools::default();
    t.set(Kind::Text, &mut ed);
    // R = 複写
    r(&mut t, &mut ed, p(1.0, 1.0));
    l(&mut t, &mut ed, p(50.0, 50.0));
    let ts: Vec<P> = ed.doc.ents.iter().filter_map(|e| if let Shape::Text(x) = &e.shape { Some(x.start) } else { None }).collect();
    assert_eq!(ts.len(), 2);
    // L = 移動（内容を変えて移動）
    l(&mut t, &mut ed, p(1.0, 1.0));
    assert_eq!(t.text, "ABC");
    t.text = "XYZ".into();
    l(&mut t, &mut ed, p(100.0, 0.0));
    assert!(ed.doc.ents.iter().any(|e| matches!(&e.shape, Shape::Text(x) if x.s == "XYZ" && dist(x.start, p(100.0, 0.0)) < 1e-9)));
}

#[test]
fn param_base_is_center() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    let mut t = Tools::default();
    t.set(Kind::Param, &mut ed);
    l(&mut t, &mut ed, p(5.0, -1.0));
    l(&mut t, &mut ed, p(15.0, 1.0));
    // 基準点は枠の中心 (10,0)：(15,0) を指示 = 右端が 5 伸びる
    l(&mut t, &mut ed, p(15.0, 0.0));
    assert!(has(&ed, p(0.0, 0.0), p(15.0, 0.0)));
}

#[test]
fn take_attr_invert() {
    let mut ed = Editor::blank();
    ed.set_write(0, 1);
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.set_write(0, 0);
    ed.doc.glayers[0].lay_state[1] = 0; // 非表示
    let mut t = Tools::default();
    t.set(Kind::TakeAttr, &mut ed);
    l(&mut t, &mut ed, p(5.0, 0.0));
    assert_eq!(ed.lay(), 0);
    r(&mut t, &mut ed, p(50.0, 50.0)); // 反転表示
    assert!(ed.invert);
    l(&mut t, &mut ed, p(5.0, 0.0));
    assert_eq!(ed.lay(), 1);
    assert!(!ed.invert);
}

#[test]
fn shift_picks_write_layer_only() {
    let mut ed = Editor::blank();
    ed.set_write(0, 2);
    ed.line(p(0.0, 0.0), p(10.0, 0.0)); // レイヤ 2
    ed.set_write(0, 3);
    ed.line(p(0.0, 0.2), p(10.0, 0.2)); // レイヤ 3（書込）
    let mut t = Tools::default();
    t.set(Kind::Erase, &mut ed);
    // Shift+R：書込レイヤの線だけが対象
    click(&mut t, &mut ed, p(5.0, 0.0), true, false, true);
    assert_eq!(ed.doc.ents.len(), 1);
    assert_eq!(ed.doc.ents[0].a.layer, 2);
}

#[test]
fn right_drag_reads() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 10.0));
    ed.circle(p(50.0, 0.0), 1000.0);
    let mut t = Tools::default();
    t.set(Kind::Point, &mut ed);
    // AM3 = 線の中点
    let x = t.right_drag_read(&mut ed, p(5.0, 2.5), 3, TOL).unwrap();
    assert!(dist(x, p(10.0, 5.0)) < 1e-9);
    // AM3 = 円の中心、AM0 = 円周 1/4 点
    assert!(dist(t.right_drag_read(&mut ed, p(60.0, 0.0), 3, TOL).unwrap(), p(50.0, 0.0)) < 1e-9);
    assert!(dist(t.right_drag_read(&mut ed, polar(p(50.0, 0.0), 10.0, 1.4), 0, TOL).unwrap(), p(50.0, 10.0)) < 1e-9);
    // AM3 を読取点で = 2 点間中心（もう一点）
    assert!(t.right_drag_read(&mut ed, p(0.1, 0.1), 3, TOL).is_none());
    r(&mut t, &mut ed, p(19.9, 9.9));
    assert!(ed.doc.ents.iter().any(|e| matches!(&e.shape, Shape::Point(x) if dist(x.p, p(10.0, 5.0)) < 1e-9)));
    // AM9 = 線上点：点から線への垂線の足
    assert!(t.right_drag_read(&mut ed, p(10.0, 5.0), 9, TOL).is_none());
    l(&mut t, &mut ed, p(0.0, 10.0));
    let foot = project(p(0.0, 0.0), p(20.0, 10.0), p(0.0, 10.0));
    assert!(ed.doc.ents.iter().any(|e| matches!(&e.shape, Shape::Point(x) if dist(x.p, foot) < 1e-9)));
    // 線コマンド：線上から AM0 = 垂線
    t.set(Kind::Line, &mut ed);
    assert!(t.right_drag_read(&mut ed, p(10.0, 5.0), 0, TOL).is_none());
    l(&mut t, &mut ed, p(5.0, 20.0));
    let n = norm(perp(p(20.0, 10.0)));
    let (a, b) = *lines(&ed).last().unwrap();
    assert!(dist(a, p(10.0, 5.0)) < 1e-9 && cross(sub(b, a), n).abs() < 1e-9);
}

#[test]
fn auto_mode() {
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    assert!(t.command(1, &mut ed));
    assert_eq!(t.kind, Kind::Auto);
    // 何もない所 L = 線の始点 → 終点で線、AUTO に戻る
    l(&mut t, &mut ed, p(0.0, 0.0));
    assert_eq!(t.kind, Kind::Line);
    l(&mut t, &mut ed, p(20.0, 0.0));
    assert_eq!(t.kind, Kind::Auto);
    assert!(has(&ed, p(0.0, 0.0), p(20.0, 0.0)));
    // LL = 円の中心
    ll(&mut t, &mut ed, p(50.0, 0.0));
    assert_eq!(t.kind, Kind::Circle);
    l(&mut t, &mut ed, p(55.0, 0.0));
    assert_eq!(t.kind, Kind::Auto);
    assert_eq!(arcs(&ed).len(), 1);
    // 線 L → 範囲外の点 = 伸縮 → 伸縮点
    l(&mut t, &mut ed, p(5.0, 0.0));
    l(&mut t, &mut ed, p(30.0, 2.0));
    l(&mut t, &mut ed, p(25.0, 2.0));
    assert!(has(&ed, p(0.0, 0.0), p(25.0, 0.0)));
    // 線 L → 線上 L → L = 部分消し
    l(&mut t, &mut ed, p(5.0, 0.0));
    l(&mut t, &mut ed, p(10.0, 0.0));
    l(&mut t, &mut ed, p(15.0, 0.0));
    assert!(has(&ed, p(0.0, 0.0), p(10.0, 0.0)) && has(&ed, p(15.0, 0.0), p(25.0, 0.0)));
    // 線 L → R（線上の読取点でない所）= 消去
    let n = ed.doc.ents.len();
    l(&mut t, &mut ed, p(20.0, 0.0));
    r(&mut t, &mut ed, p(22.0, 0.0));
    assert_eq!(ed.doc.ents.len(), n - 1);
    // 線 A L → 線 B L = コーナー
    ed.line(p(0.0, 10.0), p(8.0, 10.0));
    ed.line(p(10.0, 12.0), p(10.0, 20.0));
    l(&mut t, &mut ed, p(4.0, 10.0));
    l(&mut t, &mut ed, p(10.0, 16.0));
    assert!(has(&ed, p(0.0, 10.0), p(10.0, 10.0)) && has(&ed, p(10.0, 10.0), p(10.0, 20.0)));
    // 線上 R = 複線（前回値なし → 間隔はマウス）
    r(&mut t, &mut ed, p(4.0, 10.0));
    assert_eq!(t.kind, Kind::Offset);
    l(&mut t, &mut ed, p(4.0, 13.0));
    assert_eq!(t.kind, Kind::Auto);
    assert!(has(&ed, p(0.0, 13.0), p(10.0, 13.0)));
    // 何もない所 R = 矩形
    r(&mut t, &mut ed, p(80.0, 80.0));
    assert_eq!(t.kind, Kind::Rect);
}

#[test]
fn wants_double_rules() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    let mut t = Tools::default();
    t.set(Kind::Stretch, &mut ed);
    // 伸縮：線上の R は RR（基準線）の可能性 → 待つ。L は待たない
    assert!(t.wants_double(&ed, p(5.0, 0.0), true, TOL));
    assert!(!t.wants_double(&ed, p(5.0, 0.0), false, TOL));
    // 範囲選択：終点だけ
    t.set(Kind::Range, &mut ed);
    assert!(!t.wants_double(&ed, p(-1.0, -1.0), false, TOL));
    l(&mut t, &mut ed, p(-1.0, -1.0));
    assert!(t.wants_double(&ed, p(30.0, 1.0), false, TOL));
    // 線：双击なし
    t.set(Kind::Line, &mut ed);
    assert!(!t.wants_double(&ed, p(5.0, 0.0), true, TOL));
}

fn points(ed: &Editor) -> Vec<P> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Point(x) = &e.shape { Some(x.p) } else { None }).collect()
}

#[test]
fn divide_more() {
    // 割付：２点間 30mm を 1000（10mm）ずつ
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(30.0, 0.0));
    let mut t = Tools::default();
    t.set(Kind::Divide, &mut ed);
    t.div_wari = true;
    t.div_dist = "1000".into();
    r(&mut t, &mut ed, p(0.1, 0.1));
    r(&mut t, &mut ed, p(29.9, 0.1));
    r(&mut t, &mut ed, p(50.0, 50.0));
    let xs: Vec<f64> = points(&ed).iter().map(|q| q.x).collect();
    assert_eq!(xs, vec![10.0, 20.0]);
    // 振分：700 で 4 個、余り 2 を両側へ → 1, 8, 15, 22, 29
    t.div_furi = true;
    t.div_dist = "700".into();
    let n0 = points(&ed).len();
    r(&mut t, &mut ed, p(0.1, 0.1));
    r(&mut t, &mut ed, p(29.9, 0.1));
    r(&mut t, &mut ed, p(50.0, 50.0));
    let xs: Vec<f64> = points(&ed)[n0..].iter().map(|q| (q.x * 1e6).round() / 1e6).collect();
    assert_eq!(xs, vec![1.0, 8.0, 15.0, 22.0, 29.0]);
    // 割付距離以下：800 以下の等間隔 → 4 等分
    t.div_furi = false;
    t.div_under = true;
    t.div_dist = "800".into();
    let n0 = points(&ed).len();
    r(&mut t, &mut ed, p(0.1, 0.1));
    r(&mut t, &mut ed, p(29.9, 0.1));
    r(&mut t, &mut ed, p(50.0, 50.0));
    assert_eq!(points(&ed).len() - n0, 3);

    // 円全周分割：始点 R、円の読取点でない所 R
    let mut ed = Editor::blank();
    ed.circle(p(0.0, 0.0), 1000.0);
    ed.point(p(10.0, 0.0));
    t.set(Kind::Divide, &mut ed);
    t.div_wari = false;
    t.div_n = 4;
    r(&mut t, &mut ed, p(10.0, 0.1));
    r(&mut t, &mut ed, polar(p(0.0, 0.0), 10.0, 0.7));
    let ps = points(&ed);
    assert_eq!(ps.len(), 1 + 4);
    assert!(ps.iter().any(|q| dist(*q, p(0.0, 10.0)) < 1e-9) && ps.iter().any(|q| dist(*q, p(-10.0, 0.0)) < 1e-9));

    // 等角度分割：直角を 3 等分 → 30°・60°
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(0.0, 0.0), p(0.0, 20.0));
    t.set(Kind::Divide, &mut ed);
    t.div_angle = true;
    t.div_n = 3;
    l(&mut t, &mut ed, p(10.0, 0.0));
    l(&mut t, &mut ed, p(0.0, 10.0));
    assert!(has(&ed, p(0.0, 0.0), polar(p(0.0, 0.0), 20.0, rad(30.0))) && has(&ed, p(0.0, 0.0), polar(p(0.0, 0.0), 20.0, rad(60.0))));

    // 逆分割：交差の向きで結ぶ
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(0.0, 10.0), p(20.0, 10.0));
    t.set(Kind::Divide, &mut ed);
    t.div_angle = false;
    t.div_n = 2;
    l(&mut t, &mut ed, p(5.0, 0.0));
    t.div_rev = true;
    l(&mut t, &mut ed, p(5.0, 10.0));
    assert!(has(&ed, p(0.0, 5.0), p(20.0, 5.0)) || has(&ed, p(10.0, 5.0), p(10.0, 5.0)) || lines(&ed).iter().any(|(a, b)| dist(mid(*a, *b), p(10.0, 5.0)) < 1e-9 && (a.y - 5.0).abs() < 1e-9));

    // 線長割合分割：A 10mm、B 30mm → 間隔が 1 : 3 で比例
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.line(p(0.0, 40.0), p(30.0, 40.0));
    t.set(Kind::Divide, &mut ed);
    t.div_n = 3;
    l(&mut t, &mut ed, p(5.0, 0.0));
    t.div_ratio = Some(10.0);
    l(&mut t, &mut ed, p(5.0, 40.0));
    // 間隔 1, 2, 3（合計 6）→ y = 40/6, 40*3/6
    let ys: Vec<f64> = lines(&ed)[2..].iter().map(|(a, _)| (a.y * 1e6).round() / 1e6).collect();
    assert_eq!(ys, vec![(40.0f64 / 6.0 * 1e6).round() / 1e6, 20.0]);

    // 馬目地：２段・縦 2 → 横 1 本 + 下段の目地 1 本 + 上段（半分ずらし）2 本
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(20.0, 0.0));
    ed.line(p(0.0, 10.0), p(20.0, 10.0));
    t.set(Kind::Divide, &mut ed);
    t.div_n = 2;
    l(&mut t, &mut ed, p(5.0, 0.0));
    t.div_umame = Some(2);
    l(&mut t, &mut ed, p(5.0, 10.0));
    assert_eq!(lines(&ed).len(), 2 + 1 + 1 + 2);
    assert!(has(&ed, p(10.0, 0.0), p(10.0, 5.0)) && has(&ed, p(5.0, 5.0), p(5.0, 10.0)) && has(&ed, p(15.0, 5.0), p(15.0, 10.0)));
}

/// 文字（寸法図形の中の寸法値も）
fn texts(ed: &Editor) -> Vec<String> {
    ed.doc
        .ents
        .iter()
        .filter_map(|e| match &e.shape {
            Shape::Text(x) => Some(x.s.clone()),
            Shape::Dim(d) => match &d.text.shape {
                Shape::Text(x) => Some(x.s.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn dim_figure_like_company_drawings() {
    // 新しい図面は会社の設定：寸法図形、寸法線色 2・点色 3、寸法ごとにグループ -1, -2
    let mut ed = Editor::blank();
    assert!(ed.dim_fig());
    ed.dim_lead(p(0.0, 0.0), p(30.0, 0.0), 0.0, p(0.0, 10.0), None);
    ed.dim_lead(p(30.0, 0.0), p(50.0, 0.0), 0.0, p(0.0, 10.0), None);
    let kinds: Vec<&str> = ed
        .doc
        .ents
        .iter()
        .map(|e| match e.shape {
            Shape::Point(_) => "P",
            Shape::Line { .. } => "L",
            Shape::Dim(_) => "D",
            _ => "?",
        })
        .collect();
    assert_eq!(kinds.join(""), "PPLLDPPLLD");
    assert!(ed.doc.ents[..5].iter().all(|e| e.a.group == u32::MAX) && ed.doc.ents[5..].iter().all(|e| e.a.group == u32::MAX - 1));
    assert_eq!((ed.doc.ents[0].a.color, ed.doc.ents[2].a.color, ed.doc.ents[4].a.color), (3, 2, 2));
    assert_eq!(texts(&ed), vec!["3,000".to_string(), "2,000".to_string()]);
    // 保存して読み直しても寸法図形のまま
    let b = jwwview::jwwsave::write_bytes(&ed.doc);
    let d = jwwview::jww::read_bytes(&b).unwrap();
    assert_eq!(d.ents.iter().filter(|e| matches!(e.shape, Shape::Dim(_))).count(), 2);
    // 「寸法図形」を外すとばらばら
    ed.set_dim_fig(false);
    assert!(!ed.dim_fig());
    ed.dim_lead(p(0.0, 0.0), p(10.0, 0.0), 0.0, p(0.0, 10.0), None);
    assert!(!matches!(ed.doc.ents.last().unwrap().shape, Shape::Dim(_)));
}

#[test]
fn dim_more() {
    use jwwview::tools::DimKind;
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(0.0, -10.0));
    ed.line(p(12.345, 0.0), p(12.345, -10.0));
    ed.line(p(30.0, 0.0), p(30.0, -10.0));
    let mut t = Tools::default();
    t.set(Kind::Dim, &mut ed);
    // 小数桁 1、端部 矢印
    ed.dim_dec = 1;
    ed.dim_end = 1;
    t.dim_eq = false;
    l(&mut t, &mut ed, p(0.0, 10.0));
    l(&mut t, &mut ed, p(0.1, 0.1));
    l(&mut t, &mut ed, p(12.3, 0.1));
    assert_eq!(texts(&ed), vec!["1,234.5".to_string()]);
    assert!(!ed.doc.ents.iter().any(|e| matches!(e.shape, Shape::Point(_))));
    // 累進：起点 0、12.345 → 1,234.5、30 → 3,000
    t.reset(&mut ed);
    t.dim_kind = DimKind::Cumulative;
    ed.dim_dec = 2;
    l(&mut t, &mut ed, p(0.0, 20.0));
    l(&mut t, &mut ed, p(0.1, 0.1));
    l(&mut t, &mut ed, p(12.3, 0.1));
    l(&mut t, &mut ed, p(29.9, 0.1));
    let ts = texts(&ed);
    assert!(ts.contains(&"0".to_string()) && ts.contains(&"1,234.5".to_string()) && ts.contains(&"3,000".to_string()), "{ts:?}");
    // 一括処理：3 本の縦線を挟んで実行 → 2 個
    let mut ed2 = Editor::blank();
    for x in [0.0, 10.0, 25.0] {
        ed2.line(p(x, 0.0), p(x, -10.0));
    }
    t.set(Kind::Dim, &mut ed2);
    t.dim_kind = DimKind::Line;
    t.dim_eq = false;
    l(&mut t, &mut ed2, p(0.0, 10.0));
    t.dim_batch = true;
    l(&mut t, &mut ed2, p(0.0, -5.0));
    l(&mut t, &mut ed2, p(25.0, -5.0));
    assert_eq!(ed2.sel.len(), 3);
    t.dim_batch_run(&mut ed2);
    assert_eq!(texts(&ed2), vec!["1,000".to_string(), "1,500".to_string()]);
    // 寸法値：値だけ、R で移動
    let mut ed3 = Editor::blank();
    ed3.line(p(0.0, 0.0), p(20.0, 0.0));
    t.set(Kind::Dim, &mut ed3);
    t.dim_kind = DimKind::Value;
    l(&mut t, &mut ed3, p(0.1, 0.1));
    l(&mut t, &mut ed3, p(19.9, 0.1));
    assert_eq!(texts(&ed3), vec!["2,000".to_string()]);
    assert_eq!(ed3.doc.ents.len(), 2);
    let Shape::Text(t0) = ed3.doc.ents[1].shape.clone() else { panic!() };
    let c0 = mid(t0.start, t0.end);
    r(&mut t, &mut ed3, c0);
    l(&mut t, &mut ed3, p(10.0, 30.0));
    let Shape::Text(t1) = ed3.doc.ents[1].shape.clone() else { panic!() };
    assert!(dist(mid(t1.start, t1.end), p(10.0, 30.0)) < 1e-9);
    // 引出角 45°：引出線が傾く
    let mut ed4 = Editor::blank();
    t.set(Kind::Dim, &mut ed4);
    ed4.point(p(0.0, 0.0));
    ed4.point(p(10.0, 0.0));
    t.dim_kind = DimKind::Line;
    t.dim_eq = false;
    ed4.dim_lead_ang = 45.0;
    l(&mut t, &mut ed4, p(0.0, 10.0));
    l(&mut t, &mut ed4, p(0.1, 0.1));
    l(&mut t, &mut ed4, p(9.9, 0.1));
    let ls = lines(&ed4);
    assert!(ls.iter().any(|(a, b)| (dist(*a, p(0.0, 0.0)) < 1e-9 || dist(*b, p(0.0, 0.0)) < 1e-9) && ((b.x - a.x).abs() - (b.y - a.y).abs()).abs() < 1e-9 && (b.x - a.x).abs() > 1.0), "{ls:?}");
}

#[test]
fn text_link_and_ring() {
    let mut ed = Editor::blank();
    ed.text(p(0.0, 0.0), "ABC");
    ed.text(p(0.0, 20.0), "DE");
    let mut t = Tools::default();
    t.set(Kind::Text, &mut ed);
    t.text_link = true;
    // 後付け：ABC の後半を L、DE を L（移動して連結）
    let Shape::Text(a) = ed.doc.ents[0].shape.clone() else { panic!() };
    l(&mut t, &mut ed, lerp(a.start, a.end, 0.9));
    l(&mut t, &mut ed, p(1.0, 21.0));
    assert_eq!(texts(&ed), vec!["ABCDE".to_string()]);
    // 切断：R で 3 文字目の後ろ
    let Shape::Text(a) = ed.doc.ents[0].shape.clone() else { panic!() };
    r(&mut t, &mut ed, lerp(a.start, a.end, 0.6));
    assert_eq!(texts(&ed), vec!["ABC".to_string(), "DE".to_string()]);
    let Shape::Text(d) = ed.doc.ents[1].shape.clone() else { panic!() };
    // 後ろの文字は前の文字の続きの位置から（3 文字分右）
    let Shape::Text(h) = ed.doc.ents[0].shape.clone() else { panic!() };
    assert!(d.start.x > h.start.x && d.start.x < a.end.x && (d.start.y - a.start.y).abs() < 1e-9);
    assert!(dist(h.start, a.start) < 1e-9);
    // LL = 移動
    ll(&mut t, &mut ed, p(1.0, 1.0));
    l(&mut t, &mut ed, p(50.0, 50.0));
    let Shape::Text(m) = ed.doc.ents[0].shape.clone() else { panic!() };
    assert!(dist(m.start, p(50.0, 50.0)) < 1e-9);

    // 円環ソリッド：半径 10mm、内側 500（5mm）
    let mut ed = Editor::blank();
    ed.circle(p(0.0, 0.0), 1000.0);
    t.set(Kind::Solid, &mut ed);
    t.solid_from = true;
    t.ring = true;
    t.ring_r = "500".into();
    l(&mut t, &mut ed, p(10.0, 0.0));
    let sol: Vec<[P; 4]> = ed.doc.ents.iter().filter_map(|e| if let Shape::Solid { pts, .. } = e.shape { Some(pts) } else { None }).collect();
    assert!(!sol.is_empty());
    assert!(sol.iter().flatten().all(|q| { let d = len(*q); d > 5.0 - 1e-9 && d < 10.0 + 1e-9 }));
}

#[test]
fn dim_keeps_going() {
    // 寸法：始点・終点の後、R で次々と終点 = 前の終点から連続
    let mut ed = Editor::blank();
    for x in [0.0, 10.0, 25.0, 40.0] {
        ed.line(p(x, 0.0), p(x, -10.0));
    }
    let mut t = Tools::default();
    t.set(Kind::Dim, &mut ed);
    t.dim_eq = false;
    l(&mut t, &mut ed, p(0.0, 10.0));
    for x in [0.0, 10.0, 25.0, 40.0] {
        r(&mut t, &mut ed, p(x, 0.0));
    }
    assert_eq!(texts(&ed), vec!["1,000".to_string(), "1,500".to_string(), "1,500".to_string()]);
}

#[test]
fn dim_space_flips_0_90() {
    use jwwview::edit::DimDir;
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    t.set(Kind::Dim, &mut ed);
    assert!(t.dim_dir == DimDir::Horizontal);
    t.space();
    assert!(t.dim_dir == DimDir::Vertical);
    t.space();
    assert!(t.dim_dir == DimDir::Horizontal);
}

#[test]
fn pen_corner_smooth_close() {
    use jwwview::tools::{pen_points, Anchor};
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    t.set(Kind::Pen, &mut ed);
    // 角 → 滑らか（ドラッグ） → Enter
    l(&mut t, &mut ed, p(0.0, 0.0));
    t.pen_drag(&mut ed, p(10.0, 0.0), p(15.0, 5.0), false, TOL);
    assert_eq!(t.pen.len(), 2);
    assert!(dist(t.pen[1].h_in, p(5.0, -5.0)) < 1e-9);
    t.pen_div = 8;
    t.pen_finish(&mut ed, false);
    let ls = lines(&ed);
    assert_eq!(ls.len(), 8);
    assert!(dist(ls[0].0, p(0.0, 0.0)) < 1e-9 && dist(ls[7].1, p(10.0, 0.0)) < 1e-9);
    // 同じ曲線属性
    let g = ed.doc.ents[0].a.group;
    assert!(g > 0 && ed.doc.ents.iter().all(|e| e.a.group == g));
    assert!(t.pen.is_empty());
    // 角だけ 3 点 → 始点クリックで閉じる = 直線 3 本
    let mut ed2 = Editor::blank();
    t.set(Kind::Pen, &mut ed2);
    l(&mut t, &mut ed2, p(0.0, 0.0));
    l(&mut t, &mut ed2, p(10.0, 0.0));
    l(&mut t, &mut ed2, p(10.0, 10.0));
    l(&mut t, &mut ed2, p(0.1, 0.1));
    assert_eq!(lines(&ed2).len(), 3);
    assert!(has(&ed2, p(10.0, 10.0), p(0.0, 0.0)));
    // BackSpace
    l(&mut t, &mut ed2, p(0.0, 0.0));
    l(&mut t, &mut ed2, p(5.0, 0.0));
    t.pen_back();
    assert_eq!(t.pen.len(), 1);
    // 曲線の途中点
    let v = [Anchor { p: p(0.0, 0.0), h_in: p(0.0, 0.0), h_out: p(0.0, 10.0) }, Anchor { p: p(10.0, 0.0), h_in: p(10.0, 10.0), h_out: p(10.0, 0.0) }];
    let pts = pen_points(&v, false, 2);
    assert!(dist(pts[1], p(5.0, 7.5)) < 1e-9);
}
