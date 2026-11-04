//! 照用户的操作顺序点鼠标（左键 = 任意点，右键 = 读取点），检查作图命令的流程。
//! 缩尺 1/100：实寸 5000 = 图寸 50mm。

use jwwview::cad::*;
use jwwview::edit::Editor;
use jwwview::jww::{Shape, P};
use jwwview::tools::{Axis, Kind, Tools};

const TOL: f64 = 0.5;

fn left(t: &mut Tools, ed: &mut Editor, q: P) {
    t.click(ed, q, false, None, TOL);
}
fn right(t: &mut Tools, ed: &mut Editor, q: P) {
    let r = ed.read_point(q, TOL);
    t.click(ed, q, true, r, TOL);
}
fn lines(ed: &Editor) -> Vec<(P, P)> {
    ed.doc.ents.iter().filter_map(|e| if let Shape::Line { a, b } = e.shape { Some((a, b)) } else { None }).collect()
}
fn has(ed: &Editor, a: P, b: P) -> bool {
    lines(ed).iter().any(|(x, y)| (dist(*x, a) < 1e-6 && dist(*y, b) < 1e-6) || (dist(*x, b) < 1e-6 && dist(*y, a) < 1e-6))
}

#[test]
fn draw_triangle_and_rect_by_clicks() {
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    t.set(Kind::Line, &mut ed);
    // ① 水平 5000：寸法を入れて、始点クリック → 右の方へクリック
    t.hv = true;
    t.len = "5000".into();
    left(&mut t, &mut ed, p(0.0, 0.0));
    left(&mut t, &mut ed, p(30.0, 2.0));
    assert!(has(&ed, p(0.0, 0.0), p(50.0, 0.0)));
    // ② 右端（読取点）から上へ 5000
    right(&mut t, &mut ed, p(49.8, 0.2));
    left(&mut t, &mut ed, p(51.0, 20.0));
    assert!(has(&ed, p(50.0, 0.0), p(50.0, 50.0)));
    // ③ 寸法を消して、両端を読取点でつなぐ
    t.hv = false;
    t.len.clear();
    right(&mut t, &mut ed, p(0.1, 0.1));
    right(&mut t, &mut ed, p(50.1, 49.9));
    assert!(has(&ed, p(0.0, 0.0), p(50.0, 50.0)));
    // ④ 1割5分
    t.angle = "1割5分".into();
    t.len = "3000".into();
    right(&mut t, &mut ed, p(50.0, 50.2));
    left(&mut t, &mut ed, p(80.0, 52.0));
    let end = polar(p(50.0, 50.0), 30.0, 0.15f64.atan());
    assert!(has(&ed, p(50.0, 50.0), end));

    // ⑤ 矩形 5000×10000、右上 = 直角点：基準点を読取って、左下の方へマウス
    t.set(Kind::Rect, &mut ed);
    t.rect_size = "5000,10000".into();
    right(&mut t, &mut ed, p(50.2, 0.1));
    left(&mut t, &mut ed, p(20.0, -30.0));
    assert!(has(&ed, p(0.0, -100.0), p(50.0, -100.0)));
    assert!(has(&ed, p(0.0, 0.0), p(0.0, -100.0)) || has(&ed, p(0.0, -100.0), p(0.0, 0.0)));

    // 戻る
    let n = ed.doc.ents.len();
    ed.undo();
    assert_eq!(ed.doc.ents.len(), n - 4);
    ed.redo();
    assert_eq!(ed.doc.ents.len(), n);
}

#[test]
fn offset_corner_stretch_by_clicks() {
    let mut ed = Editor::blank();
    ed.rect(p(0.0, -100.0), p(50.0, 0.0));
    let mut t = Tools::default();
    // 複線 100：左辺を選んで内側をクリック
    t.set(Kind::Offset, &mut ed);
    t.offset = "100".into();
    left(&mut t, &mut ed, p(0.0, -50.0));
    left(&mut t, &mut ed, p(10.0, -50.0));
    assert!(has(&ed, p(1.0, -100.0), p(1.0, 0.0)) || has(&ed, p(1.0, 0.0), p(1.0, -100.0)));
    // 上辺も
    left(&mut t, &mut ed, p(25.0, 0.0));
    left(&mut t, &mut ed, p(25.0, -10.0));
    // コーナー
    t.set(Kind::Corner, &mut ed);
    left(&mut t, &mut ed, p(1.0, -50.0));
    left(&mut t, &mut ed, p(25.0, -1.0));
    assert!(has(&ed, p(1.0, -1.0), p(1.0, -100.0)) || has(&ed, p(1.0, -100.0), p(1.0, -1.0)));
    // 伸縮：底辺を下の線（新しく引く）まで
    ed.line(p(-10.0, -120.0), p(60.0, -120.0));
    t.set(Kind::Stretch, &mut ed);
    left(&mut t, &mut ed, p(1.0, -50.0)); // 伸ばす線（上側を残す）
    left(&mut t, &mut ed, p(30.0, -120.0)); // 基準線
    assert!(has(&ed, p(1.0, -1.0), p(1.0, -120.0)) || has(&ed, p(1.0, -120.0), p(1.0, -1.0)));
    // 伸縮点（右クリック）：その線を左端 x=-10 まで
    left(&mut t, &mut ed, p(30.0, -120.0));
    right(&mut t, &mut ed, p(-10.0, -120.0));
    // 端点そのものなので変化なし（線内）→ 長さ不変でも壊れない
    assert!(lines(&ed).iter().any(|(a, b)| (a.y + 120.0).abs() < 1e-9 && (b.y + 120.0).abs() < 1e-9));
}

#[test]
fn copy_with_axis_lock() {
    let mut ed = Editor::blank();
    ed.rect(p(0.0, 0.0), p(10.0, 10.0));
    let mut t = Tools::default();
    t.set(Kind::Copy, &mut ed);
    // 範囲：2 点
    left(&mut t, &mut ed, p(-1.0, -1.0));
    left(&mut t, &mut ed, p(11.0, 11.0));
    assert_eq!(ed.sel.len(), 4);
    t.confirm(&mut ed);
    t.axis = Axis::X;
    // 基点は中心 (5,5)。左へ、少し上下にずれてクリックしても y は固定
    left(&mut t, &mut ed, p(-25.0, 7.0));
    assert!(has(&ed, p(-30.0, 0.0), p(-20.0, 0.0)));
    assert_eq!(lines(&ed).len(), 8);
    // 続けてもう 1 つ
    left(&mut t, &mut ed, p(-55.0, 3.0));
    assert_eq!(lines(&ed).len(), 12);
}

#[test]
fn erase_modes_by_clicks() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(30.0, 0.0));
    ed.line(p(10.0, -5.0), p(10.0, 5.0));
    ed.line(p(20.0, -5.0), p(20.0, 5.0));
    let mut t = Tools::default();
    t.set(Kind::Erase, &mut ed);
    // 節間消し
    t.between = true;
    left(&mut t, &mut ed, p(15.0, 0.0));
    assert!(has(&ed, p(0.0, 0.0), p(10.0, 0.0)) && has(&ed, p(20.0, 0.0), p(30.0, 0.0)));
    // 部分消し：左クリックで線、点を 2 つ（Jw_cad：L = 部分消し）
    t.between = false;
    left(&mut t, &mut ed, p(10.0, 3.0));
    left(&mut t, &mut ed, p(10.0, 1.0));
    left(&mut t, &mut ed, p(10.0, 4.0));
    assert!(has(&ed, p(10.0, -5.0), p(10.0, 1.0)) && has(&ed, p(10.0, 4.0), p(10.0, 5.0)));
    // 図形消去は右クリック
    let n = ed.doc.ents.len();
    right(&mut t, &mut ed, p(20.0, -4.0));
    assert_eq!(ed.doc.ents.len(), n - 1);
    // 部分消しで同じ点を２回 = 切断
    let n = ed.doc.ents.len();
    left(&mut t, &mut ed, p(25.0, 0.0));
    left(&mut t, &mut ed, p(25.0, 0.0));
    left(&mut t, &mut ed, p(25.0, 0.0));
    assert_eq!(ed.doc.ents.len(), n + 1);
    assert!(has(&ed, p(20.0, 0.0), p(25.0, 0.0)) && has(&ed, p(25.0, 0.0), p(30.0, 0.0)));
}

#[test]
fn dim_and_text_by_clicks() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(50.0, 0.0));
    ed.line(p(50.0, 0.0), p(50.0, 50.0));
    let mut t = Tools::default();
    ed.line(p(50.0, 50.0), p(80.0, 50.0));
    t.set(Kind::Dim, &mut ed);
    left(&mut t, &mut ed, p(0.0, 5.0)); // 引出線の始点位置（＝）
    left(&mut t, &mut ed, p(0.0, 10.0)); // 寸法線の位置
    // 測点は左クリックでも読取点
    left(&mut t, &mut ed, p(0.1, 0.1));
    left(&mut t, &mut ed, p(49.9, 0.1));
    // 右クリックで終点だけ = 前の終点から続ける
    right(&mut t, &mut ed, p(79.9, 49.9));
    // 左クリック = 新しい始点 → 終点
    left(&mut t, &mut ed, p(0.1, 0.1));
    left(&mut t, &mut ed, p(79.8, 50.1));
    // 寸法図形（会社の設定）の中の寸法値
    let texts: Vec<String> = ed
        .doc
        .ents
        .iter()
        .filter_map(|e| match &e.shape {
            Shape::Dim(d) => match &d.text.shape {
                Shape::Text(t) => Some(t.s.clone()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["5,000".to_string(), "3,000".to_string(), "8,000".to_string()]);
    // 引出線は寸法線（y=10）から引出線の始点（y=5）まで
    assert!(lines(&ed).iter().any(|(a, b)| a.x.abs() < 1e-9 && (a.y - 10.0).abs() < 1e-9 && (b.y - 5.0).abs() < 1e-9));
    // 何もない所の左クリックは測点にならない
    let n = ed.doc.ents.len();
    left(&mut t, &mut ed, p(30.0, 30.0));
    left(&mut t, &mut ed, p(31.0, 31.0));
    assert_eq!(ed.doc.ents.len(), n);
    t.set(Kind::Text, &mut ed);
    t.text = "平林シート".into();
    left(&mut t, &mut ed, p(10.0, 20.0));
    assert!(ed.doc.ents.iter().any(|e| matches!(&e.shape, Shape::Text(x) if x.s == "平林シート")));
}

#[test]
fn range_then_copy_or_erase() {
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.line(p(0.0, 5.0), p(10.0, 5.0));
    ed.line(p(50.0, 0.0), p(60.0, 0.0));
    // Y = 範囲選択：框选左边两条
    assert!(t.command(33, &mut ed));
    left(&mut t, &mut ed, p(-1.0, -1.0));
    left(&mut t, &mut ed, p(11.0, 6.0));
    assert_eq!(ed.sel.len(), 2);
    // C = 複写：选好的直接带过去，基准点默认是选中图形的中心 (5, 2.5)，点一下就复写
    assert!(t.command(16, &mut ed));
    assert_eq!(ed.sel.len(), 2);
    t.click(&mut ed, p(5.0, 22.5), false, None, TOL);
    assert!(has(&ed, p(0.0, 20.0), p(10.0, 20.0)) && has(&ed, p(0.0, 25.0), p(10.0, 25.0)));
    // 範囲 → D = 消去：马上消掉
    t.command(33, &mut ed);
    left(&mut t, &mut ed, p(49.0, -1.0));
    left(&mut t, &mut ed, p(61.0, 1.0));
    t.command(15, &mut ed);
    assert!(!has(&ed, p(50.0, 0.0), p(60.0, 0.0)));
    assert_eq!(lines(&ed).len(), 4);
}

#[test]
fn pick_values_into_fields() {
    use jwwview::tools::Pick;
    let mut ed = Editor::blank(); // 1/100：图寸 1mm = 实寸 100
    let mut t = Tools::default();
    ed.line(p(0.0, 0.0), p(30.0, 40.0)); // 长 50mm = 5000，角度 53.13°
    ed.line(p(0.0, 10.0), p(100.0, 10.0));
    ed.line(p(0.0, 25.0), p(100.0, 25.0));
    t.set(Kind::Line, &mut ed);
    t.start_pick(Pick::LineLen);
    left(&mut t, &mut ed, p(15.0, 20.0));
    assert_eq!(t.len, "5000");
    t.start_pick(Pick::LineAngle);
    left(&mut t, &mut ed, p(15.0, 20.0));
    assert_eq!(t.angle, "53.1301");
    t.start_pick(Pick::PtLen);
    right(&mut t, &mut ed, p(0.1, 0.1));
    right(&mut t, &mut ed, p(99.9, 10.1));
    assert_eq!(t.len, "10049.8756");
    // 間隔 → 複線間隔：两条平行线相距 15mm = 1500
    t.set(Kind::Offset, &mut ed);
    assert!(t.command(95, &mut ed));
    left(&mut t, &mut ed, p(50.0, 10.0));
    left(&mut t, &mut ed, p(50.0, 25.0));
    assert_eq!(t.offset, "1500");
    assert!(t.pick.is_none());
    // 2点角 → 寸法の傾き
    t.set(Kind::Dim, &mut ed);
    t.start_pick(Pick::PtAngle);
    left(&mut t, &mut ed, p(0.0, 0.0));
    left(&mut t, &mut ed, p(10.0, 10.0));
    assert_eq!(t.dim_dir, jwwview::edit::DimDir::Angle(45.0));
}

#[test]
fn clip_copy_paste() {
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    ed.set_write(0, 3);
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.line(p(0.0, 0.0), p(0.0, 10.0));
    // 範囲 → コピー
    t.command(33, &mut ed);
    left(&mut t, &mut ed, p(-1.0, -1.0));
    left(&mut t, &mut ed, p(11.0, 11.0));
    assert!(t.command(75, &mut ed));
    assert_eq!(t.clip.len(), 2);
    // 換一张新图纸贴（剪贴板跟着 Tools 走）：基准点 = 中心 (5,5)
    let mut ed2 = Editor::blank();
    ed2.set_write(0, 7);
    assert!(t.command(34, &mut ed2));
    left(&mut t, &mut ed2, p(105.0, 5.0));
    assert!(has(&ed2, p(100.0, 0.0), p(110.0, 0.0)) && has(&ed2, p(100.0, 0.0), p(100.0, 10.0)));
    assert!(ed2.doc.ents.iter().all(|e| e.a.layer == 3)); // 默认保持原图层
    t.paste_to_write = true;
    left(&mut t, &mut ed2, p(205.0, 5.0));
    assert_eq!(ed2.doc.ents.iter().filter(|e| e.a.layer == 7).count(), 2);
    assert!(ed2.undo());
    assert_eq!(ed2.doc.ents.len(), 2);
}
