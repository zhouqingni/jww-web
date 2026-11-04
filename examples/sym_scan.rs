//! 線記号変形データの概要：cargo run --example sym_scan -- a.DAT …
fn main() {
    for a in std::env::args().skip(1) {
        let f = jwwview::symbol::read(std::path::Path::new(&a)).unwrap();
        let line = f.syms.iter().filter(|s| s.needs_line).count();
        let empty = f.syms.iter().filter(|s| s.items.is_empty()).count();
        println!("{a}: 「{}」 {} 個（指示線あり {line}、描くもの無し {empty}）", f.name, f.syms.len());
        for s in f.syms.iter().take(4) {
            println!("   [{}] {} 部材 {}", s.count, s.name, s.items.len());
        }
    }
}
