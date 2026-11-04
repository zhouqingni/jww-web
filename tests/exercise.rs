//! 验收题（docs/stage2.md）按题号一步步用编辑命令做，存成 .jww 再读回来核对。
//! 缩尺 1/100：实寸 5000 = 图寸 50mm。

use jwwview::cad::*;
use jwwview::edit::{DimDir, Editor, PolyMode};
use jwwview::jww::{self, Shape, P};
use jwwview::jwwsave;

fn near(a: P, b: P) -> bool {
    dist(a, b) < 1e-6
}
fn seg(ed: &Editor, i: usize) -> (P, P) {
    match ed.doc.ents[i].shape {
        Shape::Line { a, b } => (a, b),
        ref s => panic!("{i} 不是线：{s:?}"),
    }
}
fn has_line(ed: &Editor, a: P, b: P) -> bool {
    ed.doc.ents.iter().any(|e| match e.shape {
        Shape::Line { a: x, b: y } => (near(x, a) && near(y, b)) || (near(x, b) && near(y, a)),
        _ => false,
    })
}
fn find_line(ed: &Editor, a: P, b: P) -> usize {
    ed.doc
        .ents
        .iter()
        .position(|e| match e.shape {
            Shape::Line { a: x, b: y } => (near(x, a) && near(y, b)) || (near(x, b) && near(y, a)),
            _ => false,
        })
        .unwrap_or_else(|| panic!("找不到线 {a:?}-{b:?}"))
}

pub fn run() -> Editor {
    let mut ed = Editor::blank();
    assert_eq!(ed.scale(), 100.0);

    // 直線（レイヤー２）
    ed.set_write(0, 2);
    ed.begin();
    let l1 = ed.line(p(0.0, 0.0), p(ed.paper(5000.0), 0.0)); // ①
    let l2 = ed.line(p(50.0, 0.0), p(50.0, 50.0)); // ②
    let l3 = ed.line(p(0.0, 0.0), p(50.0, 50.0)); // ③
    let a4 = rad(parse_angle("1割5分").unwrap());
    let l4 = ed.line(p(50.0, 50.0), polar(p(50.0, 50.0), ed.paper(3000.0), a4)); // ④
    assert!((parse_angle("//0.15").unwrap() - deg(0.15f64.atan())).abs() < 1e-12);
    let _ = (l1, l2, l3);

    // 四角（レイヤー１）
    ed.set_write(0, 1);
    let r5 = ed.rect_size(p(50.0, 0.0), 5000.0, 10000.0, (1, 1)); // ⑤ 右上 = 直角点
    assert!(has_line(&ed, p(0.0, -100.0), p(50.0, -100.0)));
    let r6 = ed.rect_size(p(50.0, -100.0), 200.0, 200.0, (1, -1)); // ⑥ 右下内部
    assert!(has_line(&ed, p(48.0, -100.0), p(48.0, -98.0)));
    let (top5, right5, bottom5, left5) = (r5[2], r5[1], r5[0], r5[3]);
    assert!(near(seg(&ed, bottom5).0, p(0.0, -100.0)));
    let _ = r6;

    // 円（レイヤー３）
    ed.set_write(0, 3);
    let c7 = ed.circle(p(50.0, 0.0), 250.0); // ⑦
    let c8 = ed.circle3(p(0.0, 0.0), p(50.0, 0.0), p(50.0, 50.0)).unwrap(); // ⑧
    let Some(Curve::Circ { c: o8, r: r8, .. }) = ed.curve(c8) else { panic!() };
    assert!(near(o8, p(25.0, 25.0)) && (r8 - 25.0 * 2f64.sqrt()).abs() < 1e-9);

    // 複線（レイヤー２）
    ed.set_write(0, 2);
    let c = p(25.0, -50.0); // ⑤ 的中间
    let o9: Vec<usize> = [top5, right5, bottom5, left5].iter().map(|&i| ed.offset(i, 100.0, c).unwrap()).collect(); // ⑨
    assert!(has_line(&ed, p(50.0, -1.0), p(0.0, -1.0)));
    let d1 = ed.offset(bottom5, 1000.0, p(0.0, -200.0)).unwrap(); // ⑩
    let d2 = ed.offset(d1, 1000.0, p(0.0, -200.0)).unwrap();
    assert!((seg(&ed, d1).0.y + 110.0).abs() < 1e-9 && (seg(&ed, d2).0.y + 120.0).abs() < 1e-9);

    // 二線（レイヤー１）
    ed.set_write(0, 1);
    let t11 = ed.two_lines(l4, 50.0, 50.0); // ⑪
    assert_eq!(t11.len(), 2);

    // 中心線（レイヤー０）
    ed.set_write(0, 0);
    let cl = ed.center_of_lines(top5, bottom5, p(10.0, 0.0), p(10.0, -100.0)).unwrap(); // ⑫
    let c12 = ed.center_line(cl, p(0.0, 0.0), p(50.0, -100.0));
    assert!(near(seg(&ed, c12).0, p(0.0, -50.0)) && near(seg(&ed, c12).1, p(50.0, -50.0)));
    let cl = Editor::center_of_points(p(0.0, -100.0), p(50.0, 0.0)); // ⑬
    ed.center_line(cl, p(0.0, 0.0), p(50.0, -100.0));
    let (a, b) = seg(&ed, l4);
    let cl = Editor::center_of_points(a, b); // ⑭
    ed.center_line(cl, add(mid(a, b), p(0.0, 10.0)), sub(mid(a, b), p(0.0, 10.0)));

    // 分割（レイヤー０）
    let div = ed.divide_lines(left5, right5, 5); // ⑮
    assert_eq!(div.len(), 4);
    assert!((seg(&ed, div[0]).0.x - 10.0).abs() < 1e-9);

    // コーナー ⑯：⑨的左线和上线
    let (top9, left9) = (o9[0], o9[3]);
    assert!(ed.corner(left9, p(1.0, -50.0), top9, p(25.0, -1.0)));
    assert!(has_line(&ed, p(1.0, -1.0), p(1.0, -100.0)) || has_line(&ed, p(1.0, -100.0), p(1.0, -1.0)));
    assert!(has_line(&ed, p(1.0, -1.0), p(50.0, -1.0)));

    // 接線（レイヤー３）⑰ ⑦和⑧的上面
    ed.set_write(0, 3);
    let o8 = p(25.0, 25.0);
    let t17 = ed.tangent_cc(c7, p(50.0, 2.5), c8, add(o8, p(0.0, r8))).unwrap();
    let (ta, tb) = seg(&ed, t17);
    assert!((dist(ta, p(50.0, 0.0)) - 2.5).abs() < 1e-9 && (dist(tb, o8) - r8).abs() < 1e-9);

    // 接円 ⑱ ⑤的左边和上边、Φ500
    let c18 = ed.tangent_circle_ll(left5, top5, 250.0, p(3.0, -3.0)).unwrap();
    let Some(Curve::Circ { c: o18, .. }) = ed.curve(c18) else { panic!() };
    assert!(near(o18, p(2.5, -2.5)));

    // 伸縮 ⑲：⑮的 4 根到⑩上面的线
    for &i in &div {
        assert!(ed.stretch(i, p(seg(&ed, i).0.x, -50.0), d1));
    }
    assert!(has_line(&ed, p(10.0, -110.0), p(10.0, 0.0)) || has_line(&ed, p(10.0, 0.0), p(10.0, -110.0)));
    // ⑳ ⑩上面的线伸到最左边的点（⑬中心线的左端）
    let leftmost = ed
        .doc
        .ents
        .iter()
        .filter_map(|e| if let Shape::Line { a, b } = e.shape { Some([a, b]) } else { None })
        .flatten()
        .min_by(|u, v| u.x.total_cmp(&v.x))
        .unwrap();
    assert!(ed.stretch_to_point(d1, p(25.0, -110.0), leftmost));
    assert!((seg(&ed, d1).0.x.min(seg(&ed, d1).1.x) - leftmost.x).abs() < 1e-9);
    // ㉑ 一括処理：⑮全部到⑩下面的线
    let n = ed.stretch_batch(d2, p(5.0, -60.0), p(45.0, -60.0));
    assert!(n >= 4, "一括处理只处理了 {n} 根");
    for &i in &div {
        let (a, b) = seg(&ed, i);
        assert!((a.y.min(b.y) + 120.0).abs() < 1e-9, "⑮ 没伸到 -120：{a:?} {b:?}");
    }

    // 面取（レイヤー２）㉒ ⑤左下角丸面 200
    ed.set_write(0, 2);
    let f = ed.chamfer(left5, p(0.0, -50.0), bottom5, p(25.0, -100.0), 200.0, true).unwrap();
    let Some(Curve::Circ { c: fc, r: fr, sweep, .. }) = ed.curve(f) else { panic!() };
    assert!(near(fc, p(2.0, -98.0)) && (fr - 2.0).abs() < 1e-9 && (sweep - std::f64::consts::FRAC_PI_2).abs() < 1e-9);

    // 多角形（レイヤー５）㉓ 円⑧の下 500 の六角形
    ed.set_write(0, 5);
    let hc = p(25.0, -20.0);
    let hex = ed.polygon(hc, 6, 500.0, PolyMode::Edge, 90.0);
    let (ha, hb) = seg(&ed, hex[0]);
    assert!((dist(ha, hb) - 5.0).abs() < 1e-9);
    // 点 ㉔
    let verts: Vec<P> = hex.iter().map(|&i| seg(&ed, i).0).collect();
    for v in &verts {
        ed.point(*v);
    }

    // 属取・属変 ㉕
    ed.take_attr(c7);
    assert_eq!(ed.lay(), 3);
    ed.change_attr(&hex, true, true, true);
    assert!(hex.iter().all(|&i| ed.doc.ents[i].a.layer == 3));

    // 消去 ㉖ ⑩下面的线
    ed.erase(vec![d2]);
    // ⑮的线已经伸到 -120，下面那根线消掉之后它们还在
    // ㉗ ⑥里面多余的线：⑨的右线・下线穿过⑥，用節間消し消掉⑥里面那一节
    let right9 = find_line(&ed, p(49.0, 0.0), p(49.0, -100.0));
    let before = ed.doc.ents.len();
    assert!(ed.erase_between(right9, p(49.0, -99.5)));
    assert!(ed.doc.ents.len() >= before);

    // 寸法（レイヤー７）㉘
    ed.set_write(0, 7);
    let c8 = ed
        .doc
        .ents
        .iter()
        .position(|e| matches!(Curve::of(e), Some(Curve::Circ { c, .. }) if near(c, p(25.0, 25.0))))
        .unwrap();
    let c7 = ed
        .doc
        .ents
        .iter()
        .position(|e| matches!(Curve::of(e), Some(Curve::Circ { c, r, .. }) if near(c, p(50.0, 0.0)) && (r - 2.5).abs() < 1e-9))
        .unwrap();
    ed.dim_radius(c8, p(60.0, 60.0), true);
    ed.dim_radius(c7, p(60.0, 10.0), false);
    let texts: Vec<String> =
        ed.doc.ents.iter().filter_map(|e| if let Shape::Text(t) = &e.shape { Some(t.s.clone()) } else { None }).collect();
    assert!(texts.contains(&"φ7,071.07".to_string()), "{texts:?}");
    assert!(texts.contains(&"R250".to_string()));
    // ㉙ ⑤⑮全部、上へ（二重）・右へ
    let xs = [0.0, 10.0, 20.0, 30.0, 40.0, 50.0].map(|x| p(x, 0.0));
    ed.dim_chain(&xs, DimDir::Horizontal, p(0.0, 10.0), Some(8.0));
    ed.dim(p(50.0, 0.0), p(50.0, -100.0), DimDir::Vertical, p(60.0, 0.0));
    let texts: Vec<String> =
        ed.doc.ents.iter().filter_map(|e| if let Shape::Text(t) = &e.shape { Some(t.s.clone()) } else { None }).collect();
    assert_eq!(texts.iter().filter(|s| *s == "1,000").count(), 5);
    assert!(texts.contains(&"5,000".to_string()) && texts.contains(&"10,000".to_string()));
    // ㉚ 線角：④的长度
    let (a, b) = seg(&ed, l4);
    ed.dim(a, b, DimDir::Angle(deg(angle_of(sub(b, a)))), add(a, p(0.0, 5.0)));
    let texts: Vec<String> =
        ed.doc.ents.iter().filter_map(|e| if let Shape::Text(t) = &e.shape { Some(t.s.clone()) } else { None }).collect();
    assert!(texts.contains(&"3,000".to_string()), "{texts:?}");

    // 文字（レイヤー８）㉛〜㉞
    ed.set_write(0, 8);
    ed.moji_kind = 3;
    let t31 = ed.text(sub(hc, p(5.0, 1.0)), "平林シート"); // ㉛
    assert!(ed.set_text(t31, "平林シー")); // ㉜
    if let Shape::Text(t) = &ed.doc.ents[t31].shape {
        assert!((dist(t.start, t.end) - (4.0 * 3.0 + 3.0 * 0.5)).abs() < 1e-9);
    }
    ed.move_(&[t31], sub(p(20.0, 25.0), sub(hc, p(5.0, 1.0)))); // ㉝ 移到⑧的圆
    let t34 = ed.text(sub(hc, p(5.0, 1.0)), "平林シート"); // ㉞

    // 範囲・複写・移動 ㉟〜㊲
    let sel = ed.select_window(p(15.0, -30.0), p(35.0, -10.0));
    let hex_only: Vec<usize> = sel.iter().copied().filter(|&i| matches!(ed.doc.ents[i].shape, Shape::Line { .. })).collect();
    assert_eq!(hex_only.len(), 6, "范围里的线：{hex_only:?}");
    let c35 = ed.copy(&hex_only, p(-30.0, 0.0)); // ㉟ X 轴固定（y 不变）往左
    let mut group = hex_only.clone();
    group.push(t34);
    let c36a = ed.copy(&group, p(0.0, -40.0)); // ㊱ Y 轴固定往下 2 个
    let _c36b = ed.copy(&group, p(0.0, -80.0));
    let lines36: Vec<usize> = c36a.iter().copied().filter(|&i| matches!(ed.doc.ents[i].shape, Shape::Line { .. })).collect();
    ed.move_(&lines36, p(-30.0, 40.0)); // ㊲ 移到㉟的位置
    for (&i, &j) in lines36.iter().zip(&c35) {
        let (a, b) = seg(&ed, i);
        let (c, d) = seg(&ed, j);
        assert!(near(a, c) && near(b, d));
    }

    // 撤销・重做
    let n = ed.doc.ents.len();
    ed.begin();
    ed.line(p(0.0, 0.0), p(1.0, 1.0));
    assert!(ed.undo());
    assert_eq!(ed.doc.ents.len(), n);
    assert!(ed.redo());
    assert_eq!(ed.doc.ents.len(), n + 1);
    ed.undo();
    ed
}

#[test]
fn exercise_save_and_reload() {
    let ed = run();
    let bytes = jwwsave::write_bytes(&ed.doc);
    let d2 = jww::read_bytes(&bytes).expect("存的 .jww 读不回来");
    assert_eq!(d2.ents.len(), ed.doc.ents.len());
    assert_eq!(jwwsave::write_bytes(&d2), bytes, "读回来再存不一致");
    // 文字（日文）也原样
    assert!(d2.ents.iter().any(|e| matches!(&e.shape, Shape::Text(t) if t.s == "平林シー")));
    // 各图层都有东西
    for l in [0, 1, 2, 3, 7, 8] {
        assert!(d2.ents.iter().any(|e| e.a.layer == l), "图层 {l} 是空的");
    }
    // 存一份给人看（云端测试时在 target/ 下）
    let out = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/exercise.jww");
    let _ = std::fs::write(out, &bytes);
}

#[test]
fn erase_parts() {
    let mut ed = Editor::blank();
    let i = ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.erase_part(i, p(3.0, 1.0), p(6.0, -1.0));
    assert!(has_line(&ed, p(0.0, 0.0), p(3.0, 0.0)) && has_line(&ed, p(6.0, 0.0), p(10.0, 0.0)));
    // 圆的部分消し：剩下的是圆弧
    let c = ed.circle(p(0.0, 20.0), 500.0);
    ed.erase_part(c, p(5.0, 20.0), p(0.0, 25.0));
    let Some(Curve::Circ { full, sweep, .. }) = ed.curve(c) else { panic!() };
    assert!(!full && (sweep - 1.5 * std::f64::consts::PI).abs() < 1e-9);
    // 節間消し：十字的中间一节
    let mut ed = Editor::blank();
    let h = ed.line(p(0.0, 0.0), p(30.0, 0.0));
    ed.line(p(10.0, -5.0), p(10.0, 5.0));
    ed.line(p(20.0, -5.0), p(20.0, 5.0));
    ed.erase_between(h, p(15.0, 0.0));
    assert!(has_line(&ed, p(0.0, 0.0), p(10.0, 0.0)) && has_line(&ed, p(20.0, 0.0), p(30.0, 0.0)));
    assert!(!has_line(&ed, p(0.0, 0.0), p(30.0, 0.0)));
}

#[test]
fn read_point_snaps() {
    let mut ed = Editor::blank();
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    ed.line(p(5.0, -5.0), p(5.0, 5.0));
    assert!(near(ed.read_point(p(9.8, 0.1), 1.0).unwrap(), p(10.0, 0.0)));
    assert!(near(ed.read_point(p(5.2, 0.3), 1.0).unwrap(), p(5.0, 0.0)));
    assert!(ed.read_point(p(2.0, 3.0), 1.0).is_none());
}

#[test]
fn layer_names_saved() {
    let mut ed = Editor::blank();
    ed.doc.glayers[0].lay_name[2] = "通り芯".into();
    ed.doc.glayers[1].name = "平面図".into();
    ed.line(p(0.0, 0.0), p(1.0, 1.0));
    let d = jww::read_bytes(&jwwsave::write_bytes(&ed.doc)).unwrap();
    assert_eq!(d.glayers[0].lay_name[2], "通り芯");
    assert_eq!(d.glayers[1].name, "平面図");
    assert_eq!(d.memo, "");
}
