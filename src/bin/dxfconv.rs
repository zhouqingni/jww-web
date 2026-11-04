//! .dxf ⇔ .jww の変換。用法：dxfconv <入力.dxf> <出力.jww> ／ dxfconv <入力.jww> <出力.dxf>
use jwwview::{dxf, edit::Editor, jww, jwwsave};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("用法: dxfconv <入力.dxf> <出力.jww>  または  dxfconv <入力.jww> <出力.dxf>");
        std::process::exit(2);
    }
    let (src, dst) = (Path::new(&args[0]), Path::new(&args[1]));
    let ext = |p: &Path| p.extension().map(|x| x.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let res = if ext(src) == "dxf" {
        let mut ed = Editor::blank();
        std::fs::read(src).map_err(|e| e.to_string()).and_then(|b| dxf::import(&b, &mut ed.doc)).and_then(|r| {
            println!("{}", r.message());
            std::fs::write(dst, jwwsave::write_bytes(&ed.doc)).map_err(|e| e.to_string())
        })
    } else {
        jww::read(src).map_err(|e| e.to_string()).and_then(|d| std::fs::write(dst, dxf::export(&d)).map_err(|e| e.to_string()))
    };
    match res {
        Ok(()) => println!("{} → {}", src.display(), dst.display()),
        Err(e) => {
            eprintln!("できません: {e}");
            std::process::exit(1);
        }
    }
}
