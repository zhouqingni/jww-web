//! macOS：在访达里双击 .jww、「打开方式」、拖到程序坞图标上时，系统不是通过命令行参数，
//! 而是给 NSApplication 的委托发 `application:openURLs:`。
//!
//! 委托是 winit 自己的（换掉它 winit 会 panic），所以不换委托，而是在程序启动前给 NSObject
//! 补一个 `application:openURLs:` 方法，winit 的委托继承到它就会响应。双击启动时这个消息
//! 在 applicationDidFinishLaunching 之前就来，所以要在 eframe 启动前装好。
//! 收到的路径放进队列，界面每帧取走。

use objc2::ffi;
use objc2::runtime::{AnyObject, Sel};
use objc2::sel;
use objc2_foundation::{NSArray, NSURL};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

static PENDING: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static CTX: OnceLock<eframe::egui::Context> = OnceLock::new();

extern "C" fn open_urls(_this: *mut AnyObject, _cmd: Sel, _app: *mut AnyObject, urls: *mut NSArray<NSURL>) {
    // 从 Objective-C 调过来，这里不能 panic
    let Some(urls) = (unsafe { urls.as_ref() }) else { return };
    let Ok(mut q) = PENDING.lock() else { return };
    for u in urls.iter() {
        if let Some(s) = unsafe { u.path() } {
            q.push(PathBuf::from(s.to_string()));
        }
    }
    if let Some(c) = CTX.get() {
        c.request_repaint();
    }
}

/// main 里、eframe 启动前调用
pub fn install() {
    unsafe {
        let cls = ffi::objc_getClass(c"NSObject".as_ptr()) as *mut ffi::objc_class;
        if cls.is_null() {
            return;
        }
        let f: extern "C" fn(*mut AnyObject, Sel, *mut AnyObject, *mut NSArray<NSURL>) = open_urls;
        let imp: ffi::IMP = Some(std::mem::transmute::<_, unsafe extern "C" fn()>(f));
        // v = 返回 void，@ = self，: = 选择子，@ = NSApplication，@ = NSArray
        ffi::class_addMethod(cls, sel!(application:openURLs:).as_ptr(), imp, c"v@:@@".as_ptr());
    }
}

/// 窗口建好后告诉它界面在哪，收到文件时好叫醒界面
pub fn set_ctx(ctx: &eframe::egui::Context) {
    let _ = CTX.set(ctx.clone());
}

/// 取走收到的文件
pub fn take() -> Vec<PathBuf> {
    PENDING.lock().map(|mut q| std::mem::take(&mut *q)).unwrap_or_default()
}
