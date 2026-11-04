//! JWW Viewer：打开 .jww，看图、画图・改图、存回 .jww、输出 PDF
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use eframe::egui;
use egui::{Color32, FontData, FontDefinitions, FontFamily, Sense, Shape};
use jwwview::draw::{bboxes, Walker};
use jwwview::font::Fonts;
use jwwview::geom::BBox;
use jwwview::edit::Editor;
use jwwview::jwf::Env;
use jwwview::jww::{self, Doc, P};
use jwwview::pdf;
use jwwview::screen::{ScreenSink, View};
use jwwview::tools::{self, Btn, Kind, Prev, Tools};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(target_os = "macos")]
mod macos_open;
#[cfg(target_arch = "wasm32")]
use jwwview::platform;

struct Loaded {
    /// None = 新图纸，还没存过
    path: Option<PathBuf>,
    ed: Editor,
    boxes: Vec<BBox>,
    all: BBox,
    visible: Vec<bool>,
}

impl Loaded {
    fn doc(&self) -> &Doc {
        &self.ed.doc
    }
    fn name(&self) -> String {
        match &self.path {
            Some(p) => p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
            None => "無題".into(),
        }
    }
}

struct App {
    fonts: Fonts,
    file: Option<Loaded>,
    view: Option<View>,
    need_fit: bool,
    error: Option<String>,
    page: usize,
    color: bool,
    show_layers: bool,
    message: Option<String>,
    frames: u32,
    /// 打印框中心（图寸）、打印倍率、"点一下放框"模式
    frame_center: P,
    frame_scale: f64,
    placing: bool,
    tools: Tools,
    /// 图形变了，包围盒要重算
    stale: bool,
    /// 上次算包围盒时的 Editor::rev
    last_rev: u64,
    /// 环境设定（.jwf）：默认是公司的设定
    env: Env,
    /// 印刷ダイアログ：开着吗、打印机一览、选中的打印机、份数
    print_open: bool,
    /// 線属性ダイアログ
    line_attr_open: bool,
    printers: Vec<String>,
    printer: Option<String>,
    copies: u32,
    /// 画布上按着的鼠标（Jw_cad 式：点击・时钟菜单・两键缩放）
    ptr: Option<Ptr>,
    /// 两键缩放后另一个键还没松开
    ptr_wait_up: bool,
    /// 等着看是不是双击的第一下
    pending: Option<Pend>,
    /// 前倍率（两键左下拖动回到这里）
    prev_view: Option<View>,
    /// 时钟菜单选的命令，下一帧执行
    queued: Vec<u32>,
    /// 网页版：浏览器里选好的文件（异步放进来）
    #[cfg(target_arch = "wasm32")]
    inbox: platform::Inbox,
}

/// 时钟菜单开始的拖动距离、(2) 的距离（像素）
const CLOCK: f32 = 20.0;
const FAR: f32 = 100.0;

#[derive(Clone, Copy)]
struct Ptr {
    /// 1 = 左，2 = 右，3 = 两键・中键
    btn: u8,
    start: egui::Pos2,
    last: egui::Pos2,
    max: f32,
    /// 拖出了时钟菜单的圈；回到圈里一次 AM/PM 切换
    out: bool,
    pm: bool,
    mods: egui::Modifiers,
}

#[derive(Clone, Copy)]
struct Pend {
    right: bool,
    pos: egui::Pos2,
    q: P,
    shift: bool,
    ctrl: bool,
}

/// 屏幕上的拖动方向 → 时钟的几点（0 = 上，顺时针）
fn clock_of(a: egui::Pos2, b: egui::Pos2) -> usize {
    let ang = (b.x - a.x).atan2(-(b.y - a.y)).to_degrees();
    (((ang + 360.0 + 15.0) % 360.0) / 30.0) as usize % 12
}

/// 点一下：右键时先找读取点
fn fire(tools: &mut Tools, ed: &mut Editor, q: P, b: Btn, tol: f64) {
    let read = if b.right { ed.read_point(q, tol) } else { None };
    tools.click_btn(ed, q, b, read, tol);
}

impl App {
    fn new(cc: &eframe::CreationContext, path: Option<PathBuf>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        let fonts = Fonts::system();
        // 网页版用不了电脑里的字体，带一套 IPA ゴシック（JIS 第1・第2水準）
        #[cfg(target_arch = "wasm32")]
        let fonts = Fonts::embedded(include_bytes!("../web/ipag-jis.ttf").to_vec());
        // 界面上的日文（图层名等）用同一个系统字体
        if let Some((data, index)) = &fonts.ui_font {
            let mut defs = FontDefinitions::default();
            let mut fd = FontData::from_owned(data.clone());
            fd.index = *index;
            defs.font_data.insert("jp".into(), Arc::new(fd));
            for fam in [FontFamily::Proportional, FontFamily::Monospace] {
                defs.families.entry(fam).or_default().push("jp".into());
            }
            cc.egui_ctx.set_fonts(defs);
        }
        #[cfg(target_os = "macos")]
        macos_open::set_ctx(&cc.egui_ctx);
        let mut app = App {
            fonts,
            file: None,
            view: None,
            need_fit: false,
            error: None,
            page: 1,
            color: false,
            show_layers: true,
            message: None,
            frames: 0,
            frame_center: P::default(),
            frame_scale: 1.0,
            placing: false,
            tools: Tools::default(),
            stale: false,
            last_rev: 0,
            env: load_env(),
            print_open: false,
            line_attr_open: false,
            printers: Vec::new(),
            printer: None,
            copies: 1,
            ptr: None,
            ptr_wait_up: false,
            pending: None,
            prev_view: None,
            queued: Vec::new(),
            #[cfg(target_arch = "wasm32")]
            inbox: Default::default(),
        };
        if let Some(p) = path {
            app.open(&p);
        }
        app
    }

    /// 有没存的修改时问一下。true = 可以继续
    fn confirm_discard(&self) -> bool {
        match &self.file {
            #[cfg(target_arch = "wasm32")]
            Some(f) if f.ed.dirty => platform::confirm(&format!("{} は変更されています。保存せずに続けますか？", f.name())),
            #[cfg(not(target_arch = "wasm32"))]
            Some(f) if f.ed.dirty => {
                rfd::MessageDialog::new()
                    .set_title("JWW Viewer")
                    .set_description(format!("{} は変更されています。保存せずに続けますか？", f.name()))
                    .set_buttons(rfd::MessageButtons::YesNo)
                    .show()
                    == rfd::MessageDialogResult::Yes
            }
            _ => true,
        }
    }

    fn open(&mut self, path: &Path) {
        if !self.confirm_discard() {
            return;
        }
        if is_dxf(path) {
            match std::fs::read(path) {
                Ok(b) => self.load_dxf(path.to_path_buf(), &b),
                Err(e) => self.error = Some(format!("{}\n{e}", path.display())),
            }
            return;
        }
        match jww::read(path) {
            Ok(doc) => self.load(Some(path.to_path_buf()), Editor::new(doc)),
            Err(e) => self.error = Some(format!("{}\n{}", path.display(), e)),
        }
    }

    /// DXF を新しい図面に読み込む（保存は .jww。上書保存でも名前を聞く）
    fn load_dxf(&mut self, path: PathBuf, b: &[u8]) {
        let mut ed = Editor::blank();
        self.env.apply_new(&mut ed.doc);
        match jwwview::dxf::import(b, &mut ed.doc) {
            Ok(r) => {
                self.load(Some(path), ed);
                self.message = Some(r.message());
            }
            Err(e) => self.error = Some(format!("{}\n{e}", path.display())),
        }
    }

    fn new_doc(&mut self) {
        if !self.confirm_discard() {
            return;
        }
        let mut ed = Editor::blank();
        self.env.apply_new(&mut ed.doc);
        self.load(None, ed);
    }

    fn load(&mut self, path: Option<PathBuf>, mut ed: Editor) {
        // 换写入图层时线色・线种跟着环境设定换；当前写入层也套一次
        ed.lay_pens = Some(self.env.layer_pens());
        let (g, l) = (ed.glay(), ed.lay());
        ed.set_write(g, l);
        let doc = &ed.doc;
        let w = Walker::new(doc, &self.fonts);
        let (boxes, _) = bboxes(&w);
        let all = jwwview::draw::main_box(&boxes);
        let visible = pdf::file_visibility(doc);
        (self.page, self.frame_center, self.frame_scale) = pdf::initial_frame(doc, &boxes);
        self.placing = false;
        self.color = doc.color_print != 0;
        self.file = Some(Loaded { path, ed, boxes, all, visible });
        self.need_fit = true;
        self.error = None;
        self.message = None;
        let Self { tools, file, .. } = self;
        if let Some(f) = file {
            tools.reset(&mut f.ed);
        }
    }

    /// 网页版：保存 = 下载 .jww（浏览器的「ダウンロード」フォルダに入ります）
    #[cfg(target_arch = "wasm32")]
    fn save(&mut self, _as_new: bool) {
        let Some(f) = &mut self.file else { return };
        let stem = f.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
        let name = format!("{}.jww", stem.unwrap_or_else(|| "無題".into()));
        let bytes = jwwview::jwwsave::write_bytes(&f.ed.doc);
        self.message = Some(match platform::download(&name, &bytes, "application/octet-stream") {
            Ok(n) => {
                f.ed.dirty = false;
                f.path = Some(PathBuf::from(&name));
                format!("ダウンロードしました：{n}")
            }
            Err(e) => format!("保存できません：{e}"),
        });
    }

    /// 开文件对话框
    fn pick_jww(&mut self) {
        #[cfg(target_arch = "wasm32")]
        platform::pick(&self.inbox, "図面", &["jww", "jw$", "dxf", "DXF"]);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = rfd::FileDialog::new().add_filter("図面（Jw_cad・DXF）", &["jww", "jw$", "dxf", "DXF"]).pick_file() {
            self.open(&p);
        }
    }

    /// 网页版：浏览器选好・拖进来的文件（.jww 打开，.jwf 读成环境设定）
    #[cfg(target_arch = "wasm32")]
    fn take_inbox(&mut self) {
        let Some((p, b)) = self.inbox.borrow_mut().pop() else { return };
        self.inbox.borrow_mut().clear();
        let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        if name.to_ascii_lowercase().ends_with(".jwf") {
            self.env = Env::parse(&b, &name);
            if let Some(f) = &mut self.file {
                f.ed.lay_pens = Some(self.env.layer_pens());
            }
            self.message = Some(format!("環境設定ファイルを読み込みました：{name}"));
            return;
        }
        if !self.confirm_discard() {
            return;
        }
        if is_dxf(&p) {
            self.load_dxf(p, &b);
            return;
        }
        match jww::read_bytes(&b) {
            Ok(doc) => self.load(Some(p), Editor::new(doc)),
            Err(e) => self.error = Some(format!("{name}\n{e}")),
        }
    }

    /// 保存。as_new = 名前を付けて保存
    #[cfg(not(target_arch = "wasm32"))]
    fn save(&mut self, as_new: bool) {
        let Some(f) = &mut self.file else { return };
        let target = match (&f.path, as_new) {
            // DXF から読んだ図は .jww として名前を付けて保存
            (Some(p), false) if !is_dxf(p) => Some(p.clone()),
            _ => {
                let stem = f.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
                rfd::FileDialog::new()
                    .add_filter("Jw_cad", &["jww"])
                    .set_file_name(format!("{}.jww", stem.unwrap_or_else(|| "無題".into())))
                    .save_file()
            }
        };
        let Some(mut out) = target else { return };
        if out.extension().is_none() {
            out.set_extension("jww");
        }
        self.message = Some(match jwwview::jwwsave::save(&f.ed.doc, &out) {
            Ok(_) => {
                f.ed.dirty = false;
                f.path = Some(out.clone());
                format!("保存しました：{}（元のファイルは .bak に残しています）", out.display())
            }
            Err(e) => format!("保存できません：{e}"),
        });
    }

    /// 印刷枠の中身を PDF に（PDF 出力と直接印刷で共通）
    fn pdf_bytes(&self) -> Option<Vec<u8>> {
        let f = self.file.as_ref()?;
        let w = Walker::new(&f.ed.doc, &self.fonts);
        let opt = pdf::PdfOptions {
            color: self.color,
            visible: f.visible.clone(),
            page: page_mm(self.page),
            center: self.frame_center,
            scale: self.frame_scale,
        };
        Some(pdf::render(&w, &opt))
    }

    fn open_print(&mut self) {
        let (list, def) = jwwview::print::printers();
        if self.printer.as_ref().map_or(true, |p| !list.contains(p)) {
            self.printer = def.or_else(|| list.first().cloned());
        }
        self.printers = list;
        self.print_open = true;
    }

    /// 网页版：调不了打印机，下载 PDF 用浏览器打印
    #[cfg(target_arch = "wasm32")]
    fn do_print(&mut self) {
        self.print_open = false;
        self.export_pdf();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn do_print(&mut self) {
        let Some(bytes) = self.pdf_bytes() else { return };
        let stem = self.file.as_ref().and_then(|f| f.path.as_ref()).and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or("無題".into());
        let tmp = std::env::temp_dir().join(format!("jwwviewer-{stem}.pdf"));
        if let Err(e) = std::fs::write(&tmp, bytes) {
            self.message = Some(format!("印刷用 PDF を作れません：{e}"));
            return;
        }
        let media = pdf::PAGES[self.page].0;
        self.message = Some(match jwwview::print::print_pdf(&tmp, self.printer.as_deref(), self.copies, media) {
            Ok(_) => format!("{} に {} 部送りました（{media}）", self.printer.as_deref().unwrap_or("既定のプリンタ"), self.copies),
            Err(e) => format!("印刷できません：{e}"),
        });
    }

    /// 网页版：PDF 下载
    #[cfg(target_arch = "wasm32")]
    fn export_pdf(&mut self) {
        let Some(f) = &self.file else { return };
        let stem = f.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or("無題".into());
        let Some(bytes) = self.pdf_bytes() else { return };
        let name = format!("{stem}.pdf");
        self.message = Some(match platform::download(&name, &bytes, "application/pdf") {
            Ok(n) => format!("PDF をダウンロードしました：{n}（印刷はブラウザで PDF を開いて）"),
            Err(e) => format!("PDF を保存できません：{e}"),
        });
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_pdf(&mut self) {
        let Some(f) = &self.file else { return };
        let stem = f.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or("無題".into());
        let Some(out) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).set_file_name(format!("{stem}.pdf")).save_file()
        else {
            return;
        };
        let Some(bytes) = self.pdf_bytes() else { return };
        self.message = Some(match std::fs::write(&out, bytes) {
            Ok(_) => format!("PDF を保存しました：{}", out.display()),
            Err(e) => format!("PDF を保存できません：{e}"),
        });
    }

    /// DXF 出力（R12・実寸）。図面のファイル名や「変更あり」はそのまま
    #[cfg(not(target_arch = "wasm32"))]
    fn export_dxf(&mut self) {
        let Some(f) = &self.file else { return };
        let stem = f.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or("無題".into());
        let Some(mut out) = rfd::FileDialog::new().add_filter("DXF", &["dxf"]).set_file_name(format!("{stem}.dxf")).save_file() else {
            return;
        };
        if out.extension().is_none() {
            out.set_extension("dxf");
        }
        let bytes = jwwview::dxf::export(&f.ed.doc);
        self.message = Some(match std::fs::write(&out, bytes) {
            Ok(_) => format!("DXF を保存しました：{}", out.display()),
            Err(e) => format!("DXF を保存できません：{e}"),
        });
    }

    /// 網頁版：DXF をダウンロード
    #[cfg(target_arch = "wasm32")]
    fn export_dxf(&mut self) {
        let Some(f) = &self.file else { return };
        let stem = f.path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or("無題".into());
        let bytes = jwwview::dxf::export(&f.ed.doc);
        self.message = Some(match platform::download(&format!("{stem}.dxf"), &bytes, "application/dxf") {
            Ok(n) => format!("DXF をダウンロードしました：{n}"),
            Err(e) => format!("DXF を保存できません：{e}"),
        });
    }
}

fn is_dxf(p: &Path) -> bool {
    p.extension().map_or(false, |x| x.eq_ignore_ascii_case("dxf"))
}

fn page_mm(i: usize) -> (f64, f64) {
    let p = pdf::PAGES[i];
    (p.1, p.2)
}

fn fit_box(f: &Loaded) -> BBox {
    if !f.all.is_empty() {
        return f.all;
    }
    let (w, h) = f.doc().paper_size();
    BBox { x0: -w / 2.0, y0: -h / 2.0, x1: w / 2.0, y1: h / 2.0 }
}

/// 测试用：设了 JWW_SNAP=<png 路径> 就在画好之后存一张截图并退出
fn snapshot(ctx: &egui::Context, frames: &mut u32) {
    let Some(path) = std::env::var_os("JWW_SNAP") else { return };
    *frames += 1;
    if *frames == 5 {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
    }
    let shot = ctx.input(|i| {
        i.raw.events.iter().find_map(|e| match e {
            egui::Event::Screenshot { image, .. } => Some(image.clone()),
            _ => None,
        })
    });
    if let Some(img) = shot {
        let [w, h] = img.size;
        let rgba: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
        let png = encode_png(w as u32, h as u32, &rgba);
        let _ = std::fs::write(path, png);
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    ctx.request_repaint();
}

fn encode_png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, ty: &[u8], data: &[u8]) {
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut v = ty.to_vec();
        v.extend_from_slice(data);
        out.extend_from_slice(&v);
        out.extend_from_slice(&crc(&v).to_be_bytes());
    }
    let mut raw = Vec::with_capacity((w * 4 + 1) as usize * h as usize);
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
    chunk(&mut out, b"IEND", &[]);
    out
}

const LINE_COLORS: [&str; 9] = ["線色1", "線色2", "線色3", "線色4", "線色5", "線色6", "線色7", "線色8", "補助線色"];
const LINE_STYLES: [&str; 9] = ["実線", "点線1", "点線2", "点線3", "一点鎖1", "一点鎖2", "二点鎖1", "二点鎖2", "補助線種"];
const PAPERS: [(u32, &str); 12] = [
    (0, "A-0"), (1, "A-1"), (2, "A-2"), (3, "A-3"), (4, "A-4"), (8, "2A"), (9, "3A"), (10, "4A"), (11, "5A"),
    (12, "10m"), (13, "50m"), (14, "100m"),
];

impl App {
    /// 右下の書込レイヤ・縮尺・用紙。右から左に並べるので、置く順は逆
    fn write_bar(&mut self, ui: &mut egui::Ui) {
        let Some(f) = &mut self.file else { return };
        let ed = &mut f.ed;
        let mut paper = ed.doc.paper;
        egui::ComboBox::from_id_salt("paper").width(56.0).selected_text(ed.doc.paper_name()).show_ui(ui, |ui| {
            for (k, n) in PAPERS {
                ui.selectable_value(&mut paper, k, n);
            }
        });
        if paper != ed.doc.paper {
            ed.begin();
            ed.doc.paper = paper;
        }
        let mut sc = ed.doc.glayers[ed.glay()].scale;
        if ui.add(egui::DragValue::new(&mut sc).range(0.01..=100000.0).speed(1.0)).on_hover_text("書込グループの縮尺").changed() {
            ed.begin();
            let gi = ed.glay();
            ed.doc.glayers[gi].scale = sc;
        }
        ui.label("縮尺 1/");
        let (mut g, mut l) = (ed.glay(), ed.lay());
        let gname = |g: usize| {
            let n = &ed.doc.glayers[g].name;
            if n.is_empty() { format!("[{g:X}]") } else { format!("[{g:X}] {n}") }
        };
        let lname = |l: usize| {
            let n = &ed.doc.glayers[g].lay_name[l];
            if n.is_empty() { format!("({l:X})") } else { format!("({l:X}) {n}") }
        };
        let ltext = lname(l);
        egui::ComboBox::from_id_salt("wl").width(90.0).selected_text(ltext).show_ui(ui, |ui| {
            for k in 0..16 {
                ui.selectable_value(&mut l, k, lname(k));
            }
        });
        let gtext = format!("[{g:X}] S=1/{}", ed.doc.glayers[g].scale);
        egui::ComboBox::from_id_salt("wg").width(90.0).selected_text(gtext).show_ui(ui, |ui| {
            for k in 0..16 {
                ui.selectable_value(&mut g, k, gname(k));
            }
        });
        ui.label("書込");
        if (g, l) != (ed.glay(), ed.lay()) {
            ed.set_write(g, l);
            f.visible[g * 16 + l] = true;
        }
    }

}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        snapshot(ctx, &mut self.frames);
        // 拖进窗口的文件
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        #[cfg(target_os = "macos")]
        let dropped = [dropped, macos_open::take()].concat();
        #[cfg(target_arch = "wasm32")]
        {
            let web_drop: Vec<(PathBuf, Vec<u8>)> = ctx.input(|i| {
                i.raw.dropped_files.iter().filter_map(|f| f.bytes.as_ref().map(|b| (PathBuf::from(&f.name), b.to_vec()))).collect()
            });
            self.inbox.borrow_mut().extend(web_drop);
            self.take_inbox();
            platform::set_dirty(self.file.as_ref().map_or(false, |f| f.ed.dirty));
        }
        if let Some(p) = dropped.last() {
            self.open(p);
        }
        // 键盘：Esc 取消、⌘Z 戻る、⇧⌘Z 進む、⌘S 保存
        let typing = ctx.wants_keyboard_input();
        let (esc, undo, redo, save) = ctx.input(|i| {
            let cmd = i.modifiers.command;
            (
                i.key_pressed(egui::Key::Escape),
                !typing && cmd && !i.modifiers.shift && i.key_pressed(egui::Key::Z),
                !typing && cmd && ((i.modifiers.shift && i.key_pressed(egui::Key::Z)) || i.key_pressed(egui::Key::Y)),
                cmd && i.key_pressed(egui::Key::S),
            )
        });
        let (copy_key, paste_key) = ctx.input(|i| {
            let cmd = i.modifiers.command && !typing;
            (
                cmd && i.events.iter().any(|e| matches!(e, egui::Event::Copy)) || cmd && i.key_pressed(egui::Key::C),
                cmd && i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))) || cmd && i.key_pressed(egui::Key::V),
            )
        });
        if let Some(f) = &mut self.file {
            if copy_key && !f.ed.sel.is_empty() {
                self.tools.copy_selection(&mut f.ed);
            }
            if paste_key {
                self.tools.set(Kind::Paste, &mut f.ed);
            }
        }
        // Jw_cad 的键：A〜Z（Shift 另一组）、F2〜F9、空格 = 环境设定的命令号；
        // Esc = 做到一半就取消这一步，否则「戻る」；Shift+Esc =「進む」；Tab = 属性取得
        let keys: Vec<(egui::Key, bool)> = if typing {
            vec![]
        } else {
            ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. }
                            if !modifiers.command && !modifiers.alt && !modifiers.ctrl =>
                        {
                            Some((*key, modifiers.shift))
                        }
                        _ => None,
                    })
                    .collect()
            })
        };
        let mut esc = esc && typing;
        let (mut undo, mut redo, mut save) = (undo, redo, save);
        let queued: Vec<(egui::Key, bool, u32)> = std::mem::take(&mut self.queued).into_iter().map(|n| (egui::Key::Escape, false, n)).collect();
        let keys: Vec<(egui::Key, bool, u32)> = keys.into_iter().map(|(k, s)| (k, s, 0)).chain(queued).collect();
        for (key, shift, clock_cmd) in keys {
            let Some(f) = &mut self.file else { break };
            // キーを押したら、ダブルクリック待ちは取り消し
            self.pending = None;
            let cmd = match key {
                _ if clock_cmd != 0 => clock_cmd,
                egui::Key::Escape if shift => {
                    redo = true;
                    continue;
                }
                egui::Key::Escape => {
                    if self.tools.busy(&f.ed) {
                        esc = true;
                    } else {
                        undo = true;
                    }
                    continue;
                }
                egui::Key::Space => {
                    self.tools.space();
                    continue;
                }
                // ペン：Enter = 確定、BackSpace = 一つ戻す
                egui::Key::Enter if self.tools.kind == Kind::Pen => {
                    self.tools.pen_finish(&mut f.ed, false);
                    self.stale = true;
                    continue;
                }
                egui::Key::Backspace if self.tools.kind == Kind::Pen => {
                    self.tools.pen_back();
                    continue;
                }
                egui::Key::Tab => 78,
                k => match key_index(k) {
                    Some(KeyIdx::Letter(i)) => {
                        let (a, b) = self.env.keys[i];
                        if shift { b } else { a }
                    }
                    Some(KeyIdx::F(i)) => self.env.fkeys[i],
                    None => continue,
                },
            };
            match cmd {
                0 => {}
                98 => {
                    self.pick_jww();
                }
                100 => save = true,
                99 => self.open_print(),
                150 => self.need_fit = true,
                n => {
                    if !self.tools.command(n, &mut f.ed) {
                        self.tools.note = Some(format!("コマンド {n} はまだありません"));
                    }
                    self.stale = true;
                }
            }
        }
        if let Some(f) = &mut self.file {
            if esc {
                self.tools.reset(&mut f.ed);
            }
            if undo {
                self.tools.reset(&mut f.ed);
                f.ed.undo();
                self.stale = true;
            }
            if redo {
                self.tools.reset(&mut f.ed);
                f.ed.redo();
                self.stale = true;
            }
        }
        if save {
            self.save(false);
        }
        let title = match &self.file {
            Some(f) => format!("{}{} - JWW Viewer", f.name(), if f.ed.dirty { " *" } else { "" }),
            None => "JWW Viewer".into(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("新規").clicked() {
                    self.new_doc();
                }
                if ui.button("開く…").clicked() {
                    self.pick_jww();
                }
                let has = self.file.is_some();
                if ui.add_enabled(has, egui::Button::new("上書保存")).on_hover_text("元のファイルは .bak に残します").clicked() {
                    self.save(false);
                }
                if ui.add_enabled(has, egui::Button::new("名前を付けて保存…")).clicked() {
                    self.save(true);
                }
                if ui.add_enabled(has, egui::Button::new("DXF 出力…")).on_hover_text("実寸の DXF（R12）。ブロック・寸法は分解して書きます").clicked() {
                    self.export_dxf();
                }
                ui.separator();
                let (cu, cr) = self.file.as_ref().map(|f| (f.ed.can_undo(), f.ed.can_redo())).unwrap_or((false, false));
                if ui.add_enabled(cu, egui::Button::new("戻る")).on_hover_text("⌘Z").clicked() {
                    if let Some(f) = &mut self.file {
                        self.tools.reset(&mut f.ed);
                        f.ed.undo();
                        self.stale = true;
                    }
                }
                if ui.add_enabled(cr, egui::Button::new("進む")).on_hover_text("⇧⌘Z").clicked() {
                    if let Some(f) = &mut self.file {
                        self.tools.reset(&mut f.ed);
                        f.ed.redo();
                        self.stale = true;
                    }
                }
                ui.separator();
                if ui.button("全体表示").clicked() {
                    self.need_fit = true;
                }
                ui.toggle_value(&mut self.show_layers, "レイヤ");
                ui.separator();
                egui::ComboBox::from_id_salt("page")
                    .selected_text(pdf::PAGES[self.page].0)
                    .show_ui(ui, |ui| {
                        for (i, p) in pdf::PAGES.iter().enumerate() {
                            ui.selectable_value(&mut self.page, i, p.0);
                        }
                    });
                ui.add(
                    egui::DragValue::new(&mut self.frame_scale)
                        .range(0.01..=10.0)
                        .speed(0.01)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                        .custom_parser(|s| s.trim_end_matches('%').trim().parse::<f64>().ok().map(|v| v / 100.0)),
                )
                .on_hover_text("印刷倍率");
                ui.toggle_value(&mut self.placing, "印刷枠")
                    .on_hover_text("押してから図面をクリックすると、そこが印刷枠の中心になります");
                ui.checkbox(&mut self.color, "カラー");
                if ui.add_enabled(self.file.is_some(), egui::Button::new("PDF 出力…")).clicked() {
                    self.export_pdf();
                }
                if ui.add_enabled(self.file.is_some(), egui::Button::new("印刷…")).on_hover_text("印刷枠の中をプリンタに送ります（F8）").clicked() {
                    self.open_print();
                }
                ui.separator();
                if ui
                    .button("環境設定…")
                    .on_hover_text(format!("いまの環境設定ファイル：{}
新規図面のレイヤ名・縮尺・文字種・色、レイヤ別の線色・線種、キー割当に使います", self.env.name))
                    .clicked()
                {
                    #[cfg(target_arch = "wasm32")]
                    platform::pick(&self.inbox, "Jw_cad 環境設定", &["jwf", "JWF"]);
                    #[cfg(not(target_arch = "wasm32"))]
                    if let Some(p) = rfd::FileDialog::new().add_filter("Jw_cad 環境設定", &["jwf", "JWF"]).pick_file() {
                        match std::fs::read(&p) {
                            Ok(b) => {
                                let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                                self.env = Env::parse(&b, &name);
                                if let Some(m) = env_memo() {
                                    let _ = std::fs::write(m, p.to_string_lossy().as_bytes());
                                }
                                if let Some(f) = &mut self.file {
                                    f.ed.lay_pens = Some(self.env.layer_pens());
                                }
                                self.message = Some(format!("環境設定ファイルを読み込みました：{name}"));
                            }
                            Err(e) => self.error = Some(format!("{}
{e}", p.display())),
                        }
                    }
                }
            });
            if self.file.is_some() {
                ui.horizontal(|ui| {
                    if let Some(f) = &mut self.file {
                        self.tools.bar(ui, &mut f.ed);
                    }
                });
            }
        });

        let mut cursor: Option<P> = None;
        if let Some(view) = self.view {
            if let Some(pos) = ctx.pointer_hover_pos() {
                if view.rect.contains(pos) {
                    cursor = Some(view.to_paper(pos));
                }
            }
        }
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.file.is_some() {
                    ui.label(self.tools.prompt());
                }
            });
            ui.horizontal(|ui| {
                if let Some(f) = &self.file {
                    let sc = f.ed.scale();
                    ui.label(f.name());
                    if let Some(c) = cursor {
                        ui.label(format!("X {:.1}  Y {:.1}", c.x * sc, c.y * sc));
                    }
                }
                if let Some(m) = &self.message {
                    ui.separator();
                    ui.label(m);
                }
                // 右下：書込レイヤ・縮尺・用紙
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    self.write_bar(ui);
                });
            });
        });

        // 工具面板（左）：指定の二列
        if self.file.is_some() {
            let mut print = false;
            egui::SidePanel::left("tools").resizable(false).exact_width(104.0).show(ctx, |ui| {
                ui.add_space(4.0);
                egui::Grid::new("palette").num_columns(2).min_col_width(46.0).spacing([4.0, 3.0]).show(ui, |ui| {
                    for (a, b) in tools::SIDE {
                        for (kind, label) in [a, b] {
                            let on = kind.map_or(false, |k| self.tools.kind == k);
                            let btn = egui::Button::new(egui::RichText::new(*label).size(13.0)).selected(on);
                            if ui.add_sized([46.0, 24.0], btn).clicked() {
                                match kind {
                                    None => print = true,
                                    Some(k) => {
                                        if let Some(f) = &mut self.file {
                                            let k2 = if on { Kind::None } else { *k };
                                            self.tools.set(k2, &mut f.ed);
                                            self.pending = None;
                                            self.stale = true;
                                        }
                                    }
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
                // 取値：線角・2点角・線長・2点長・間隔
                ui.add_space(6.0);
                egui::Grid::new("picks").num_columns(2).min_col_width(46.0).spacing([4.0, 3.0]).show(ui, |ui| {
                    for (k, (m, label)) in tools::PICKS.iter().enumerate() {
                        let on = self.tools.pick == Some(*m);
                        // 測る系は底の色を変えて上の工具と見分ける
                        let fill = if on { ui.visuals().selection.bg_fill } else { Color32::from_rgb(214, 232, 214) };
                        let b = egui::Button::new(egui::RichText::new(*label).size(13.0).color(Color32::from_gray(20))).fill(fill);
                        if ui.add_sized([46.0, 24.0], b).on_hover_text("取った値を今のコマンドの入力欄に入れます").clicked() {
                            self.tools.start_pick(*m);
                        }
                        if k % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
            });
            if print {
                self.open_print();
            }
        }

        // 右：グループ・レイヤ・線属性
        {
            let Self { file, show_layers, line_attr_open, .. } = self;
            if let Some(f) = file {
                egui::SidePanel::right("layers")
                    .resizable(false)
                    .exact_width(LB * 2.0 + 2.0 + 10.0)
                    .frame(egui::Frame::side_top_panel(&ctx.style()).inner_margin(egui::Margin::symmetric(5, 0)))
                    .show(ctx, |ui| {
                    ui.add_space(10.0);
                    if *show_layers {
                        layer_bars(ui, &mut f.ed, line_attr_open);
                    } else if line_attr_button(ui, &f.ed) {
                        *line_attr_open = !*line_attr_open;
                    }
                    ui.add_space(10.0);
                });
                if *line_attr_open {
                    line_attr_window(ctx, &mut f.ed, line_attr_open);
                }
            }
        }
        if let Some(f) = &mut self.file {
            // 属性取得のレイヤ反転表示：見えないレイヤを表示、見えるレイヤを隠す
            f.visible = pdf::file_visibility(&f.ed.doc);
            if f.ed.invert {
                for (g, gl) in f.ed.doc.glayers.iter().enumerate() {
                    for l in 0..16 {
                        f.visible[g * 16 + l] = gl.state < 2 || gl.lay_state[l] < 2;
                    }
                }
            }
            if let Some(x) = self.tools.offset_dialog(ctx, &f.ed) {
                let tol = 8.0 / self.view.map(|v| v.zoom).unwrap_or(1.0);
                self.tools.click_btn(&mut f.ed, x, Btn { right: true, ..Default::default() }, Some(x), tol);
            }
        }

        if self.print_open {
            let mut open = true;
            let mut go = false;
            egui::Window::new("印刷").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                ui.label(format!("用紙 {}　印刷倍率 {:.0}%　{}", pdf::PAGES[self.page].0, self.frame_scale * 100.0, if self.color { "カラー" } else { "黒" }));
                ui.label("印刷するのは画面の赤い印刷枠の中です");
                ui.horizontal(|ui| {
                    ui.label("プリンタ");
                    egui::ComboBox::from_id_salt("printer")
                        .width(220.0)
                        .selected_text(self.printer.clone().unwrap_or_else(|| "既定のプリンタ".into()))
                        .show_ui(ui, |ui| {
                            for p in &self.printers {
                                ui.selectable_value(&mut self.printer, Some(p.clone()), p);
                            }
                        });
                });
                if self.printers.is_empty() {
                    ui.colored_label(Color32::from_rgb(200, 60, 0), "プリンタが見つかりません（既定のプリンタに送ります）");
                }
                ui.horizontal(|ui| {
                    ui.label("部数");
                    ui.add(egui::DragValue::new(&mut self.copies).range(1..=99));
                });
                if ui.button("印刷").clicked() {
                    go = true;
                }
            });
            if go {
                self.do_print();
                open = false;
            }
            self.print_open = open;
        }

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
            let rect = resp.rect;
            let Some(f) = &mut self.file else {
                painter.rect_filled(rect, 0.0, Color32::from_gray(40));
                let msg = self.error.clone().unwrap_or_else(|| "「開く」か、.jww・.dxf をここへドラッグしてください。「新規」で新しい図面".into());
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, msg, egui::FontId::proportional(18.0), Color32::LIGHT_GRAY);
                return;
            };
            let mut view = self.view.unwrap_or(View { center: P::default(), zoom: 1.0, rect });
            view.rect = rect;
            if self.need_fit {
                view.fit(&fit_box(f));
                self.need_fit = false;
            }
            // 滚轮・双指捏合 = 以光标为中心缩放
            if let Some(pos) = resp.hover_pos() {
                let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
                let factor = (scroll as f64 / 200.0).exp() * pinch as f64;
                if factor != 1.0 {
                    let before = view.to_paper(pos);
                    view.zoom = (view.zoom * factor).clamp(1e-4, 1e5);
                    let after = view.to_paper(pos);
                    view.center.x += before.x - after.x;
                    view.center.y += before.y - after.y;
                }
            }
            let tol = 8.0 / view.zoom;

            // ---- 鼠标（Jw_cad）：L = 任意点、R = 读取点、不动再点 = 双击、
            // 左右拖动 = 时钟菜单、两键（中键）拖动 = 缩放、Shift+拖动 = 平移 ----
            let events = ui.input(|i| i.events.clone());
            let any_down = ui.input(|i| i.pointer.any_down());
            let mut clicks: Vec<(bool, egui::Pos2, egui::Modifiers)> = Vec::new();
            let mut clocks: Vec<(u8, usize, bool, bool, egui::Pos2)> = Vec::new();
            let mut zoom_end: Option<(egui::Pos2, egui::Pos2)> = None;
            let mut pen_drags: Vec<(bool, egui::Pos2, egui::Pos2)> = Vec::new();
            for ev in &events {
                match ev {
                    egui::Event::PointerButton { pos, button, pressed: true, modifiers } => {
                        // Mac：Control+クリック = 右クリック
                        let mut modifiers = *modifiers;
                        let mut button = *button;
                        if cfg!(target_os = "macos") && button == egui::PointerButton::Primary && modifiers.ctrl && !modifiers.mac_cmd {
                            button = egui::PointerButton::Secondary;
                            modifiers.ctrl = false;
                        }
                        let (modifiers, button) = (&modifiers, &button);
                        let b = match button {
                            egui::PointerButton::Primary => 1,
                            egui::PointerButton::Secondary => 2,
                            egui::PointerButton::Middle => 3,
                            _ => continue,
                        };
                        match &mut self.ptr {
                            None if resp.hovered() && rect.contains(*pos) => {
                                self.ptr = Some(Ptr { btn: b, start: *pos, last: *pos, max: 0.0, out: false, pm: false, mods: *modifiers })
                            }
                            Some(p) if p.btn != b => p.btn = 3,
                            _ => {}
                        }
                    }
                    egui::Event::PointerMoved(pos) => {
                        if let Some(p) = &mut self.ptr {
                            let d = (*pos - p.start).length();
                            p.max = p.max.max(d);
                            if p.mods.shift && p.btn != 2 {
                                let dd = *pos - p.last;
                                view.center.x -= dd.x as f64 / view.zoom;
                                view.center.y += dd.y as f64 / view.zoom;
                            } else if p.btn != 3 {
                                if d >= CLOCK {
                                    p.out = true;
                                } else if p.out && d < CLOCK * 0.5 {
                                    p.out = false;
                                    p.pm = !p.pm;
                                }
                            }
                            p.last = *pos;
                        }
                    }
                    egui::Event::PointerButton { pos, pressed: false, .. } => {
                        if self.ptr_wait_up {
                            self.ptr_wait_up = any_down;
                            continue;
                        }
                        let Some(p) = self.ptr.take() else { continue };
                        let d = (*pos - p.start).length();
                        if p.btn == 3 {
                            self.ptr_wait_up = any_down;
                            if !p.mods.shift {
                                zoom_end = Some((p.start, *pos));
                            }
                        } else if p.mods.shift && p.btn == 1 && p.max > 3.0 {
                            // Shift+ドラッグ = 移動（済み）
                        } else if self.tools.kind == Kind::Pen && p.max >= 4.0 {
                            // ペン：ドラッグ = 滑らかな点（時計メニューは出さない）
                            pen_drags.push((p.btn == 2, p.start, *pos));
                        } else if p.max < CLOCK {
                            // 時計メニューまで動いていない = クリック（トラックパッドで少しずれても）
                            clicks.push((p.btn == 2, p.start, p.mods));
                        } else if d >= CLOCK {
                            clocks.push((p.btn, clock_of(p.start, *pos), p.pm, d >= FAR, p.start));
                        }
                    }
                    _ => {}
                }
            }
            // 双击等待中，鼠标动了 = 单击
            if let (Some(pd), Some(hp)) = (self.pending, ctx.pointer_hover_pos()) {
                if (hp - pd.pos).length() > 3.0 && self.ptr.is_none() {
                    self.pending = None;
                    fire(&mut self.tools, &mut f.ed, pd.q, Btn { right: pd.right, dbl: false, shift: pd.shift, ctrl: pd.ctrl }, tol);
                }
            }
            for (right, pos, mods) in clicks {
                if self.placing && !right {
                    self.frame_center = view.to_paper(pos);
                    self.placing = false;
                    continue;
                }
                let q = view.to_paper(pos);
                let (shift, ctrl) = (mods.shift, mods.ctrl || mods.mac_cmd);
                if let Some(pd) = self.pending.take() {
                    if pd.right == right && (pos - pd.pos).length() <= 3.0 {
                        fire(&mut self.tools, &mut f.ed, pd.q, Btn { right, dbl: true, shift: pd.shift, ctrl: pd.ctrl }, tol);
                        continue;
                    }
                    fire(&mut self.tools, &mut f.ed, pd.q, Btn { right: pd.right, dbl: false, shift: pd.shift, ctrl: pd.ctrl }, tol);
                }
                if self.tools.wants_double(&f.ed, q, right, tol) {
                    self.pending = Some(Pend { right, pos, q, shift, ctrl });
                } else {
                    fire(&mut self.tools, &mut f.ed, q, Btn { right, dbl: false, shift, ctrl }, tol);
                }
            }
            for (right, start, end) in pen_drags {
                self.pending = None;
                let (a, b) = (view.to_paper(start), view.to_paper(end));
                self.tools.pen_drag(&mut f.ed, a, b, right, tol);
            }
            // ペンのドラッグ中：ハンドルを見せる
            self.tools.pen_live = match &self.ptr {
                Some(p) if self.tools.kind == Kind::Pen && p.btn != 3 && p.max >= 4.0 && !p.mods.shift => {
                    Some((view.to_paper(p.start), view.to_paper(p.last)))
                }
                _ => None,
            };
            for (btn, h, pm, far, start) in clocks {
                self.pending = None;
                let q0 = view.to_paper(start);
                let lr = (btn == 2) as usize;
                if btn == 1 && !pm && self.tools.left_drag_special(&mut f.ed, q0, h, tol) {
                    continue;
                }
                let n = clock_cmd(&self.env, lr, pm, far, h);
                if btn == 2 && !pm && n == 0 && h % 3 == 0 {
                    if let Some(x) = self.tools.right_drag_read(&mut f.ed, q0, h, tol) {
                        self.tools.click_btn(&mut f.ed, x, Btn { right: true, ..Default::default() }, Some(x), tol);
                    }
                    continue;
                }
                if n > 0 {
                    self.queued.push(n as u32);
                } else if n < 0 {
                    self.tools.note = Some(format!("クロックメニュー {n} はまだありません"));
                }
            }
            if let Some((a, b)) = zoom_end {
                let d = b - a;
                let cur = view;
                if d.length() < 6.0 {
                    // 両ボタンクリック = その位置を画面中央へ
                    view.center = view.to_paper(a);
                    self.prev_view = Some(cur);
                } else if d.x > 0.0 && d.y > 0.0 {
                    // 右下 = 範囲拡大
                    let (pa, pb) = (view.to_paper(a), view.to_paper(b));
                    view.center = P { x: (pa.x + pb.x) / 2.0, y: (pa.y + pb.y) / 2.0 };
                    view.zoom = (view.zoom * (rect.width() / d.x).min(rect.height() / d.y) as f64).clamp(1e-4, 1e5);
                    self.prev_view = Some(cur);
                } else if d.x < 0.0 && d.y < 0.0 {
                    // 左上 = 1/2 縮小
                    view.zoom *= 0.5;
                    self.prev_view = Some(cur);
                } else if d.x > 0.0 {
                    // 右上 = 全体
                    view.fit(&fit_box(f));
                    self.prev_view = Some(cur);
                } else if let Some(pv) = self.prev_view.replace(cur) {
                    // 左下 = 前倍率
                    view.center = pv.center;
                    view.zoom = pv.zoom;
                }
            }
            self.view = Some(view);
            if self.stale || f.ed.rev != self.last_rev {
                self.last_rev = f.ed.rev;
                let w = Walker::new(&f.ed.doc, &self.fonts);
                let (boxes, _) = bboxes(&w);
                f.all = jwwview::draw::main_box(&boxes);
                f.boxes = boxes;
                self.stale = false;
            }

            let doc = &f.ed.doc;
            let bg = jwwview::draw::rgb(doc.pens.first().map(|p| p.color).unwrap_or(0xFFFFFF));
            painter.rect_filled(rect, 0.0, Color32::from_rgb(bg.0, bg.1, bg.2));

            let w = Walker::new(doc, &self.fonts);
            let mut sink = ScreenSink::new(doc, view, &f.visible);
            let vr = view.paper_rect();
            for (e, bb) in doc.ents.iter().zip(&f.boxes) {
                if vr.hits(bb) {
                    w.ent(e, &jwwview::geom::Affine::ID, &e.a, 0, &mut sink);
                }
            }
            let ScreenSink { fills, lines, text, .. } = sink;
            painter.add(Shape::mesh(fills));
            painter.add(Shape::mesh(lines));
            painter.add(Shape::mesh(text));

            // 选中・拾取中的图形（粉色），预览（青色），读取点（小方块）
            if let Some(hp) = resp.hover_pos() {
                let q = view.to_paper(hp);
                let (prev, hl) = self.tools.preview(&f.ed, q);
                let sel_col = Color32::from_rgb(255, 0, 200);
                let mut hl_prev = Vec::new();
                for i in hl {
                    if let Some(e) = f.ed.doc.ents.get(i) {
                        tools_shape(&mut hl_prev, &e.shape);
                    }
                }
                draw_prev(&painter, &view, &hl_prev, egui::Stroke::new(2.0_f32, sel_col));
                draw_prev(&painter, &view, &prev, egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 160, 255)));
                if self.tools.kind != Kind::None {
                    if let Some(r) = f.ed.read_point(q, tol) {
                        let s = view.to_screen(r);
                        painter.rect_stroke(
                            egui::Rect::from_center_size(s, egui::vec2(9.0, 9.0)),
                            0.0,
                            egui::Stroke::new(1.0_f32, Color32::from_rgb(255, 80, 0)),
                            egui::StrokeKind::Middle,
                        );
                    }
                }
            } else {
                let (_, hl) = self.tools.preview(&f.ed, P::default());
                let mut hl_prev = Vec::new();
                for i in hl {
                    if let Some(e) = f.ed.doc.ents.get(i) {
                        tools_shape(&mut hl_prev, &e.shape);
                    }
                }
                draw_prev(&painter, &view, &hl_prev, egui::Stroke::new(2.0_f32, Color32::from_rgb(255, 0, 200)));
            }

            // 打印框（PDF 的范围）。"印刷枠"按下时，框跟着光标走，点一下定下来
            let mut center = self.frame_center;
            if self.placing {
                if let Some(pos) = resp.hover_pos() {
                    center = view.to_paper(pos);
                }
            }
            let b = pdf::frame_box(center, self.frame_scale, page_mm(self.page));
            let corners = [
                P { x: b.x0, y: b.y0 },
                P { x: b.x1, y: b.y0 },
                P { x: b.x1, y: b.y1 },
                P { x: b.x0, y: b.y1 },
                P { x: b.x0, y: b.y0 },
            ];
            let pts: Vec<_> = corners.iter().map(|c| view.to_screen(*c)).collect();
            let col = if self.placing { Color32::from_rgb(255, 220, 0) } else { Color32::from_rgb(255, 120, 120) };
            painter.add(Shape::dashed_line(&pts, egui::Stroke::new(1.5_f32, col), 6.0, 4.0));

            // 拖动中的提示：两键 = 缩放的方向，左右 = 时钟菜单
            if let (Some(p), Some(hp)) = (self.ptr, ctx.pointer_hover_pos()) {
                let d = hp - p.start;
                let font = egui::FontId::proportional(14.0);
                let ink = Color32::from_rgb(0, 120, 255);
                if p.btn == 3 && !p.mods.shift {
                    let label = if d.length() < 6.0 {
                        "移動"
                    } else if d.x > 0.0 && d.y > 0.0 {
                        painter.rect_stroke(egui::Rect::from_two_pos(p.start, hp), 0.0, egui::Stroke::new(1.0_f32, ink), egui::StrokeKind::Middle);
                        "拡大"
                    } else if d.x < 0.0 && d.y < 0.0 {
                        "縮小"
                    } else if d.x > 0.0 {
                        "全体"
                    } else {
                        "前倍率"
                    };
                    painter.text(p.start, egui::Align2::CENTER_BOTTOM, label, font, ink);
                } else if p.btn != 3 && !p.mods.shift && p.out && self.tools.kind != Kind::Pen {
                    let h = clock_of(p.start, hp);
                    let lr = (p.btn == 2) as usize;
                    let far = d.length() >= FAR;
                    let n = clock_cmd(&self.env, lr, p.pm, far, h);
                    let what = if lr == 1 && !p.pm && n == 0 {
                        match h {
                            0 => "円周1/4点".to_string(),
                            3 => "中心点".to_string(),
                            6 => "オフセット".to_string(),
                            9 => "線上点".to_string(),
                            _ => String::new(),
                        }
                    } else if lr == 0 && !p.pm && self.tools.kind == Kind::Stretch && h == 8 {
                        "端点移動".to_string()
                    } else if lr == 0 && !p.pm && self.tools.kind == Kind::Envelope && h == 9 {
                        "中間線消去".to_string()
                    } else {
                        tools::cmd_label(n).map(|s| s.to_string()).unwrap_or_else(|| if n == 0 { String::new() } else { format!("({n})") })
                    };
                    let txt = format!("{}{}{}時 {}", if lr == 0 { "左" } else { "右" }, if p.pm { "PM" } else { "AM" }, h, what);
                    painter.line_segment([p.start, hp], egui::Stroke::new(1.0_f32, ink));
                    painter.text(hp + egui::vec2(12.0, -12.0), egui::Align2::LEFT_BOTTOM, txt, font, ink);
                }
            }
        });
    }
}

/// 把图形转成预览形状（高亮用）
fn tools_shape(v: &mut Vec<Prev>, s: &jww::Shape) {
    use jwwview::cad::Curve;
    match s {
        jww::Shape::Line { a, b } => v.push(Prev::Line(*a, *b)),
        jww::Shape::Arc(_) => {
            let e = jww::Ent { a: Default::default(), shape: s.clone() };
            match Curve::of(&e) {
                Some(Curve::Circ { c, r, a0, sweep, full }) => {
                    if full {
                        v.push(Prev::Circle(c, r))
                    } else {
                        v.push(Prev::Arc(c, r, a0, sweep))
                    }
                }
                _ => {
                    if let jww::Shape::Arc(a) = s {
                        v.push(Prev::Circle(a.c, a.r))
                    }
                }
            }
        }
        jww::Shape::Point(pt) => v.push(Prev::Mark(pt.p)),
        jww::Shape::Text(t) => {
            let e = jww::Ent { a: Default::default(), shape: s.clone() };
            let k = jwwview::edit::key_points(&e);
            let _ = t;
            v.push(Prev::Line(k[0], k[1]));
            v.push(Prev::Line(k[1], k[3]));
            v.push(Prev::Line(k[3], k[2]));
            v.push(Prev::Line(k[2], k[0]));
        }
        jww::Shape::Dim(d) => {
            tools_shape(v, &d.line.shape);
            tools_shape(v, &d.text.shape);
        }
        jww::Shape::Solid { pts, .. } => {
            let o = pts;
            for k in 0..4 {
                v.push(Prev::Line(o[k], o[(k + 1) % 4]));
            }
        }
        jww::Shape::Block { base, .. } => v.push(Prev::Mark(*base)),
    }
}

fn draw_prev(painter: &egui::Painter, view: &View, prev: &[Prev], stroke: egui::Stroke) {
    use jwwview::cad::{polar, p};
    let poly = |pts: Vec<P>| -> Vec<egui::Pos2> { pts.into_iter().map(|q| view.to_screen(q)).collect() };
    for x in prev {
        match *x {
            Prev::Line(a, b) => {
                painter.line_segment([view.to_screen(a), view.to_screen(b)], stroke);
            }
            Prev::Rect(a, b) => {
                let pts = poly(vec![a, p(b.x, a.y), b, p(a.x, b.y), a]);
                painter.add(Shape::line(pts, stroke));
            }
            Prev::Circle(c, r) => {
                painter.circle_stroke(view.to_screen(c), (r * view.zoom) as f32, stroke);
            }
            Prev::Arc(c, r, a0, sw) => {
                let n = 64;
                let pts = poly((0..=n).map(|k| polar(c, r, a0 + sw * k as f64 / n as f64)).collect());
                painter.add(Shape::line(pts, stroke));
            }
            Prev::Mark(q) => {
                let s = view.to_screen(q);
                painter.circle_stroke(s, 3.5, stroke);
            }
        }
    }
}

enum KeyIdx {
    Letter(usize),
    F(usize),
}

fn key_index(k: egui::Key) -> Option<KeyIdx> {
    use egui::Key::*;
    const LETTERS: [egui::Key; 26] = [A, B, C, D, E, F, G, H, I, J, K, L, M, N, O, P, Q, R, S, T, U, V, W, X, Y, Z];
    const FK: [egui::Key; 8] = [F2, F3, F4, F5, F6, F7, F8, F9];
    if let Some(i) = LETTERS.iter().position(|x| *x == k) {
        return Some(KeyIdx::Letter(i));
    }
    FK.iter().position(|x| *x == k).map(KeyIdx::F)
}

/// 记住用户选的环境设定文件（家目录下 .jwwviewer_env 里写路径）
fn env_memo() -> Option<PathBuf> {
    std::env::home_dir().map(|h| h.join(".jwwviewer_env"))
}

fn load_env() -> Env {
    let chosen = env_memo().and_then(|m| std::fs::read_to_string(m).ok()).map(|s| PathBuf::from(s.trim()));
    if let Some(p) = chosen {
        if let Ok(b) = std::fs::read(&p) {
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            return Env::parse(&b, &name);
        }
    }
    Env::company()
}

/// 图层组・图层的状态（Jw_cad 的写法）：0 非表示、1 表示のみ、2 編集可能、3 書込
/// 左键：編集可能 → 非表示 → 表示のみ → 編集可能（書込的不变）
fn next_state(s: u32) -> u32 {
    match s {
        2 => 0,
        0 => 1,
        1 => 2,
        s => s,
    }
}

/// グループ・レイヤボタンの大きさ（正方形、円の直径も同じ）
const LB: f32 = 30.0;
/// データがある印（Jw_cad：図形があれば下線の左半分、文字があれば右半分）
const DATA_MARK: Color32 = Color32::from_rgb(230, 0, 140);

/// 一个按钮：方块（图层组）或圆（图层）。返回（左键, 右键）
fn state_button(ui: &mut egui::Ui, label: &str, state: u32, round: bool, (has_line, has_text): (bool, bool), tip: String) -> (bool, bool) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(LB, LB + 4.0), Sense::click());
    let p = ui.painter();
    let r = egui::Rect::from_min_size(rect.min, egui::vec2(LB, LB)).shrink(1.0);
    let stroke_dark = egui::Stroke::new(1.0_f32, Color32::from_gray(90));
    let txt = |c: Color32| p.text(r.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::monospace(17.0), c);
    let shape = |fill: Color32, stroke: egui::Stroke| {
        if round {
            p.circle(r.center(), r.width() / 2.0, fill, stroke);
        } else {
            p.rect(r, 1.0, fill, stroke, egui::StrokeKind::Inside);
        }
    };
    match state {
        3 => {
            // 書込：凹下去、红框
            shape(Color32::from_gray(200), egui::Stroke::new(2.0_f32, Color32::from_rgb(220, 30, 30)));
            txt(Color32::BLACK);
        }
        2 => {
            shape(Color32::from_gray(245), stroke_dark);
            txt(Color32::BLACK);
        }
        // 表示のみ：只有字
        1 => {
            txt(ui.visuals().text_color());
        }
        // 非表示：空白
        _ => {}
    }
    let y = rect.bottom() - 1.5;
    let stroke = egui::Stroke::new(2.0_f32, DATA_MARK);
    if has_line {
        p.line_segment([egui::pos2(r.left() + 1.0, y), egui::pos2(r.center().x, y)], stroke);
    }
    if has_text {
        p.line_segment([egui::pos2(r.center().x, y), egui::pos2(r.right() - 1.0, y)], stroke);
    }
    let resp = resp.on_hover_text(tip);
    (resp.clicked(), resp.secondary_clicked())
}

/// 右边：上にグループ（方块）、中に線属性、下にレイヤ（圆）。左键切换状态，右键设为書込
fn layer_bars(ui: &mut egui::Ui, ed: &mut Editor, line_attr_open: &mut bool) {
    // (図形がある, 文字がある)
    let mut gdata = [(false, false); 16];
    let mut ldata = [[(false, false); 16]; 16];
    for e in &ed.doc.ents {
        let (g, l) = (e.a.glayer as usize & 15, e.a.layer as usize & 15);
        let (line, text) = match e.shape {
            jww::Shape::Text(_) => (false, true),
            jww::Shape::Dim(_) => (true, true),
            _ => (true, false),
        };
        for d in [&mut gdata[g], &mut ldata[g][l]] {
            d.0 |= line;
            d.1 |= text;
        }
    }
    let state_name = |s: u32| ["非表示", "表示のみ", "編集可能", "書込"][s.min(3) as usize];
    let title = |ui: &mut egui::Ui, s: &str| {
        ui.vertical_centered(|ui| ui.label(egui::RichText::new(s).size(11.0)));
    };
    let mut set_w: Option<(usize, usize)> = None;
    // ---- 图层组 ----
    title(ui, "グループ");
    egui::Grid::new("gbar").num_columns(2).min_col_width(LB).spacing([2.0, 1.0]).show(ui, |ui| {
        for row in 0..8 {
            for g in [row, row + 8] {
                let gl = &ed.doc.glayers[g];
                let tip = format!("[{g:X}] {}  1/{}　{}
左クリック：状態切替　右クリック：書込", gl.name, gl.scale, state_name(gl.state));
                let (l, r) = state_button(ui, &format!("{g:X}"), gl.state, false, gdata[g], tip);
                if r {
                    set_w = Some((g, ed.doc.glayers[g].write_lay as usize & 15));
                } else if l {
                    let st = &mut ed.doc.glayers[g].state;
                    *st = next_state(*st);
                }
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        if ui.small_button("All").on_hover_text("全グループを編集可能に").clicked() {
            for g in ed.doc.glayers.iter_mut() {
                if g.state != 3 {
                    g.state = 2;
                }
            }
        }
        if ui.small_button("×").on_hover_text("書込グループ以外を非表示に").clicked() {
            for g in ed.doc.glayers.iter_mut() {
                if g.state != 3 {
                    g.state = 0;
                }
            }
        }
    });
    if let Some((g, l)) = set_w.take() {
        ed.set_write(g, l);
    }
    // ---- 線属性（グループとレイヤの間）----
    ui.add_space(8.0);
    if line_attr_button(ui, ed) {
        *line_attr_open = !*line_attr_open;
    }
    ui.add_space(8.0);
    // ---- 图层（書込グループの中）----
    let wg = ed.glay();
    title(ui, "レイヤ");
    egui::Grid::new("lbar").num_columns(2).min_col_width(LB).spacing([2.0, 1.0]).show(ui, |ui| {
        for row in 0..8 {
            for l in [row, row + 8] {
                let gl = &ed.doc.glayers[wg];
                let tip = format!("({l:X}) {}　{}
左クリック：状態切替　右クリック：書込", gl.lay_name[l], state_name(gl.lay_state[l]));
                let (lc, rc) = state_button(ui, &format!("{l:X}"), gl.lay_state[l], true, ldata[wg][l], tip);
                if rc {
                    set_w = Some((wg, l));
                } else if lc {
                    let st = &mut ed.doc.glayers[wg].lay_state[l];
                    *st = next_state(*st);
                }
            }
            ui.end_row();
        }
    });
    ui.horizontal(|ui| {
        if ui.small_button("All").on_hover_text("このグループの全レイヤを編集可能に").clicked() {
            for s in ed.doc.glayers[wg].lay_state.iter_mut() {
                if *s != 3 {
                    *s = 2;
                }
            }
        }
        ui.label(format!("{wg:X}")).on_hover_text("書込グループ");
    });
    if let Some((g, l)) = set_w {
        ed.set_write(g, l);
    }
}

/// 線の見本：黒地に、線色 color・線種 style で a → b
fn line_sample(p: &egui::Painter, a: egui::Pos2, b: egui::Pos2, doc: &Doc, color: u16, style: u8) {
    let c = doc.pens.get(color as usize).map(|x| x.color).unwrap_or(0xFFFFFF);
    let (r, g, bl) = jwwview::draw::rgb(c);
    let stroke = egui::Stroke::new(2.0_f32, Color32::from_rgb(r, g, bl));
    let Some((runs, pitch, _)) = jwwview::draw::dash_cells(doc, style) else {
        p.line_segment([a, b], stroke);
        return;
    };
    // 1 マス = pitch ドット（画面）を 1.5 倍
    let cell = pitch as f32 * 1.5;
    let len = b.x - a.x;
    let mut x = 0.0;
    'outer: loop {
        for (k, n) in runs.iter().enumerate() {
            let w = *n as f32 * cell;
            if k % 2 == 0 && w > 0.0 {
                let x1 = (x + w).min(len);
                p.line_segment([egui::pos2(a.x + x, a.y), egui::pos2(a.x + x1, a.y)], stroke);
            }
            x += w;
            if x >= len {
                break 'outer;
            }
        }
        if runs.iter().all(|n| *n == 0) {
            break;
        }
    }
}

/// 右下の線属性ボタン：今の線色・線種を見本で。押すと線属性ダイアログ
fn line_attr_button(ui: &mut egui::Ui, ed: &Editor) -> bool {
    ui.vertical_centered(|ui| ui.label(egui::RichText::new("線属性").size(11.0)));
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(LB * 2.0 + 2.0, 24.0), Sense::click());
    let p = ui.painter();
    p.rect(rect, 2.0, Color32::BLACK, egui::Stroke::new(1.0_f32, Color32::from_gray(120)), egui::StrokeKind::Inside);
    let y = rect.center().y;
    line_sample(p, egui::pos2(rect.left() + 4.0, y), egui::pos2(rect.right() - 4.0, y), &ed.doc, ed.pen.color, ed.pen.style);
    let c = (ed.pen.color as usize).clamp(1, 9);
    let s = (ed.pen.style as usize).clamp(1, 9);
    resp.on_hover_text(format!("{}・{}（クリックで線属性）", LINE_COLORS[c - 1], LINE_STYLES[s - 1])).clicked()
}

/// 線属性ダイアログ（Jw_cad と同じ並び：左に線色、右に線種、見本付き）
fn line_attr_window(ctx: &egui::Context, ed: &mut Editor, open: &mut bool) {
    let mut close = false;
    egui::Window::new("線属性").collapsible(false).resizable(false).open(open).show(ctx, |ui| {
        egui::Grid::new("lattr").num_columns(4).spacing([6.0, 4.0]).show(ui, |ui| {
            for k in 0..9 {
                let (color, style) = (k as u16 + 1, k as u8 + 1);
                // 線色：その色の実線
                let on = ed.pen.color == color;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(64.0, 20.0), Sense::click());
                let p = ui.painter();
                p.rect(rect, 2.0, Color32::BLACK, egui::Stroke::new(if on { 2.0_f32 } else { 1.0 }, if on { Color32::from_rgb(80, 160, 255) } else { Color32::from_gray(120) }), egui::StrokeKind::Inside);
                line_sample(p, egui::pos2(rect.left() + 6.0, rect.center().y), egui::pos2(rect.right() - 6.0, rect.center().y), &ed.doc, color, 1);
                if resp.clicked() {
                    ed.pen.color = color;
                }
                ui.label(if on { format!("✔ {}", LINE_COLORS[k]) } else { LINE_COLORS[k].to_string() });
                // 線種：今の線色で
                let on = ed.pen.style == style;
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(64.0, 20.0), Sense::click());
                let p = ui.painter();
                p.rect(rect, 2.0, Color32::BLACK, egui::Stroke::new(if on { 2.0_f32 } else { 1.0 }, if on { Color32::from_rgb(80, 160, 255) } else { Color32::from_gray(120) }), egui::StrokeKind::Inside);
                line_sample(p, egui::pos2(rect.left() + 6.0, rect.center().y), egui::pos2(rect.right() - 6.0, rect.center().y), &ed.doc, ed.pen.color, style);
                if resp.clicked() {
                    ed.pen.style = style;
                }
                ui.label(if on { format!("✔ {}", LINE_STYLES[k]) } else { LINE_STYLES[k].to_string() });
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        ui.vertical_centered(|ui| {
            if ui.button("　Ok　").clicked() {
                close = true;
            }
        });
    });
    if close {
        *open = false;
    }
}

/// 网页版：画在 index.html 的 <canvas id="jww">
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;
    wasm_bindgen_futures::spawn_local(async {
        let doc = web_sys::window().unwrap().document().unwrap();
        let canvas = doc.get_element_by_id("jww").unwrap().dyn_into::<web_sys::HtmlCanvasElement>().unwrap();
        let r = eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(App::new(cc, None)))))
            .await;
        if let Some(el) = doc.get_element_by_id("loading") {
            match r {
                Ok(_) => el.remove(),
                Err(e) => el.set_inner_html(&format!("起動できません：{e:?}")),
            }
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    // 旧版 macOS 从访达启动时会带一个 -psn_… 参数，不是文件
    let path = std::env::args().skip(1).find(|a| !a.starts_with("-psn")).map(PathBuf::from);
    #[cfg(target_os = "macos")]
    macos_open::install();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_title("JWW Viewer")
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("JWW Viewer", options, Box::new(move |cc| Ok(Box::new(App::new(cc, path)))))
}

/// 时钟菜单的命令号（环境设定的 LD_AM 等）。拖得远时先看 (2)
fn clock_cmd(env: &Env, lr: usize, pm: bool, far: bool, h: usize) -> i32 {
    let near = env.clock[lr][pm as usize][h];
    if far {
        let v = env.clock2[lr][pm as usize][h];
        if v != 0 {
            return v;
        }
    }
    near
}
