//! 図形（.jws）：Jw_cad 自带的部品（冷蔵庫-60 = Ver.3.51，EV-9人 = Ver.6）

use jwwview::cad::*;
use jwwview::edit::{key_points, Editor};
use jwwview::geom::BBox;
use jwwview::jww::{self, P};
use jwwview::jwwsave;

fn fig(name: &str) -> jww::Jws {
    jww::read_jws(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)).unwrap()
}

fn bbox(ed: &Editor) -> BBox {
    let mut bb = BBox::EMPTY;
    for e in &ed.doc.ents {
        for q in key_points(e) {
            bb.add(q);
        }
    }
    bb
}

#[test]
fn read_jws_files() {
    let f = fig("fridge60.jws");
    assert_eq!(f.version, 351);
    assert_eq!(f.ents.len(), 6);
    assert_eq!(f.scales[0], 100.0);
    let e = fig("ev9.jws");
    assert_eq!(e.version, 600);
    assert_eq!(e.ents.len(), 69);
}

#[test]
fn place_keeps_real_size() {
    let f = fig("fridge60.jws");
    // 1/50 的图纸：600 实寸 = 12mm
    let mut ed = Editor::blank();
    ed.doc.glayers[0].scale = 50.0;
    ed.set_write(0, 4);
    let (n, skip) = ed.place_figure(&f, p(100.0, 100.0), 1.0, 0.0);
    assert_eq!((n, skip), (6, 0));
    let bb = bbox(&ed);
    assert!(((bb.x1 - bb.x0) - 12.0).abs() < 1e-6 && ((bb.y1 - bb.y0) - 12.0).abs() < 1e-6);
    // 基准点在左边中间
    assert!((bb.x0 - 100.0).abs() < 1e-6 && (bb.y0 - 94.0).abs() < 1e-6);
    assert!(ed.doc.ents.iter().all(|e| e.a.layer == 4 && e.a.glayer == 0));
    // 转 90°、倍率 2
    let mut ed = Editor::blank();
    ed.place_figure(&f, P::default(), 2.0, 90.0);
    let bb = bbox(&ed);
    assert!(((bb.x1 - bb.x0) - 12.0).abs() < 1e-6 && ((bb.y1 - bb.y0) - 12.0).abs() < 1e-6);
    assert!((bb.y0 - 0.0).abs() < 1e-6 && (bb.x1 - 6.0).abs() < 1e-6);
    // 存了还能读
    let d = jww::read_bytes(&jwwsave::write_bytes(&ed.doc)).unwrap();
    assert_eq!(d.ents.len(), 6);
}
