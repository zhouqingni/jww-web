//! .jww（或 .dxf）→ PDF。用法：jww2pdf [--color] [--paper A3] <输入.jww|.dxf> <输出.pdf>
use jwwview::{draw::{bboxes, Walker}, font::Fonts, jww, pdf};
use std::path::Path;

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let mut color = false;
    let mut page: Option<(f64, f64)> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--color" => {
                color = true;
                args.remove(i);
            }
            "--paper" if i + 1 < args.len() => {
                let name = args.remove(i + 1).to_uppercase();
                args.remove(i);
                match pdf::PAGES.iter().find(|p| p.0 == name) {
                    Some(p) => page = Some((p.1, p.2)),
                    None => {
                        eprintln!("不认识的纸: {name}（A4〜A0）");
                        std::process::exit(2);
                    }
                }
            }
            _ => i += 1,
        }
    }
    if args.len() != 2 {
        eprintln!("用法: jww2pdf [--color] [--paper A3] <输入.jww> <输出.pdf>");
        std::process::exit(2);
    }
    let is_dxf = args[0].to_ascii_lowercase().ends_with(".dxf");
    let read = if is_dxf {
        let mut d = jwwview::edit::Editor::blank().doc;
        std::fs::read(&args[0]).map_err(|e| e.to_string()).and_then(|b| jwwview::dxf::import(&b, &mut d)).map(|r| {
            println!("{}", r.message());
            d
        })
    } else {
        jww::read(Path::new(&args[0])).map_err(|e| e.to_string())
    };
    let doc = match read {
        Ok(d) => d,
        Err(e) => {
            eprintln!("读不了: {e}");
            std::process::exit(1);
        }
    };
    let fonts = Fonts::system();
    let w = Walker::new(&doc, &fonts);
    // 指定了纸就用文件里存的打印位置；没指定就自动（旧的打印位置会框到整张图）
    let (page, center, scale) = match page {
        Some(pg) => (pg, pdf::file_center(&doc, pg), pdf::file_scale(&doc)),
        None => {
            let (boxes, _) = bboxes(&w);
            let (i, c, s) = pdf::initial_frame(&doc, &boxes);
            ((pdf::PAGES[i].1, pdf::PAGES[i].2), c, s)
        }
    };
    let opt = pdf::PdfOptions { color, visible: pdf::file_visibility(&doc), page, center, scale };
    let bytes = pdf::render(&w, &opt);
    std::fs::write(&args[1], &bytes).expect("写不了 PDF");
    println!("{} → {}（{} KB）", args[0], args[1], bytes.len() / 1024);
}
