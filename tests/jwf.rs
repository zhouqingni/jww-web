//! 公司的环境设定（assets/company.jwf = 1カラー書き込み用zhou2.jwf）

use jwwview::cad::p;
use jwwview::edit::Editor;
use jwwview::jwf::Env;
use jwwview::jww;
use jwwview::jwwsave;
use jwwview::tools::{Kind, Tools};

#[test]
fn company_env_parsed() {
    let e = Env::company();
    // KEY 页：H = 線、A = 文字 / Shift+A = AUTO、D = 消去、スペース = 76
    assert_eq!(e.keys[(b'H' - b'A') as usize], (2, 0));
    assert_eq!(e.keys[0], (5, 1));
    assert_eq!(e.keys[(b'D' - b'A') as usize], (15, 4));
    assert_eq!(e.space, 76);
    assert_eq!(e.fkeys[7], 100); // F9 = 保存
    // 图层组 [A] S=1/100，[B] S=1/150
    assert_eq!(e.glayer_names[10], "S=1/100");
    assert_eq!(e.scales[10], 100.0);
    assert_eq!(e.scales[11], 150.0);
    assert_eq!(e.scales[0], 1.0);
    let names: Vec<&str> = e.lay_names[0].iter().map(|s| s.as_str()).collect();
    assert_eq!(&names[..9], &["通り芯", "既設", "鉄骨01", "鉄骨02", "", "", "建具", "寸法", "文字"]);
    // 通り芯 = 线色 1・一点鎖 1（线种 5），(F) 不换
    assert_eq!((e.lay_color[0][0], e.lay_style[0][0]), (1, 5));
    assert_eq!((e.lay_color[0][15], e.lay_style[0][15]), (0, 0));
    // 文字种 [3] 3×3 间隔 0.5 线色 2
    assert_eq!(e.moji[2], (3.0, 3.0, 0.5, 2));
    // 线色 8 = 红
    assert_eq!(e.screen[7], Some((0x0000ff, 1)));
}

#[test]
fn new_drawing_uses_env() {
    let e = Env::company();
    let mut ed = Editor::blank();
    e.apply_new(&mut ed.doc);
    ed.lay_pens = Some(e.layer_pens());
    ed.set_write(10, 7);
    assert_eq!(ed.scale(), 100.0);
    // (7)寸法 = 线色 8・线种 9
    assert_eq!((ed.pen.color, ed.pen.style), (8, 9));
    ed.set_write(10, 0);
    assert_eq!((ed.pen.color, ed.pen.style), (1, 5));
    ed.line(p(0.0, 0.0), p(10.0, 0.0));
    let d = jww::read_bytes(&jwwsave::write_bytes(&ed.doc)).unwrap();
    assert_eq!(d.glayers[10].name, "S=1/100");
    assert_eq!(d.glayers[10].lay_name[0], "通り芯");
    assert_eq!(d.glayers[10].scale, 100.0);
    assert_eq!((d.ents[0].a.color, d.ents[0].a.style), (1, 5));
    assert_eq!(d.moji[3].w, 3.0);
}

#[test]
fn keys_and_space() {
    let mut ed = Editor::blank();
    let mut t = Tools::default();
    assert!(t.command(2, &mut ed));
    assert_eq!(t.kind, Kind::Line);
    // 空格 = 水平・垂直 切替
    assert!(!t.hv);
    t.space();
    assert!(t.hv);
    // 做到一半时 Esc 先取消这一步
    assert!(!t.busy(&ed));
    t.click(&mut ed, p(0.0, 0.0), false, None, 0.5);
    assert!(t.busy(&ed));
    t.reset(&mut ed);
    assert!(!t.busy(&ed));
    assert!(t.command(10, &mut ed) && t.cont);
    assert!(!t.command(20, &mut ed)); // 建具平面：还没有
}

#[test]
fn company_clock_menu() {
    let e = jwwview::jwf::Env::company();
    // 左 AM：0 時 = 文字(5)、1 時 = 線(2)、3 時 = 包絡(25)、4 時 = 範囲(33)
    assert_eq!(&e.clock[0][0][..5], &[5, 2, 4, 25, 33]);
    // 右 AM の 0・3・6・9 時は 0（= 標準の読取り）
    assert_eq!([e.clock[1][0][0], e.clock[1][0][3], e.clock[1][0][6], e.clock[1][0][9]], [0, 0, 0, 0]);
    assert_eq!(e.clock[1][1][7], -83);
    assert_eq!(e.clock2[0][0][9], 1);
}

#[test]
fn company_background_is_black() {
    let e = jwwview::jwf::Env::company();
    assert_eq!(e.background, Some(0));
    let mut ed = jwwview::edit::Editor::blank();
    e.apply_new(&mut ed.doc);
    assert_eq!(ed.doc.pens[0].color, 0);
}
