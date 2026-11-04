//! 网页版（wasm）专用：开文件、下载、确认对话框、关网页时的提醒。Mac/Windows 版不用这个文件。
//!
//! 浏览器里选文件是异步的：选好后把「文件名 + 内容」放进 Inbox，下一帧取出来打开。

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use wasm_bindgen::JsCast;

/// 选好的文件：(文件名, 内容)
pub type Inbox = Rc<RefCell<Vec<(PathBuf, Vec<u8>)>>>;

pub fn confirm(msg: &str) -> bool {
    web_sys::window().and_then(|w| w.confirm_with_message(msg).ok()).unwrap_or(true)
}

/// 弹出选文件（.jww 或环境设定 .jwf），选好后放进 inbox
pub fn pick(inbox: &Inbox, name: &'static str, ext: &'static [&'static str]) {
    let inbox = inbox.clone();
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(f) = rfd::AsyncFileDialog::new().add_filter(name, ext).pick_file().await {
            let n = f.file_name();
            let bytes = f.read().await;
            inbox.borrow_mut().push((PathBuf::from(n), bytes));
        }
    });
}

/// 下载文件（进浏览器的「下载」文件夹）。返回实际的文件名。
/// 在 claude.ai 的 Artifact 里打开时，走 Artifact 的下载接口（会问一下）；
/// 那里只收 PDF 等格式，.jww・.jws 打成 zip（解压就是原文件）
pub fn download(name: &str, bytes: &[u8], mime: &str) -> Result<String, String> {
    if let Some(claude) = artifact_host() {
        let lower = name.to_ascii_lowercase();
        let (fname, data) = if lower.ends_with(".pdf") { (name.to_string(), bytes.to_vec()) } else { (format!("{name}.zip"), zip_store(name, bytes)) };
        let n = fname.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(e) = artifact_save(&claude, &n, &data).await {
                web_sys::console::warn_1(&e);
            }
        });
        return Ok(fname);
    }
    let err = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let arr = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&arr.buffer());
    let opt = web_sys::BlobPropertyBag::new();
    opt.set_type(mime);
    let blob = web_sys::Blob::new_with_buffer_source_sequence_and_options(&parts, &opt).map_err(err)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(err)?;
    let doc = web_sys::window().and_then(|w| w.document()).ok_or("no document")?;
    let a: web_sys::HtmlAnchorElement = doc.create_element("a").map_err(err)?.dyn_into().map_err(|_| "anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    Ok(name.to_string())
}

/// claude.ai の Artifact の中か（window.claude.use がある）
fn artifact_host() -> Option<wasm_bindgen::JsValue> {
    let w = web_sys::window()?;
    let c = js_sys::Reflect::get(&w, &"claude".into()).ok()?;
    let u = js_sys::Reflect::get(&c, &"use".into()).ok()?;
    u.is_function().then_some(c)
}

/// await claude.use("downloads") → save({filename, data})
async fn artifact_save(claude: &wasm_bindgen::JsValue, name: &str, data: &[u8]) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen_futures::JsFuture;
    let use_fn: js_sys::Function = js_sys::Reflect::get(claude, &"use".into())?.dyn_into()?;
    let p: js_sys::Promise = use_fn.call1(claude, &"downloads".into())?.dyn_into()?;
    let dl = JsFuture::from(p).await?;
    if dl.is_null() || dl.is_undefined() {
        return Err("downloads unavailable".into());
    }
    let save: js_sys::Function = js_sys::Reflect::get(&dl, &"save".into())?.dyn_into()?;
    let req = js_sys::Object::new();
    js_sys::Reflect::set(&req, &"filename".into(), &name.into())?;
    js_sys::Reflect::set(&req, &"data".into(), &js_sys::Uint8Array::from(data))?;
    let p: js_sys::Promise = save.call1(&dl, &req)?.dyn_into()?;
    JsFuture::from(p).await?;
    Ok(())
}

/// 不压缩的 zip（文件一个）
fn zip_store(name: &str, data: &[u8]) -> Vec<u8> {
    let crc = {
        let mut c: u32 = !0;
        for &b in data {
            c ^= b as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
        }
        !c
    };
    let n = name.as_bytes();
    let size = data.len() as u32;
    let mut z = Vec::with_capacity(data.len() + 2 * n.len() + 100);
    let u16le = |z: &mut Vec<u8>, v: u16| z.extend_from_slice(&v.to_le_bytes());
    let u32le = |z: &mut Vec<u8>, v: u32| z.extend_from_slice(&v.to_le_bytes());
    // ローカルヘッダ（ファイル名は UTF-8：フラグ bit 11）
    u32le(&mut z, 0x0403_4b50);
    u16le(&mut z, 20);
    u16le(&mut z, 0x0800);
    u16le(&mut z, 0);
    u32le(&mut z, 0);
    u32le(&mut z, crc);
    u32le(&mut z, size);
    u32le(&mut z, size);
    u16le(&mut z, n.len() as u16);
    u16le(&mut z, 0);
    z.extend_from_slice(n);
    z.extend_from_slice(data);
    let cd = z.len() as u32;
    // 中央ディレクトリ
    u32le(&mut z, 0x0201_4b50);
    u16le(&mut z, 20);
    u16le(&mut z, 20);
    u16le(&mut z, 0x0800);
    u16le(&mut z, 0);
    u32le(&mut z, 0);
    u32le(&mut z, crc);
    u32le(&mut z, size);
    u32le(&mut z, size);
    u16le(&mut z, n.len() as u16);
    for _ in 0..4 {
        u16le(&mut z, 0);
    }
    u32le(&mut z, 0);
    u32le(&mut z, 0);
    z.extend_from_slice(n);
    let cd_len = z.len() as u32 - cd;
    u32le(&mut z, 0x0605_4b50);
    u16le(&mut z, 0);
    u16le(&mut z, 0);
    u16le(&mut z, 1);
    u16le(&mut z, 1);
    u32le(&mut z, cd_len);
    u32le(&mut z, cd);
    u16le(&mut z, 0);
    z
}

thread_local! {
    static DIRTY: Cell<bool> = const { Cell::new(false) };
    static HOOKED: Cell<bool> = const { Cell::new(false) };
}

/// 有没存的修改：关网页・刷新时浏览器会问「このサイトを離れますか？」
pub fn set_dirty(dirty: bool) {
    DIRTY.with(|d| d.set(dirty));
    if HOOKED.with(|h| h.replace(true)) {
        return;
    }
    let Some(w) = web_sys::window() else { return };
    let cb = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::BeforeUnloadEvent)>::new(|e: web_sys::BeforeUnloadEvent| {
        if DIRTY.with(|d| d.get()) {
            e.prevent_default();
            e.set_return_value("変更が保存されていません");
        }
    });
    let _ = w.add_event_listener_with_callback("beforeunload", cb.as_ref().unchecked_ref());
    cb.forget();
}
