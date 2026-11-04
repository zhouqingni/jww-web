//! 作图命令的操作（界面部分）。手感照 Jw_cad：
//! 左键 = 任意点（光标所在位置），右键 = 读取点（端点・交点吸附）。
//! 选线的步骤左右键都行。Esc = 取消当前步骤。

use crate::cad::*;
use crate::edit::{key_points, DimDir, Editor, PolyMode};
use crate::fill::{path_len, signed_area, FillColor};
use crate::jww::{Ent, Shape, P};
use crate::ops::{End, TanObj};
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    None,
    Point,
    Line,
    Rect,
    Circle,
    Polygon,
    Offset,
    TwoLine,
    Center,
    Divide,
    Corner,
    Stretch,
    Chamfer,
    Tangent,
    TanCircle,
    Erase,
    TakeAttr,
    ChangeAttr,
    Copy,
    Move,
    Dim,
    Text,
    Solid,
    Hatch,
    Measure,
    Tidy,
    /// 範囲選択：选好后再选複写・移動・消去・属変・コピー，就对选中的图形做
    Range,
    /// コピー（放进本程序的剪贴板）・貼付
    ClipCopy,
    Paste,
    /// 図形（.jws）を読んで置く
    Figure,
    /// パラメトリック変形
    Param,
    /// 曲線（スプライン）
    Spline,
    /// ペン（ベジェ曲線。クリック = 角、ドラッグ = 滑らか）
    Pen,
    /// 寸解
    DimExplode,
    /// 包絡
    Envelope,
    /// 図登（選んだ図形を .jws に保存）
    FigReg,
    /// 記変（線記号変形）
    Symbol,
    /// AUTO モード
    Auto,
}

/// 一次点击：右键・双击（第二下鼠标没动）・Shift・Ctrl
#[derive(Clone, Copy, Debug, Default)]
pub struct Btn {
    pub right: bool,
    pub dbl: bool,
    pub shift: bool,
    pub ctrl: bool,
}

/// 範囲選択：普通 / 追加範囲 / 除外範囲
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RangeMode {
    Normal,
    Add,
    Remove,
}

/// 右ドラッグの読取りで、もう一下要るもの
#[derive(Clone, Debug)]
pub enum Rd {
    /// 2点間中心（1 点目）
    Mid(P),
    /// 線上点（線）
    OnLine(usize),
    /// オフセット（読取点、入力中の文字）
    Offset(P, String),
}

#[derive(Clone, Copy, Debug)]
enum Div {
    Curve(usize, P),
    Pt(P),
}

/// 伸縮一括処理の伸縮先
#[derive(Clone, Copy, Debug)]
enum Target {
    Line(usize, P),
    Pt(P),
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum AutoSub {
    None,
    /// 部分消しの始点
    Part(P),
    /// 指定点までの伸縮
    Stretch,
}

/// 取值（Jw_cad 的 線角・2点角・線長・2点長・間隔）：量出来的值填进当前命令的输入框
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    LineAngle,
    PtAngle,
    LineLen,
    PtLen,
    Gap,
}

pub const PICKS: &[(Pick, &str)] = &[
    (Pick::LineAngle, "線角"),
    (Pick::PtAngle, "2点角"),
    (Pick::LineLen, "線長"),
    (Pick::PtLen, "2点長"),
    (Pick::Gap, "間隔"),
];

fn num_str(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.into()
    }
}

/// 工具面板：(命令, 按钮文字)
pub const PALETTE: &[(Kind, &str)] = &[
    (Kind::Range, "範囲"),
    (Kind::Point, "点"),
    (Kind::Line, "／"),
    (Kind::Rect, "□"),
    (Kind::Circle, "○"),
    (Kind::Text, "文字"),
    (Kind::Dim, "寸法"),
    (Kind::TwoLine, "２線"),
    (Kind::Center, "中心線"),
    (Kind::Offset, "複線"),
    (Kind::Corner, "コーナー"),
    (Kind::Stretch, "伸縮"),
    (Kind::Chamfer, "面取"),
    (Kind::Erase, "消去"),
    (Kind::Copy, "複写"),
    (Kind::Move, "移動"),
    (Kind::Divide, "分割"),
    (Kind::Tangent, "接線"),
    (Kind::TanCircle, "接円"),
    (Kind::Polygon, "多角形"),
    (Kind::TakeAttr, "属取"),
    (Kind::ChangeAttr, "属変"),
    (Kind::Solid, "塗り"),
    (Kind::Hatch, "ハッチ"),
    (Kind::Measure, "測定"),
    (Kind::Tidy, "整理"),
    (Kind::ClipCopy, "コピー"),
    (Kind::Paste, "貼付"),
    (Kind::Figure, "図形"),
    (Kind::Param, "パラメ"),
    (Kind::Spline, "曲線"),
    (Kind::Pen, "ペン"),
    (Kind::DimExplode, "寸解"),
    (Kind::Envelope, "包絡"),
    (Kind::FigReg, "図登"),
    (Kind::Symbol, "記変"),
    (Kind::Auto, "AUTO"),
];

/// 左の工具パネルの並び（周青妮さん指定）：(左列, 右列) を一行ずつ。None = 印刷
pub const SIDE: &[((Option<Kind>, &str), (Option<Kind>, &str))] = &[
    ((Some(Kind::Range), "範囲"), (Some(Kind::Point), "点")),
    ((Some(Kind::Copy), "複写"), (Some(Kind::Circle), "○")),
    ((Some(Kind::Move), "移動"), (Some(Kind::Rect), "□")),
    ((Some(Kind::Erase), "消去"), (Some(Kind::Line), "／")),
    ((Some(Kind::TakeAttr), "属取"), (Some(Kind::Offset), "複線")),
    ((Some(Kind::ChangeAttr), "属変"), (Some(Kind::TwoLine), "２線")),
    ((Some(Kind::ClipCopy), "ｺﾋﾟｰ"), (Some(Kind::Center), "中心線")),
    ((Some(Kind::Paste), "貼付"), (Some(Kind::Divide), "分割")),
    ((Some(Kind::Polygon), "多角形"), (Some(Kind::Stretch), "伸縮")),
    ((Some(Kind::Solid), "塗り"), (Some(Kind::Envelope), "包絡")),
    ((Some(Kind::Hatch), "ﾊｯﾁ"), (Some(Kind::Chamfer), "面取")),
    ((Some(Kind::Figure), "図形"), (Some(Kind::Corner), "ｺｰﾅｰ")),
    ((Some(Kind::FigReg), "図登"), (Some(Kind::Param), "ﾊﾟﾗﾒ")),
    ((Some(Kind::Symbol), "記変"), (Some(Kind::Tangent), "接線")),
    ((Some(Kind::Measure), "測定"), (Some(Kind::TanCircle), "接円")),
    ((Some(Kind::Dim), "寸法"), (Some(Kind::Spline), "曲線")),
    ((Some(Kind::Text), "文字"), (Some(Kind::Pen), "ペン")),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Free,
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimKind {
    Line,
    Radius,
    Diameter,
    /// 角度：中心 → 1 点目 → 2 点目 → 位置
    Angle,
    /// 弧長（円周）：円弧 → 位置
    ArcLen,
    /// 累進寸法
    Cumulative,
    /// 寸法値（値だけ作図・移動・変更）
    Value,
}

/// 预览用的临时图形
/// ペンのアンカー（ハンドルは絶対座標。角の点はハンドル = 点）
#[derive(Clone, Copy, Debug)]
pub struct Anchor {
    pub p: P,
    pub h_in: P,
    pub h_out: P,
}

/// ペンの経路を折れ線に（a → b の区間を div 分割。ハンドルがなければ直線一本）
pub fn pen_points(v: &[Anchor], closed: bool, div: usize) -> Vec<P> {
    let mut out: Vec<P> = Vec::new();
    let n = v.len();
    if n == 0 {
        return out;
    }
    out.push(v[0].p);
    let segs = if closed { n } else { n - 1 };
    for k in 0..segs {
        let (a, b) = (v[k], v[(k + 1) % n]);
        let (p0, c1, c2, p3) = (a.p, a.h_out, b.h_in, b.p);
        if dist(c1, p0) < 1e-12 && dist(c2, p3) < 1e-12 {
            out.push(p3);
            continue;
        }
        for s in 1..=div.max(1) {
            let t = s as f64 / div.max(1) as f64;
            let u = 1.0 - t;
            let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
            out.push(p(
                w[0] * p0.x + w[1] * c1.x + w[2] * c2.x + w[3] * p3.x,
                w[0] * p0.y + w[1] * c1.y + w[2] * c2.y + w[3] * p3.y,
            ));
        }
    }
    out
}

pub enum Prev {
    Line(P, P),
    Circle(P, f64),
    Arc(P, f64, f64, f64),
    Mark(P),
    Rect(P, P),
}

pub struct Tools {
    pub kind: Kind,
    pts: Vec<P>,
    picks: Vec<(usize, P)>,
    // ---- 各命令的参数（字符串，照 Jw_cad 可以输入「1割5分」「5000,10000」）----
    pub len: String,
    pub angle: String,
    pub hv: bool,
    pub cont: bool,
    pub rect_size: String,
    pub radius: String,
    pub three: bool,
    pub poly_n: u32,
    pub poly_size: String,
    pub poly_mode: PolyMode,
    pub poly_angle: String,
    pub offset: String,
    pub twoline: String,
    pub div_n: u32,
    pub chamfer: String,
    pub round: bool,
    pub tan_r: String,
    pub between: bool,
    pub batch: bool,
    pub axis: Axis,
    pub move_d: String,
    pub dim_dir: DimDir,
    pub dim_kind: DimKind,
    pub dim_double: bool,
    pub text: String,
    pub ch_color: bool,
    pub ch_style: bool,
    pub ch_layer: bool,
    /// 塗り：任意色（关 = 写入线色）和颜色
    pub fill_any: bool,
    pub fill_rgb: [u8; 3],
    pub hatch_angle: String,
    pub hatch_pitch: String,
    pub hatch_cross: bool,
    /// 測定：面积（关 = 距离）
    pub measure_area: bool,
    /// 取值中（命令本身不变，取完回到原命令）
    pub pick: Option<Pick>,
    pick_pts: Vec<P>,
    pick_line: Option<usize>,
    /// 剪贴板（换图纸也留着）和它的基准点
    pub clip: Vec<Ent>,
    clip_base: P,
    /// 貼付：放到书込图层（关 = 保持原来的图层）
    pub paste_to_write: bool,
    /// 図形：读进来的 .jws、文件名、倍率、回転角
    pub fig: Option<crate::jww::Jws>,
    pub fig_name: String,
    pub fig_mag: String,
    pub fig_rot: String,
    /// 上次选图形的文件夹
    pub fig_dir: Option<std::path::PathBuf>,
    /// 曲線の分割数（1 区間あたり）
    pub spline_div: u32,
    /// ペン：置いたアンカー、一区間の分割数、ドラッグ中（押した点, 今の点）
    pub pen: Vec<Anchor>,
    pub pen_div: u32,
    pub pen_live: Option<(P, P)>,
    /// 記変：読んだ線記号データ、選んだ記号、倍率、INPUT の文字（複数は「,」区切り）、フォルダ
    pub sym: Option<crate::symbol::SymFile>,
    pub sym_idx: usize,
    pub sym_mag: String,
    pub sym_text: String,
    pub sym_dir: Option<std::path::PathBuf>,
    /// 复写・移动：选好了，在等基点・目标点
    sel_done: bool,
    base: Option<P>,
    /// 複線「連続」用：上一次的结果和方向
    last_offset: Option<(usize, P)>,
    /// 消息（状态栏）
    pub note: Option<String>,
    // ---- Jw_cad の操作の途中経過 ----
    pub range_mode: RangeMode,
    pub rd: Option<Rd>,
    end_move: Option<(usize, P)>,
    perp_ref: Option<Curve>,
    auto_a: Option<(usize, P)>,
    auto_sub: AutoSub,
    auto_ret: bool,
    offset_prev: Option<f64>,
    offset_chain: Option<(usize, usize)>,
    tl_bases: Vec<(P, P)>,
    tl_start: Option<End>,
    tl_env: [Option<(usize, P)>; 2],
    pub tl_flip: bool,
    /// 留線（次の１回）・留線常駐・留線出（実寸）
    pub tl_cap: bool,
    pub tl_cap_always: bool,
    pub tl_cap_out: String,
    /// 中心線寸法（実寸、空 = マウスで終点）
    pub center_len: String,
    /// 分割：等角度、割付（距離・振分・割付距離以下）、逆分割、線長割合（比）、馬目地（縦分割数）
    pub div_angle: bool,
    pub div_wari: bool,
    pub div_dist: String,
    pub div_furi: bool,
    pub div_under: bool,
    pub div_rev: bool,
    pub div_ratio: Option<f64>,
    pub div_umame: Option<u32>,
    /// 寸法：一括処理中・始めの線、寸法値の移動・変更
    pub dim_batch: bool,
    dim_b_first: Option<(usize, P, bool)>,
    dim_val_move: Option<usize>,
    dim_val_edit: Option<usize>,
    pub dim_val_text: String,
    /// 文字「連」：連結・移動・切断。選んだ文字、前付けか、移動中の文字
    pub text_link: bool,
    link_src: Option<usize>,
    pub link_front: bool,
    link_move: Option<usize>,
    /// 塗り：円環ソリッドと内側半径（実寸）
    pub ring: bool,
    pub ring_r: String,
    center_start: Option<P>,
    center_link: [Option<(usize, P)>; 2],
    div_items: Vec<Div>,
    div_series: Option<Vec<P>>,
    /// 連続点分割で両端にも点
    pub div_ends: bool,
    stretch_base: Option<(usize, P)>,
    /// 伸縮の突出寸法（実寸）
    pub protrude: String,
    batch_target: Option<Target>,
    batch_first: Option<(usize, P, bool)>,
    batch_seg: Option<(P, P)>,
    tan_objs: Vec<(TanObj, P)>,
    /// 寸法：＝（引出線始点と寸法線の２点）か －（寸法線だけ）
    pub dim_eq: bool,
    dim_start: Option<P>,
    dim_last: Option<P>,
    dim_first: Option<P>,
    dim_count: usize,
    dim_two: bool,
    /// 文字：選んだ既存の文字（複写か）
    text_src: Option<(usize, bool)>,
    /// 塗り：円・連続線指示
    pub solid_from: bool,
    hatch_range: bool,
    fig_wait_base: bool,
    /// Web 版：ブラウザで選んだファイル
    #[cfg(target_arch = "wasm32")]
    web_in: crate::platform::Inbox,
    /// 状态栏用（bar() で更新）
    sel_empty: bool,
    invert_on: bool,
}

impl Default for Tools {
    fn default() -> Self {
        Tools {
            kind: Kind::None,
            pts: Vec::new(),
            picks: Vec::new(),
            len: String::new(),
            angle: String::new(),
            hv: false,
            cont: false,
            rect_size: String::new(),
            radius: String::new(),
            three: false,
            poly_n: 6,
            poly_size: String::new(),
            poly_mode: PolyMode::Vertex,
            poly_angle: "90".into(),
            offset: String::new(),
            twoline: "50,50".into(),
            div_n: 2,
            chamfer: String::new(),
            round: false,
            tan_r: String::new(),
            between: false,
            batch: false,
            axis: Axis::Free,
            move_d: String::new(),
            dim_dir: DimDir::Horizontal,
            dim_kind: DimKind::Line,
            dim_double: false,
            text: String::new(),
            ch_color: true,
            ch_style: true,
            ch_layer: true,
            fill_any: false,
            fill_rgb: [192, 192, 192],
            hatch_angle: "45".into(),
            hatch_pitch: "100".into(),
            hatch_cross: false,
            measure_area: false,
            pick: None,
            pick_pts: Vec::new(),
            pick_line: None,
            clip: Vec::new(),
            clip_base: P::default(),
            paste_to_write: false,
            fig: None,
            fig_name: String::new(),
            fig_mag: "1".into(),
            fig_rot: "0".into(),
            fig_dir: None,
            spline_div: 10,
            pen: Vec::new(),
            pen_div: 16,
            pen_live: None,
            sym: None,
            sym_idx: 0,
            sym_mag: "1".into(),
            sym_text: String::new(),
            sym_dir: None,
            sel_done: false,
            base: None,
            last_offset: None,
            note: None,
            range_mode: RangeMode::Normal,
            rd: None,
            end_move: None,
            perp_ref: None,
            auto_a: None,
            auto_sub: AutoSub::None,
            auto_ret: false,
            offset_prev: None,
            offset_chain: None,
            tl_bases: Vec::new(),
            tl_start: None,
            tl_env: [None, None],
            tl_flip: false,
            tl_cap: false,
            tl_cap_always: false,
            tl_cap_out: "0".into(),
            center_len: String::new(),
            div_angle: false,
            div_wari: false,
            div_dist: String::new(),
            div_furi: false,
            div_under: false,
            div_rev: false,
            div_ratio: None,
            div_umame: None,
            dim_batch: false,
            dim_b_first: None,
            dim_val_move: None,
            dim_val_edit: None,
            dim_val_text: String::new(),
            text_link: false,
            link_src: None,
            link_front: false,
            link_move: None,
            ring: false,
            ring_r: String::new(),
            center_start: None,
            center_link: [None, None],
            div_items: Vec::new(),
            div_series: None,
            div_ends: false,
            stretch_base: None,
            protrude: String::new(),
            batch_target: None,
            batch_first: None,
            batch_seg: None,
            tan_objs: Vec::new(),
            dim_eq: true,
            dim_start: None,
            dim_last: None,
            dim_first: None,
            dim_count: 0,
            dim_two: false,
            text_src: None,
            solid_from: false,
            hatch_range: false,
            fig_wait_base: false,
            #[cfg(target_arch = "wasm32")]
            web_in: Default::default(),
            sel_empty: true,
            invert_on: false,
        }
    }
}

fn num(s: &str) -> Option<f64> {
    parse_nums(s).first().copied().filter(|v| *v != 0.0)
}

fn is_line(e: &crate::jww::Ent) -> bool {
    matches!(e.shape, Shape::Line { .. })
}
fn is_circ(e: &crate::jww::Ent) -> bool {
    matches!(Curve::of(e), Some(Curve::Circ { .. }))
}
fn is_curve(e: &crate::jww::Ent) -> bool {
    Curve::of(e).is_some()
}

impl Tools {
    pub fn set(&mut self, k: Kind, ed: &mut Editor) {
        // 範囲選択で選んだ図形は、次の複写・移動・消去・属変にそのまま渡す
        let carry = (self.kind == Kind::Range && matches!(k, Kind::Copy | Kind::Move | Kind::Erase | Kind::ChangeAttr | Kind::ClipCopy | Kind::FigReg))
            .then(|| ed.sel.clone())
            .filter(|s| !s.is_empty());
        self.kind = k;
        self.reset(ed);
        if let Some(s) = carry {
            ed.sel = s;
            if k == Kind::Erase {
                self.base = Some(P::default());
            }
            self.confirm(ed);
        }
    }

    /// 取消当前步骤（参数保留）
    pub fn reset(&mut self, ed: &mut Editor) {
        self.finish_dim(ed);
        self.pts.clear();
        self.pen.clear();
        self.pen_live = None;
        self.picks.clear();
        self.sel_done = false;
        self.base = None;
        self.last_offset = None;
        ed.sel.clear();
        self.note = None;
        self.pick = None;
        self.pick_pts.clear();
        self.pick_line = None;
        self.range_mode = RangeMode::Normal;
        self.rd = None;
        self.end_move = None;
        self.perp_ref = None;
        self.auto_a = None;
        self.auto_sub = AutoSub::None;
        self.offset_chain = None;
        self.tl_bases.clear();
        self.tl_start = None;
        self.tl_env = [None, None];
        self.center_start = None;
        self.center_link = [None, None];
        self.div_items.clear();
        self.div_series = None;
        self.stretch_base = None;
        self.batch_target = None;
        self.batch_first = None;
        self.batch_seg = None;
        self.tan_objs.clear();
        self.dim_start = None;
        self.dim_last = None;
        self.dim_first = None;
        self.dim_count = 0;
        self.dim_two = false;
        self.text_src = None;
        self.dim_batch = false;
        self.dim_b_first = None;
        self.dim_val_move = None;
        self.dim_val_edit = None;
        self.link_src = None;
        self.link_move = None;
        self.hatch_range = false;
        self.fig_wait_base = false;
        ed.invert = false;
        if self.auto_ret {
            self.auto_ret = false;
            self.kind = Kind::Auto;
        }
    }

    /// 命令做到一半（Esc 先取消这一步，没有的话才是「戻る」）
    pub fn busy(&self, ed: &Editor) -> bool {
        !self.pts.is_empty()
            || !self.picks.is_empty()
            || !ed.sel.is_empty()
            || self.sel_done
            || self.pick.is_some()
            || self.rd.is_some()
            || self.end_move.is_some()
            || self.auto_a.is_some()
            || !self.tl_bases.is_empty()
            || self.center_start.is_some()
            || !self.div_items.is_empty()
            || self.div_series.is_some()
            || self.stretch_base.is_some()
            || self.batch_target.is_some()
            || !self.tan_objs.is_empty()
            || self.dim_start.is_some()
            || self.dim_last.is_some()
            || self.text_src.is_some()
            || self.fig_wait_base
            || !self.pen.is_empty()
            || ed.invert
    }

    /// ペン：クリック。始点をクリック = 閉じて作図、それ以外 = 角の点
    fn pen_click(&mut self, ed: &mut Editor, q: P, pt: Option<P>, tol: f64) {
        if self.pen.len() >= 2 && dist(q, self.pen[0].p) <= tol {
            self.pen_finish(ed, true);
            return;
        }
        let Some(x) = pt else {
            self.note = Some("点がありません".into());
            return;
        };
        self.pen.push(Anchor { p: x, h_in: x, h_out: x });
    }

    /// ペン：ドラッグ = 押した所に滑らかな点、離した所が出ていくハンドル（入るハンドルは反対側）。
    /// 右ドラッグは押した所の読取点。始点からドラッグ = 始点のハンドルを決めて閉じる
    pub fn pen_drag(&mut self, ed: &mut Editor, a: P, b: P, right: bool, tol: f64) {
        self.pen_live = None;
        let a = if right {
            match ed.read_point(a, tol) {
                Some(x) => x,
                None => {
                    self.note = Some("点がありません".into());
                    return;
                }
            }
        } else {
            a
        };
        let h_in = sub(mul(a, 2.0), b);
        if self.pen.len() >= 2 && dist(a, self.pen[0].p) <= tol {
            self.pen[0].h_in = sub(mul(self.pen[0].p, 2.0), b);
            self.pen_finish(ed, true);
            return;
        }
        self.pen.push(Anchor { p: a, h_in, h_out: b });
    }

    /// ペン：確定（Enter・「作図」、閉じる時は始点クリック）
    pub fn pen_finish(&mut self, ed: &mut Editor, closed: bool) {
        if self.pen.len() < 2 {
            return;
        }
        let pts = pen_points(&self.pen, closed, self.pen_div.max(1) as usize);
        ed.begin();
        let n = ed.polyline_curve(&pts).len();
        self.note = Some(format!("{n} 本の線で{}曲線を作図しました", if closed { "閉じた" } else { "" }));
        self.pen.clear();
        self.pen_live = None;
    }

    /// ペン：最後の点を取り消す（BackSpace）
    pub fn pen_back(&mut self) {
        self.pen.pop();
    }

    /// 开始取值（当前命令的步骤保留）
    pub fn start_pick(&mut self, m: Pick) {
        self.pick = Some(m);
        self.pick_pts.clear();
        self.pick_line = None;
        self.note = None;
    }

    /// 取到的长度（实寸）填进当前命令的输入框
    fn put_len(&mut self, v: f64, what: &str) {
        let s = num_str(v);
        let field = match self.kind {
            Kind::Line => Some(&mut self.len),
            Kind::Circle => Some(&mut self.radius),
            Kind::Offset => Some(&mut self.offset),
            Kind::Polygon => Some(&mut self.poly_size),
            Kind::Chamfer => Some(&mut self.chamfer),
            Kind::TanCircle => Some(&mut self.tan_r),
            Kind::Hatch => Some(&mut self.hatch_pitch),
            _ => None,
        };
        let put = field.is_some();
        if let Some(f) = field {
            *f = s.clone();
        }
        self.note = Some(format!("{what} {s}{}", if put { " を入力しました" } else { "" }));
    }

    /// 取到的角度（度）填进当前命令的输入框
    fn put_angle(&mut self, ed: &mut Editor, d: f64, what: &str) {
        let s = num_str(d);
        let put = match self.kind {
            Kind::Line => {
                self.angle = s.clone();
                true
            }
            Kind::Polygon => {
                self.poly_angle = s.clone();
                true
            }
            Kind::Hatch => {
                self.hatch_angle = s.clone();
                true
            }
            Kind::Dim => {
                self.dim_dir = DimDir::Angle(d);
                true
            }
            Kind::Text => {
                ed.moji_deg = d;
                true
            }
            _ => false,
        };
        self.note = Some(format!("{what} {s}°{}", if put { " を入力しました" } else { "" }));
    }

    /// 取值的点击。左键点线，右键读取点
    fn pick_click(&mut self, ed: &mut Editor, m: Pick, q: P, pt: Option<P>, tol: f64) {
        let line_at = |ed: &Editor, q: P| {
            ed.pick(q, tol, is_line).and_then(|i| match ed.doc.ents[i].shape {
                Shape::Line { a, b } => Some((i, a, b)),
                _ => None,
            })
        };
        // 线的角度取 0〜180°
        let ang = |a: P, b: P| {
            let d = deg(angle_of(sub(b, a))).rem_euclid(180.0);
            if d >= 180.0 - 1e-9 {
                0.0
            } else {
                d
            }
        };
        match m {
            Pick::LineAngle | Pick::LineLen => {
                let Some((_, a, b)) = line_at(ed, q) else {
                    self.note = Some("線を指示してください".into());
                    return;
                };
                self.pick = None;
                if m == Pick::LineAngle {
                    self.put_angle(ed, ang(a, b), "線角");
                } else {
                    self.put_len(ed.real(dist(a, b)), "線長");
                }
            }
            Pick::PtAngle | Pick::PtLen => {
                let Some(x) = pt else { return };
                self.pick_pts.push(x);
                if self.pick_pts.len() == 2 {
                    let (a, b) = (self.pick_pts[0], self.pick_pts[1]);
                    self.pick = None;
                    self.pick_pts.clear();
                    if m == Pick::PtAngle {
                        self.put_angle(ed, deg(angle_of(sub(b, a))), "2点角");
                    } else {
                        self.put_len(ed.real(dist(a, b)), "2点長");
                    }
                }
            }
            Pick::Gap => match self.pick_line {
                None => match line_at(ed, q) {
                    Some((i, _, _)) => self.pick_line = Some(i),
                    None => self.note = Some("基準の線を指示してください".into()),
                },
                Some(i) => {
                    let Shape::Line { a, b } = ed.doc.ents[i].shape else { return };
                    // 第二下：右键 = 读取点，左键 = 另一条线（取它中点到基准线的距离）
                    let x = match (pt, line_at(ed, q)) {
                        (Some(x), _) if x != q => x,
                        (_, Some((j, c, d))) if j != i => mid(c, d),
                        (Some(x), _) => x,
                        _ => return,
                    };
                    self.pick = None;
                    self.pick_line = None;
                    self.put_len(ed.real(dist(x, project(a, b, x))), "間隔");
                }
            },
        }
    }

    /// パラメ：方向固定をかけて変形
    fn apply_param(&mut self, ed: &mut Editor, d: P) {
        let d = match self.axis {
            Axis::Free => d,
            Axis::X => p(d.x, 0.0),
            Axis::Y => p(0.0, d.y),
        };
        ed.begin();
        let n = ed.parametric(self.pts[0], self.pts[1], d);
        self.note = Some(format!("{n} 個を変形しました"));
        self.pts.clear();
        self.base = None;
    }

    fn cur_sym(&self) -> Option<&crate::symbol::Sym> {
        self.sym.as_ref().and_then(|f| f.syms.get(self.sym_idx))
    }

    /// 記変：原点・角度（度）・指示線の属性
    fn sym_place(&self, ed: &Editor, q: P, pt: Option<P>) -> Option<(P, f64, Option<crate::jww::Attr>)> {
        let s = self.cur_sym()?;
        if !s.needs_line {
            return pt.map(|x| (x, 0.0, None));
        }
        let &(i, pick) = self.picks.first()?;
        let Shape::Line { a, b } = ed.doc.ents.get(i)?.shape else { return None };
        let attr = Some(ed.doc.ents[i].a.clone());
        if s.count == 0 {
            // 指示した位置に近い線端、線の内側へ向ける
            let (o, other) = if dist(pick, a) <= dist(pick, b) { (a, b) } else { (b, a) };
            return Some((o, deg(angle_of(sub(other, o))), attr));
        }
        let x = pt.unwrap_or(q);
        let mut u = sub(b, a);
        if u.x < -1e-12 || (u.x.abs() <= 1e-12 && u.y < 0.0) {
            u = mul(u, -1.0);
        }
        Some((project(a, b, x), deg(angle_of(u)), attr))
    }

    fn sym_inputs(&self) -> Vec<String> {
        self.sym_text.split([',', '，']).map(|s| s.trim().to_string()).collect()
    }

    /// 図形ファイルを読む
    /// 図形ファイルの中身から（Web 版）
    pub fn load_figure_bytes(&mut self, b: &[u8], name: &str) {
        match crate::jww::read_jws_bytes(b) {
            Ok(j) => {
                self.fig_name = name.trim_end_matches(".jws").trim_end_matches(".JWS").to_string();
                self.fig = Some(j);
                self.note = None;
            }
            Err(e) => self.note = Some(format!("図形を読めません：{}", e.0)),
        }
    }

    pub fn load_figure(&mut self, path: &std::path::Path) {
        match crate::jww::read_jws(path) {
            Ok(j) => {
                self.fig_name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.fig = Some(j);
                self.fig_dir = path.parent().map(|p| p.to_path_buf());
                self.note = None;
            }
            Err(e) => self.note = Some(format!("図形を読めません：{}", e.0)),
        }
    }

    fn fig_params(&self) -> (f64, f64) {
        (num(&self.fig_mag).unwrap_or(1.0), parse_angle(&self.fig_rot).unwrap_or(0.0))
    }

    /// コピー：选中的图形放进剪贴板，基准点 = 选中图形的中心
    pub fn copy_selection(&mut self, ed: &mut Editor) -> usize {
        let mut bb = crate::geom::BBox::EMPTY;
        self.clip = ed.sel.iter().filter_map(|&i| ed.doc.ents.get(i).cloned()).collect();
        for e in &self.clip {
            for q in key_points(e) {
                bb.add(q);
            }
        }
        self.clip_base = if bb.is_empty() { P::default() } else { p((bb.x0 + bb.x1) / 2.0, (bb.y0 + bb.y1) / 2.0) };
        let n = self.clip.len();
        ed.sel.clear();
        self.sel_done = false;
        self.pts.clear();
        self.note = Some(format!("{n} 個コピーしました"));
        n
    }

    /// 貼付：剪贴板的图形放到 x（基准点对齐）
    fn paste_at(&mut self, ed: &mut Editor, x: P) {
        if self.clip.is_empty() {
            self.note = Some("コピーした図形がありません".into());
            return;
        }
        ed.begin();
        let d = sub(x, self.clip_base);
        let (g, l) = (ed.glay() as u16, ed.lay() as u16);
        for mut e in self.clip.clone() {
            crate::edit::translate(&mut e.shape, d);
            if self.paste_to_write {
                e.a.glayer = g;
                e.a.layer = l;
            }
            ed.doc.ents.push(e);
        }
        self.note = Some(format!("{} 個貼り付けました", self.clip.len()));
    }

    /// Jw_cad 的命令号（基本设定 KEY 页「コマンド一覧」）→ 本程序的命令。
    /// 返回 false = 本程序还没有这个命令
    pub fn command(&mut self, n: u32, ed: &mut Editor) -> bool {
        // 156 = 基準点変更（複写・移動・パラメ）
        if n == 156 || n == 155 {
            if matches!(self.kind, Kind::Copy | Kind::Move | Kind::Param) {
                self.base = None;
                self.note = Some("基準点を指示してください".into());
            }
            return true;
        }
        if let 87 | 90 | 92 | 93 | 95 = n {
            self.start_pick(match n {
                87 => Pick::LineAngle,
                90 => Pick::PtAngle,
                92 => Pick::LineLen,
                93 => Pick::PtLen,
                _ => Pick::Gap,
            });
            return true;
        }
        let Some(k) = kind_of(n) else { return false };
        self.set(k, ed);
        if k == Kind::Line {
            // 10 = 連続線
            self.cont = n == 10;
        }
        true
    }

    /// 空格（Jw_cad 的 KEYSP）：線のとき「水平・垂直」を切り替え
    pub fn space(&mut self) {
        match self.kind {
            Kind::Line => self.hv = !self.hv,
            // 寸法：0° ⇔ 90°（Jw_cad のキーコマンド 76 = 方向変更）
            Kind::Dim => {
                self.dim_dir = if self.dim_dir == DimDir::Horizontal { DimDir::Vertical } else { DimDir::Horizontal };
            }
            _ => {}
        }
    }

    /// 寸法：二重寸法的总长在结束时补上
    fn finish_dim(&mut self, ed: &mut Editor) {
        if self.kind == Kind::Dim && self.dim_double && self.dim_count > 1 {
            if let (Some(first), Some(last), Some((at, lead))) = (self.dim_first, self.dim_last, self.dim_at()) {
                let n = perp(p(self.dim_ang().cos(), self.dim_ang().sin()));
                let s = if dot(sub(at, first), n) >= 0.0 { 1.0 } else { -1.0 };
                ed.begin();
                ed.dim_lead(first, last, self.dim_ang(), add(at, mul(n, 8.0 * s)), lead);
            }
        }
    }

    fn dim_ang(&self) -> f64 {
        match self.dim_dir {
            DimDir::Horizontal => 0.0,
            DimDir::Vertical => std::f64::consts::FRAC_PI_2,
            DimDir::Angle(d) => rad(d),
        }
    }

    /// 状态栏的提示（照 Jw_cad 的说法）
    pub fn prompt(&self) -> String {
        let lr = "(L)free  (R)Read";
        let sel_msg = |s: &Self, what: &str| -> String {
            if !s.pts.is_empty() {
                "範囲の終点を指示してください (L)文字を除く (R)文字を含む (LL)(RR)交差も選択".into()
            } else if s.range_mode == RangeMode::Add {
                "追加範囲の始点を指示してください".into()
            } else if s.range_mode == RangeMode::Remove {
                "除外範囲の始点を指示してください".into()
            } else if s.sel_done_empty() {
                format!("{what}：範囲の始点を指示 (L)、連続線 (R)")
            } else {
                format!("{what}：追加・除外する図形を指示 (L)図形 (R)文字 (Shift+R)連続線。終わったら「選択確定」")
            }
        };
        let s = match self.kind {
            Kind::None => "左のツールから命令を選んでください。両ボタン（中ボタン）ドラッグで拡大縮小、Shift+ドラッグで移動".into(),
            Kind::Auto => match (self.auto_a, self.auto_sub) {
                (None, _) => "AUTO：(L)線始点・線選択 (LL)円中心 (R)読取点から線・線上で複線・何もない所で矩形 (RR)読取点を円中心".into(),
                (Some(_), AutoSub::None) => "AUTO：(L)線上=部分消し始点／別の線=コーナー／範囲外=伸縮 (R)この線=消去／別の線=基準線まで伸縮".into(),
                (Some(_), AutoSub::Part(_)) => format!("部分消しの終点を指示してください {lr}（同じ点で切断、線上 (R) で節間消し）"),
                (Some(_), AutoSub::Stretch) => format!("伸縮点を指示してください {lr}"),
            },
            Kind::Point => format!("点を指示してください {lr}"),
            Kind::Line if self.perp_ref.is_some() => format!("垂線の終点を指示してください {lr}"),
            Kind::Line if self.pts.is_empty() => format!("始点を指示してください {lr}（線上から右ドラッグ上 = 垂線）"),
            Kind::Line => format!("終点を指示してください {lr}"),
            Kind::Rect if self.pts.is_empty() => format!("矩形の基準点を指示してください {lr}"),
            Kind::Rect => "位置を決めてください（マウスの方向に矩形ができます）".into(),
            Kind::Circle if self.three => format!("円周上の{}点目を指示してください {lr}", self.pts.len() + 1),
            Kind::Circle if self.pts.is_empty() => format!("円の中心を指示してください {lr}"),
            Kind::Circle => format!("円周上の点を指示してください {lr}"),
            Kind::Polygon => format!("多角形の中心を指示してください {lr}"),
            Kind::Offset if self.picks.is_empty() => "複線にする線・円を指示してください (L)線 (R)前回値".into(),
            Kind::Offset => "作図する方向を指示してください (L)方向 (R)前回の複線と連結".into(),
            Kind::TwoLine if self.tl_bases.is_empty() => "２線の基準線を指示してください".into(),
            Kind::TwoLine if self.tl_start.is_none() => format!("始点を指示してください {lr} (RR)線と包絡"),
            Kind::TwoLine => format!("終点を指示してください {lr} (LL)基準線変更 (RR)線と包絡"),
            Kind::Center if !self.center_ready() => "１番目・２番目の線・円 (L) または点 (R) を指示してください".into(),
            Kind::Center => format!("中心線の{}を指示してください {lr} (LL)線と連結", if self.center_start.is_none() { "始点" } else { "終点" }),
            Kind::Divide if self.div_series.is_some() => format!("連続点分割：次の点を指示。同じ点をもう一度で実行 {lr}"),
            Kind::Divide => match self.div_items[..] {
                [] => "線・円 (L) または分割始点 (R) を指示してください (RR)連続点分割".into(),
                [Div::Pt(_)] => "線・円 (L) または分割終点 (R) を指示してください".into(),
                [Div::Pt(_), Div::Pt(_)] => "(L)線上・円周上を分割 (R)２点間を分割".into(),
                _ => "線・円 (L) または分割終点 (R) を指示してください".into(),
            },
            Kind::Corner => format!("{}番目の線を指示してください（残す側をクリック） (R)切断", self.picks.len() + 1),
            Kind::Stretch if self.end_move.is_some() => format!("端点の移動先を指示してください {lr}"),
            Kind::Stretch if self.batch => match (self.batch_target, self.batch_first, self.batch_seg) {
                (None, ..) => "一括処理：基準線 (L)、任意点 (LL)、読取点 (R) を指示してください".into(),
                (Some(_), None, _) => "一括処理：始めの線を指示してください (R)同じ属性の線だけ".into(),
                (Some(_), Some(_), None) => "一括処理：終わりの線を指示してください".into(),
                _ => "追加・除外する線を指示 (L)。(R) または「処理実行」で実行".into(),
            },
            Kind::Stretch if self.stretch_base.is_some() => "基準線まで伸縮する線を指示してください（残す側をクリック） (RR)基準線変更".into(),
            Kind::Stretch if self.picks.is_empty() => "伸縮する線を指示してください（残す側をクリック） (RR)基準線指定 (R)切断".into(),
            Kind::Stretch => format!("伸縮点を指示してください {lr}"),
            Kind::Chamfer => format!("{}番目の線を指示してください (R)切断", self.picks.len() + 1),
            Kind::Tangent if self.picks.is_empty() && self.pts.is_empty() => {
                "１番目の円（左クリック）または点（右クリック）を指示してください".into()
            }
            Kind::Tangent => "２番目の円を、接点の近くで指示してください".into(),
            Kind::TanCircle if self.tan_objs.len() == 2 && num(&self.tan_r).is_some() => "作図する接円の位置を指示してください".into(),
            Kind::TanCircle => format!("{}番目の線・円 (L) または点 (R) を指示してください", self.tan_objs.len() + 1),
            Kind::Erase if self.base.is_some() => sel_msg(self, "範囲選択消去"),
            Kind::Erase if self.between => "節間消し：線・円を指示 (L)、図形消去 (R)".into(),
            Kind::Erase if self.picks.is_empty() => "(L)線・円 部分消し  (R)図形消去".into(),
            Kind::Erase => format!("部分消しの{}を指示してください {lr}（同じ点で切断）", if self.pts.is_empty() { "始点" } else { "終点" }),
            Kind::TakeAttr => {
                if self.invert_on {
                    "レイヤ反転表示中：属性を取る図形を指示 (L)、(R)で戻す".into()
                } else {
                    "属性を取る図形を指示してください (L)  (R)レイヤ反転表示".into()
                }
            }
            Kind::Range => sel_msg(self, "範囲選択"),
            Kind::ChangeAttr if !self.sel_done => sel_msg(self, "属性変更"),
            Kind::ChangeAttr => "「属性変更」を押してください".into(),
            Kind::Copy | Kind::Move if !self.sel_done => sel_msg(self, if self.kind == Kind::Copy { "複写" } else { "移動" }),
            Kind::Copy | Kind::Move if self.base.is_none() => format!("基準点を指示してください {lr}"),
            Kind::Copy => format!("複写先を指示してください（続けて複写できます） {lr}"),
            Kind::Move => format!("移動先を指示してください {lr}"),
            Kind::ClipCopy => sel_msg(self, "コピー"),
            Kind::FigReg if self.fig_wait_base => format!("図形の基準点を指示してください {lr}"),
            Kind::FigReg if !self.sel_done => sel_msg(self, "図形登録"),
            Kind::FigReg => "「《図形登録》」を押して保存先を決めてください".into(),
            Kind::Dim => self.dim_prompt(),
            Kind::Text if self.text_link => match (self.link_src, self.link_move) {
                (_, Some(_)) => format!("移動先を指示してください {lr}"),
                (Some(_), _) => "連結する文字を指示 (L)移動して連結 (R)複写して連結".into(),
                _ => "連：文字列を指示 (L)連結 (LL)移動 (R)切断".into(),
            },
            Kind::Text => match self.text_src {
                Some((_, true)) => format!("複写先を指示してください {lr}"),
                Some(_) => format!("移動先を指示 {lr}、または文字を直して「変更」"),
                None => format!("文字の位置を指示してください {lr}  既存の文字 (L)変更・移動 (R)複写"),
            },
            Kind::Solid if self.solid_from && self.ring => "円環ソリッド：円を指示してください".into(),
            Kind::Solid if self.solid_from => "円・連続線を指示してください (L)元の図形を残す (R)元の図形を消す".into(),
            Kind::Solid => format!("塗る範囲の頂点を順に指示（{} 点）。「作図」で確定 {lr}", self.pts.len()),
            Kind::Hatch if self.hatch_range && !self.sel_done => sel_msg(self, "ハッチ"),
            Kind::Hatch => "ハッチ範囲：線を順に (L)（最初の線に戻ると閉じる）、閉じた連続線・円 (R)。「実行」でハッチ".into(),
            Kind::Measure => format!("測る点を順に指示してください。Esc でやり直し {lr}"),
            Kind::Tidy => "「整理実行」で、重なった線・同じ線・長さ 0 の線をまとめます（編集できるレイヤのみ）".into(),
            Kind::Paste => format!("貼り付ける位置を指示してください（続けて貼れます） {lr}"),
            Kind::Param if self.pts.len() < 2 => format!("変形する範囲の{}を指示してください", if self.pts.is_empty() { "始点" } else { "終点" }),
            Kind::Param if self.base.is_none() => format!("基準点を指示してください {lr}"),
            Kind::Param => format!("移動先を指示してください（基準点は「基点変更」） {lr}"),
            Kind::Spline => format!("曲線の通る点を順に指示してください（{} 点）。「作図」で確定 {lr}", self.pts.len()),
            Kind::Pen if self.pen.is_empty() => "始点：クリック = 角の点、ドラッグ = 滑らかな点（ハンドルを引き出す） (L)free (R)Read".into(),
            Kind::Pen => format!(
                "次の点（{} 点）：クリック = 角、ドラッグ = 滑らか、始点をクリック = 閉じる。Enter・「作図」で確定、BackSpace で一つ戻す",
                self.pen.len()
            ),
            Kind::DimExplode => "寸法図形を指示してください（線と文字に分解します）".into(),
            Kind::Envelope if self.pts.is_empty() => format!("包絡範囲の始点を指示してください {lr}"),
            Kind::Envelope => "終点 (L)包絡 (R)範囲内消去 (Shift+L・左ドラッグ9時)中間線消去".into(),
            Kind::Symbol if self.cur_sym().is_none() => "「記号選択…」で線記号変形データ（JW_OPT4*.DAT など）を選んでください".into(),
            Kind::Symbol if self.cur_sym().map_or(false, |s| s.needs_line) && self.picks.is_empty() => {
                if self.cur_sym().map_or(false, |s| s.count == 0) {
                    "線を指示してください（指示した位置に近い線端に作図）".into()
                } else {
                    "指示線を指示してください".into()
                }
            }
            Kind::Symbol => format!("作図位置を指示してください {lr}"),
            Kind::Figure if self.fig.is_none() => "「図形選択…」で図形ファイル（.jws）を選んでください".into(),
            Kind::Figure => format!("{} を置く位置を指示してください（続けて置けます） {lr}", self.fig_name),
        };
        let s = match self.pick {
            Some(Pick::LineAngle) => "線角：角度を取る線を指示してください".into(),
            Some(Pick::LineLen) => "線長：長さを取る線を指示してください".into(),
            Some(Pick::PtAngle | Pick::PtLen) => format!("{}点目を指示してください {lr}", self.pick_pts.len() + 1),
            Some(Pick::Gap) if self.pick_line.is_none() => "間隔：基準の線を指示してください".into(),
            Some(Pick::Gap) => "間隔：もう１本の線（左クリック）か点（右クリック）を指示してください".into(),
            None => s,
        };
        let s = match &self.rd {
            Some(Rd::Mid(_)) => format!("２点間中心：もう一方の点を指示してください {lr}"),
            Some(Rd::OnLine(_)) => format!("線上点：点を指示してください {lr}"),
            Some(Rd::Offset(..)) => "オフセット：ずれを入力してください".into(),
            None => s,
        };
        match &self.note {
            Some(n) => format!("{s}　　{n}"),
            None => s,
        }
    }

    fn sel_done_empty(&self) -> bool {
        self.sel_empty
    }

    fn dim_prompt(&self) -> String {
        let lr = "(L)free  (R)Read";
        let npos = self.dim_npos();
        let pos = |k: usize| -> String {
            if npos == 2 && k == 0 {
                format!("引出線の始点位置を指示してください {lr}")
            } else {
                format!("寸法線の位置を指示してください {lr}")
            }
        };
        match self.dim_kind {
            DimKind::Radius | DimKind::Diameter => "円を指示してください (L)寸法値を内側 (R)外側".into(),
            DimKind::Angle if self.dim_two => match self.picks.len() {
                0 | 1 => format!("{}本目の線を指示してください（左回り）", self.picks.len() + 1),
                _ => format!("寸法線の位置を指示してください {lr}"),
            },
            DimKind::Angle => match self.pts.len() {
                0 => format!("角度の原点を指示してください {lr}  (LL)２線間の角度"),
                k if k < 1 + npos => pos(k - 1),
                k if k == 1 + npos => "角度の始点を指示してください（読取点）".into(),
                _ => "角度の終点を左回りに指示してください（読取点）".into(),
            },
            DimKind::ArcLen if self.picks.is_empty() => "円・円弧を指示してください".into(),
            DimKind::ArcLen => match self.pts.len() {
                k if k < npos => pos(k),
                k if k == npos => "円周寸法の始点を指示してください（読取点）".into(),
                _ => "円周寸法の終点を左回りに指示してください（読取点）".into(),
            },
            DimKind::Value if self.dim_val_move.is_some() => format!("寸法値の移動先を指示してください {lr}"),
            DimKind::Value if self.dim_val_edit.is_some() => "値を直して「変更」を押してください".into(),
            DimKind::Value if self.dim_start.is_some() => "寸法値の終点を指示してください（読取点）".into(),
            DimKind::Value => "寸法値の始点 (L・読取点)、寸法値を移動 (R)、変更 (RR)".into(),
            DimKind::Line | DimKind::Cumulative if self.dim_batch && self.pts.len() >= npos => {
                if self.dim_b_first.is_none() { "一括処理：始めの線を指示 (L)、同じ属性だけ (R)".into() } else { "一括処理：終わりの線を指示".into() }
            }
            DimKind::Cumulative => match self.pts.len() {
                k if k < npos => pos(k),
                _ if self.dim_start.is_none() => "累進寸法の起点を指示してください（読取点）".into(),
                _ => "次の点を指示してください（読取点）".into(),
            },
            DimKind::Line => match self.pts.len() {
                k if k < npos => pos(k),
                _ if self.dim_start.is_some() => "終点を指示してください（読取点）".into(),
                _ if self.dim_last.is_some() => "(L)新しい始点 (R)前の終点から続けて終点（どちらも読取点）".into(),
                _ => "始点を指示してください（読取点）".into(),
            },
        }
    }

    // ---------- 控制栏 ----------

    pub fn bar(&mut self, ui: &mut egui::Ui, ed: &mut Editor) {
        #[cfg(target_arch = "wasm32")]
        self.take_web_files();
        self.sel_empty = ed.sel.is_empty();
        self.invert_on = ed.invert;
        let field = |ui: &mut egui::Ui, label: &str, s: &mut String, w: f32| {
            ui.label(label);
            ui.add(egui::TextEdit::singleline(s).desired_width(w));
        };
        match self.kind {
            Kind::None | Kind::Auto => {}
            Kind::Point | Kind::DimExplode | Kind::Envelope => {}
            Kind::FigReg => {
                self.sel_bar(ui, ed);
                if ui.add_enabled(!ed.sel.is_empty(), egui::Button::new("基準点変更")).clicked() {
                    self.fig_wait_base = true;
                    self.pts.clear();
                }
                if ui.add_enabled(self.sel_done, egui::Button::new("《図形登録》")).clicked() {
                    self.fig_register(ed);
                }
            }
            Kind::Symbol => {
                if ui.button("記号選択…").clicked() {
                    #[cfg(target_arch = "wasm32")]
                    crate::platform::pick(&self.web_in, "線記号変形", &["dat", "DAT"]);
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                    let mut d = rfd::FileDialog::new().add_filter("線記号変形", &["dat", "DAT"]);
                    if let Some(dir) = &self.sym_dir {
                        d = d.set_directory(dir);
                    }
                    if let Some(path) = d.pick_file() {
                        match crate::symbol::read(&path) {
                            Ok(f) => {
                                self.sym = Some(f);
                                self.sym_idx = 0;
                                self.sym_dir = path.parent().map(|p| p.to_path_buf());
                                self.picks.clear();
                            }
                            Err(e) => self.note = Some(format!("読めません：{e}")),
                        }
                    }
                    }
                }
                if let Some(f) = &self.sym {
                    let cur = f.syms.get(self.sym_idx).map(|s| s.name.clone()).unwrap_or_default();
                    egui::ComboBox::from_id_salt("sym")
                        .width(200.0)
                        .selected_text(format!("{}：{cur}", f.name))
                        .show_ui(ui, |ui| {
                            for (k, s) in f.syms.iter().enumerate() {
                                let label = if s.items.is_empty() { format!("{}（作図なし）", s.name) } else { s.name.clone() };
                                if ui.selectable_label(self.sym_idx == k, label).clicked() {
                                    self.sym_idx = k;
                                    self.picks.clear();
                                }
                            }
                        });
                }
                field(ui, "倍率", &mut self.sym_mag, 40.0);
                ui.label("文字");
                ui.add(egui::TextEdit::singleline(&mut self.sym_text).desired_width(120.0).hint_text("INPUT の文字（, 区切り）"));
            }
            Kind::Line => {
                ui.checkbox(&mut self.hv, "水平・垂直");
                field(ui, "傾き", &mut self.angle, 60.0);
                field(ui, "寸法", &mut self.len, 70.0);
                ui.checkbox(&mut self.cont, "連続");
            }
            Kind::Rect => field(ui, "寸法（横,縦）", &mut self.rect_size, 110.0),
            Kind::Circle => {
                ui.checkbox(&mut self.three, "３点指示");
                field(ui, "半径", &mut self.radius, 70.0);
            }
            Kind::Polygon => {
                ui.label("角数");
                ui.add(egui::DragValue::new(&mut self.poly_n).range(3..=64));
                ui.selectable_value(&mut self.poly_mode, PolyMode::Vertex, "中心→頂点");
                ui.selectable_value(&mut self.poly_mode, PolyMode::Side, "中心→辺");
                ui.selectable_value(&mut self.poly_mode, PolyMode::Edge, "辺寸法");
                field(ui, "寸法", &mut self.poly_size, 70.0);
                field(ui, "角度", &mut self.poly_angle, 50.0);
            }
            Kind::Offset => {
                field(ui, "複線間隔", &mut self.offset, 70.0);
                if ui.add_enabled(self.picks.len() == 1, egui::Button::new("両側複線")).clicked() {
                    if let (Some(d), Some(&(i, _))) = (num(&self.offset), self.picks.first()) {
                        ed.begin();
                        ed.offset_both(i, d);
                        self.offset_prev = Some(d);
                        self.picks.clear();
                    }
                }
                if ui.add_enabled(self.last_offset.is_some(), egui::Button::new("連続")).clicked() {
                    if let (Some(d), Some((i, side))) = (num(&self.offset), self.last_offset) {
                        // 同方向再偏一次：方向点跟着平移
                        ed.begin();
                        if let Some(j) = ed.offset(i, d, side) {
                            self.last_offset = Some((j, next_side(ed, i, j, side)));
                        }
                    }
                }
            }
            Kind::TwoLine => {
                field(ui, "間隔", &mut self.twoline, 80.0);
                let v = parse_nums(&self.twoline);
                if ui.add_enabled(v.len() >= 2 && v[0] != v[1], egui::Button::new("間隔反転")).clicked() {
                    self.tl_flip = !self.tl_flip;
                }
                for (label, k) in [("1/2間隔", 0.5), ("2倍間隔", 2.0)] {
                    if ui.add_enabled(!v.is_empty(), egui::Button::new(label)).clicked() {
                        self.twoline = v.iter().map(|x| num_str(x * k)).collect::<Vec<_>>().join(",");
                    }
                }
                ui.separator();
                ui.checkbox(&mut self.tl_cap, "留線");
                ui.checkbox(&mut self.tl_cap_always, "留線常駐");
                field(ui, "留線出", &mut self.tl_cap_out, 40.0);
            }
            Kind::Center => field(ui, "中心線寸法", &mut self.center_len, 70.0),
            Kind::Divide => {
                ui.selectable_value(&mut self.div_angle, false, "等距離分割");
                ui.selectable_value(&mut self.div_angle, true, "等角度分割");
                ui.checkbox(&mut self.div_wari, "割付");
                if self.div_wari {
                    field(ui, "距離", &mut self.div_dist, 60.0);
                    ui.checkbox(&mut self.div_furi, "振分");
                    ui.checkbox(&mut self.div_under, "割付距離以下");
                } else {
                    ui.label("分割数");
                    ui.add(egui::DragValue::new(&mut self.div_n).range(2..=999));
                }
                ui.checkbox(&mut self.div_ends, "両端に点");
                // 線 A を指示した後に出るボタン
                if let [Div::Curve(i, _)] = self.div_items[..] {
                    if let Some(Curve::Seg(a, c)) = ed.curve(i) {
                        ui.separator();
                        ui.toggle_value(&mut self.div_rev, "逆分割");
                        if ui.selectable_label(self.div_ratio.is_some(), "線長割合分割").on_hover_text("次に指示する線 B との長さの比で比例分割").clicked() {
                            self.div_ratio = if self.div_ratio.is_some() { None } else { Some(dist(a, c)) };
                        }
                        let mut on = self.div_umame.is_some();
                        if ui.toggle_value(&mut on, "馬目地分割").changed() {
                            self.div_umame = on.then_some(2);
                        }
                        if let Some(m) = &mut self.div_umame {
                            ui.label("縦分割数");
                            ui.add(egui::DragValue::new(m).range(2..=2000));
                        }
                    }
                }
            }
            Kind::Corner => {}
            Kind::Stretch => {
                if ui.checkbox(&mut self.batch, "一括処理").changed() {
                    self.picks.clear();
                    self.pts.clear();
                    self.batch_target = None;
                    self.batch_first = None;
                    self.batch_seg = None;
                    ed.sel.clear();
                }
                field(ui, "突出寸法", &mut self.protrude, 60.0);
                if self.batch {
                    if ui.add_enabled(self.batch_seg.is_some(), egui::Button::new("処理実行")).clicked() {
                        self.batch_run(ed);
                    }
                } else if self.stretch_base.is_some() && ui.button("基準線解除").clicked() {
                    self.stretch_base = None;
                }
            }
            Kind::Chamfer => {
                ui.selectable_value(&mut self.round, false, "角面（辺寸法）");
                ui.selectable_value(&mut self.round, true, "丸面");
                field(ui, "寸法", &mut self.chamfer, 70.0);
            }
            Kind::Tangent => {}
            Kind::TanCircle => field(ui, "半径", &mut self.tan_r, 70.0),
            Kind::Erase => {
                ui.checkbox(&mut self.between, "節間消し");
                if ui.button("範囲選択消去").clicked() {
                    self.reset(ed);
                    // base = Some 表示「范围选择消去」进行中
                    self.base = Some(P::default());
                }
                if self.base.is_some() {
                    self.sel_bar(ui, ed);
                }
            }
            Kind::TakeAttr => {}
            Kind::ChangeAttr => {
                ui.checkbox(&mut self.ch_color, "線色");
                ui.checkbox(&mut self.ch_style, "線種");
                ui.checkbox(&mut self.ch_layer, "レイヤ");
                self.sel_bar(ui, ed);
                if ui.add_enabled(!ed.sel.is_empty(), egui::Button::new("属性変更")).clicked() {
                    ed.begin();
                    let s = ed.sel.clone();
                    ed.change_attr(&s, self.ch_color, self.ch_style, self.ch_layer);
                    self.note = Some(format!("{} 個の属性を変更しました", s.len()));
                    self.sel_done = false;
                    ed.sel.clear();
                }
            }
            Kind::Copy | Kind::Move => {
                self.sel_bar(ui, ed);
                ui.separator();
                ui.selectable_value(&mut self.axis, Axis::Free, "任意方向");
                ui.selectable_value(&mut self.axis, Axis::X, "X方向");
                ui.selectable_value(&mut self.axis, Axis::Y, "Y方向");
                if ui.add_enabled(self.sel_done, egui::Button::new("基点変更")).clicked() {
                    self.base = None;
                }
                field(ui, "数値位置（X,Y）", &mut self.move_d, 100.0);
                if ui.add_enabled(self.sel_done, egui::Button::new("実行")).clicked() {
                    let v = parse_nums(&self.move_d);
                    if v.len() == 2 {
                        let d = p(ed.paper(v[0]), ed.paper(v[1]));
                        self.apply_move(ed, d);
                    }
                }
            }
            Kind::Dim => {
                let before = (self.dim_kind, self.dim_eq);
                ui.selectable_value(&mut self.dim_kind, DimKind::Line, "寸法");
                ui.selectable_value(&mut self.dim_kind, DimKind::Radius, "半径");
                ui.selectable_value(&mut self.dim_kind, DimKind::Diameter, "直径");
                ui.selectable_value(&mut self.dim_kind, DimKind::Angle, "角度");
                ui.selectable_value(&mut self.dim_kind, DimKind::ArcLen, "円周");
                ui.selectable_value(&mut self.dim_kind, DimKind::Cumulative, "累進");
                ui.selectable_value(&mut self.dim_kind, DimKind::Value, "寸法値");
                ui.separator();
                if ui.button(format!("小数桁 {}", ed.dim_dec)).on_hover_text("2 → 3 → 0 → 1").clicked() {
                    ed.dim_dec = (ed.dim_dec + 1) % 4;
                }
                let ends = ["端部●", "端部->", "端部-<"];
                if ui.button(ends[ed.dim_end.min(2) as usize]).clicked() {
                    ed.dim_end = (ed.dim_end + 1) % 3;
                }
                // 引出角：0 → 30 → 45 → -45 → -30（右クリックで逆）
                const LEAD: [f64; 5] = [0.0, 30.0, 45.0, -45.0, -30.0];
                let k = LEAD.iter().position(|v| (*v - ed.dim_lead_ang).abs() < 1e-9).unwrap_or(0);
                let r = ui.button(if k == 0 { "引出角 0".to_string() } else { format!("{}°", LEAD[k]) });
                if r.clicked() {
                    ed.dim_lead_ang = LEAD[(k + 1) % 5];
                } else if r.secondary_clicked() {
                    ed.dim_lead_ang = LEAD[(k + 4) % 5];
                }
                if matches!(self.dim_kind, DimKind::Line | DimKind::Cumulative) {
                    if ui.selectable_label(self.dim_batch, "一括処理").on_hover_text("寸法線の位置を決めた後、始めの線・終わりの線で挟んだ線をまとめて").clicked() {
                        self.dim_batch = !self.dim_batch;
                        self.dim_b_first = None;
                        ed.sel.clear();
                    }
                    if self.dim_batch && ui.add_enabled(!ed.sel.is_empty(), egui::Button::new("実行")).clicked() {
                        self.dim_batch_run(ed);
                    }
                }
                if let (DimKind::Value, Some(i)) = (self.dim_kind, self.dim_val_edit) {
                    ui.add(egui::TextEdit::singleline(&mut self.dim_val_text).desired_width(80.0));
                    if ui.button("変更").clicked() {
                        ed.begin();
                        ed.set_text(i, &self.dim_val_text);
                        self.dim_val_edit = None;
                    }
                }
                ui.separator();
                if ui
                    .button(if self.dim_eq { "＝" } else { "－" })
                    .on_hover_text("＝：引出線の始点と寸法線の位置を指示　－：寸法線の位置だけ（引出線は測点から）")
                    .clicked()
                {
                    self.dim_eq = !self.dim_eq;
                }
                if (self.dim_kind, self.dim_eq) != before {
                    self.finish_dim(ed);
                    self.pts.clear();
                    self.picks.clear();
                    self.dim_start = None;
                    self.dim_last = None;
                    self.dim_first = None;
                    self.dim_count = 0;
                    self.dim_two = false;
                }
                let mut deg_ = match self.dim_dir {
                    DimDir::Horizontal => 0.0,
                    DimDir::Vertical => 90.0,
                    DimDir::Angle(d) => d,
                };
                if ui.selectable_label(self.dim_dir == DimDir::Horizontal, "0°").clicked() {
                    self.dim_dir = DimDir::Horizontal;
                    self.pts.clear();
                }
                if ui.selectable_label(self.dim_dir == DimDir::Vertical, "90°").clicked() {
                    self.dim_dir = DimDir::Vertical;
                    self.pts.clear();
                }
                ui.label("傾き");
                if ui.add(egui::DragValue::new(&mut deg_).speed(1.0).suffix("°")).changed() {
                    self.dim_dir = DimDir::Angle(deg_);
                }
                if ui
                    .button("線角")
                    .on_hover_text("線を指示すると、その線の角度を寸法の傾きにします")
                    .clicked()
                {
                    self.note = Some("線角：角度を取る線を指示してください".into());
                    self.pts.clear();
                    self.picks = vec![(usize::MAX, P::default())];
                }
                ui.checkbox(&mut self.dim_double, "二重");
                let mut fig = ed.dim_fig();
                if ui.checkbox(&mut fig, "寸法図形").on_hover_text("寸法線と値を寸法図形にする（図面に保存されます）").changed() {
                    ed.set_dim_fig(fig);
                }
                if ui.button("リセット").on_hover_text("寸法線の位置を解除").clicked() {
                    self.reset(ed);
                }
            }
            Kind::Text => {
                if ui.selectable_label(self.text_link, "連").on_hover_text("文字列の連結・移動・切断").clicked() {
                    self.text_link = !self.text_link;
                    self.link_src = None;
                    self.link_move = None;
                }
                if self.text_link && self.link_src.is_some() && ui.button(if self.link_front { "前付け" } else { "後付け" }).clicked() {
                    self.link_front = !self.link_front;
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(200.0).hint_text("文字"));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if let Some((i, false)) = self.text_src {
                    // Enter・「変更」= 元の位置で文字を変える
                    if ui.button("変更").clicked() || enter {
                        ed.begin();
                        ed.set_text(i, &self.text);
                        self.text_src = None;
                    }
                }
                ui.separator();
                ui.label("文字種");
                egui::ComboBox::from_id_salt("mojikind")
                    .selected_text(if ed.moji_kind == 0 { "任意".to_string() } else { format!("[{}]", ed.moji_kind) })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut ed.moji_kind, 0, "任意サイズ");
                        for k in 1..=10u32 {
                            let m = ed.doc.moji[k as usize];
                            ui.selectable_value(&mut ed.moji_kind, k, format!("[{k}] {}×{} 間隔{}", m.w, m.h, m.gap));
                        }
                    });
                if ed.moji_kind == 0 {
                    ui.add(egui::DragValue::new(&mut ed.moji_any.0).prefix("幅 ").speed(0.1));
                    ui.add(egui::DragValue::new(&mut ed.moji_any.1).prefix("高 ").speed(0.1));
                    ui.add(egui::DragValue::new(&mut ed.moji_any.2).prefix("間隔 ").speed(0.1));
                }
                ui.add(egui::DragValue::new(&mut ed.moji_deg).prefix("角度 ").suffix("°"));
            }
            Kind::Solid => {
                ui.selectable_value(&mut self.fill_any, false, "線色");
                ui.selectable_value(&mut self.fill_any, true, "任意色");
                if self.fill_any {
                    ui.color_edit_button_srgb(&mut self.fill_rgb);
                }
                let r = ui.selectable_label(self.solid_from, "円・連続線指示").on_hover_text("右クリックで円環ソリッド");
                if r.clicked() {
                    self.solid_from = !self.solid_from;
                    self.ring = false;
                    self.pts.clear();
                } else if r.secondary_clicked() {
                    self.solid_from = true;
                    self.ring = true;
                    self.pts.clear();
                }
                if self.solid_from && self.ring {
                    field(ui, "内側半径", &mut self.ring_r, 60.0);
                }
                if ui.add_enabled(self.pts.len() >= 3, egui::Button::new("作図")).clicked() {
                    self.fill_region(ed);
                }
                if ui.add_enabled(!self.pts.is_empty(), egui::Button::new("クリア")).clicked() {
                    self.pts.clear();
                }
            }
            Kind::Hatch => {
                field(ui, "角度", &mut self.hatch_angle, 50.0);
                field(ui, "ピッチ", &mut self.hatch_pitch, 60.0);
                ui.checkbox(&mut self.hatch_cross, "クロス");
                if ui.button("範囲選択").clicked() {
                    self.hatch_range = true;
                    self.sel_done = false;
                    self.pts.clear();
                }
                if self.hatch_range && !self.sel_done {
                    self.sel_bar(ui, ed);
                }
                let any = !self.picks.is_empty() || !ed.sel.is_empty();
                if ui.add_enabled(any, egui::Button::new("実行")).clicked() {
                    self.hatch_run(ed);
                }
                if ui.add_enabled(any, egui::Button::new("クリアー")).clicked() {
                    self.picks.clear();
                    ed.sel.clear();
                    self.hatch_range = false;
                    self.sel_done = false;
                }
            }
            Kind::Measure => {
                ui.selectable_value(&mut self.measure_area, false, "距離");
                ui.selectable_value(&mut self.measure_area, true, "面積");
                if ui.add_enabled(!self.pts.is_empty(), egui::Button::new("クリア")).clicked() {
                    self.pts.clear();
                    self.note = None;
                }
            }
            Kind::ClipCopy => self.sel_bar(ui, ed),
            Kind::Param => {
                ui.selectable_value(&mut self.axis, Axis::Free, "任意方向");
                ui.selectable_value(&mut self.axis, Axis::X, "X方向");
                ui.selectable_value(&mut self.axis, Axis::Y, "Y方向");
                if ui.add_enabled(self.pts.len() == 2, egui::Button::new("基点変更")).clicked() {
                    self.base = None;
                }
                field(ui, "数値位置（X,Y）", &mut self.move_d, 100.0);
                if ui.add_enabled(self.pts.len() == 2, egui::Button::new("実行")).clicked() {
                    let v = parse_nums(&self.move_d);
                    if v.len() == 2 {
                        let d = p(ed.paper(v[0]), ed.paper(v[1]));
                        self.apply_param(ed, d);
                    }
                }
                if ui.add_enabled(!self.pts.is_empty(), egui::Button::new("再選択")).clicked() {
                    self.pts.clear();
                    self.base = None;
                }
            }
            Kind::Spline => {
                ui.label("分割数");
                ui.add(egui::DragValue::new(&mut self.spline_div).range(1..=100));
                if ui.add_enabled(self.pts.len() >= 2, egui::Button::new("作図")).clicked() {
                    ed.begin();
                    let n = ed.spline(&self.pts, self.spline_div as usize).len();
                    self.note = Some(format!("{n} 本の線で曲線を作図しました"));
                    self.pts.clear();
                }
                if ui.add_enabled(!self.pts.is_empty(), egui::Button::new("クリア")).clicked() {
                    self.pts.clear();
                }
            }
            Kind::Pen => {
                ui.label("分割数").on_hover_text("曲がった区間を何本の線にするか");
                ui.add(egui::DragValue::new(&mut self.pen_div).range(2..=100));
                if ui.add_enabled(self.pen.len() >= 2, egui::Button::new("作図")).on_hover_text("Enter").clicked() {
                    self.pen_finish(ed, false);
                }
                if ui.add_enabled(self.pen.len() >= 3, egui::Button::new("閉じて作図")).clicked() {
                    self.pen_finish(ed, true);
                }
                if ui.add_enabled(!self.pen.is_empty(), egui::Button::new("一つ戻す")).on_hover_text("BackSpace").clicked() {
                    self.pen_back();
                }
                if ui.add_enabled(!self.pen.is_empty(), egui::Button::new("クリア")).clicked() {
                    self.pen.clear();
                }
            }
            Kind::Figure => {
                if ui.button("図形選択…").clicked() {
                    #[cfg(target_arch = "wasm32")]
                    crate::platform::pick(&self.web_in, "Jw_cad 図形", &["jws", "JWS"]);
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                    let mut d = rfd::FileDialog::new().add_filter("Jw_cad 図形", &["jws", "JWS"]);
                    if let Some(dir) = &self.fig_dir {
                        d = d.set_directory(dir);
                    }
                    if let Some(path) = d.pick_file() {
                        self.load_figure(&path);
                    }
                    }
                }
                ui.label(if self.fig_name.is_empty() { "（未選択）" } else { self.fig_name.as_str() });
                field(ui, "倍率", &mut self.fig_mag, 40.0);
                field(ui, "回転角", &mut self.fig_rot, 40.0);
            }
            Kind::Paste => {
                ui.label(format!("コピー中 {} 個", self.clip.len()));
                ui.checkbox(&mut self.paste_to_write, "書込レイヤに作図")
                    .on_hover_text("チェックなし：元のレイヤのまま貼り付けます");
            }
            Kind::Range => self.sel_bar(ui, ed),
            Kind::Tidy => {
                if ui.button("整理実行").clicked() {
                    ed.begin();
                    let n = ed.tidy();
                    self.note = Some(format!("{n} 個減りました"));
                }
            }
        }
    }

    /// Web 版の図登：.jws をダウンロード
    #[cfg(target_arch = "wasm32")]
    fn fig_register(&mut self, ed: &mut Editor) {
        let Some(base) = self.base.or_else(|| Self::sel_center(ed)) else { return };
        let bytes = ed.figure_bytes(&ed.sel.clone(), base);
        self.note = Some(match crate::platform::download("図形.jws", &bytes, "application/octet-stream") {
            Ok(n) => format!("{n} をダウンロードしました"),
            Err(e) => format!("保存できません：{e}"),
        });
        ed.sel.clear();
        self.sel_done = false;
        self.base = None;
    }

    /// Web 版：選んだ図形・線記号変形ファイルを読む（ブラウザの選択は非同期なので毎フレーム見る）
    #[cfg(target_arch = "wasm32")]
    fn take_web_files(&mut self) {
        let files: Vec<(std::path::PathBuf, Vec<u8>)> = self.web_in.borrow_mut().drain(..).collect();
        for (p, b) in files {
            let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            if name.to_ascii_lowercase().ends_with(".jws") {
                self.load_figure_bytes(&b, &name);
            } else {
                self.sym = Some(crate::symbol::parse(&b, &name));
                self.sym_idx = 0;
                self.picks.clear();
            }
        }
    }

    /// 図登：選んだ図形を .jws に保存（基準点は「基準点変更」で指示した点、なければ中心）
    #[cfg(not(target_arch = "wasm32"))]
    fn fig_register(&mut self, ed: &mut Editor) {
        let Some(base) = self.base.or_else(|| Self::sel_center(ed)) else { return };
        let mut d = rfd::FileDialog::new().add_filter("Jw_cad 図形", &["jws"]).set_file_name("図形.jws");
        if let Some(dir) = &self.fig_dir {
            d = d.set_directory(dir);
        }
        let Some(path) = d.save_file() else { return };
        let bytes = ed.figure_bytes(&ed.sel.clone(), base);
        match std::fs::write(&path, bytes) {
            Ok(_) => {
                self.note = Some(format!("図形を保存しました：{}", path.display()));
                self.fig_dir = path.parent().map(|p| p.to_path_buf());
            }
            Err(e) => self.note = Some(format!("保存できません：{e}")),
        }
        ed.sel.clear();
        self.sel_done = false;
        self.base = None;
    }

    /// ハッチ実行：L で選んだ線と、R・範囲選択で選んだ図形から閉じた範囲を作る
    pub fn hatch_run(&mut self, ed: &mut Editor) {
        let mut idx: Vec<usize> = self.picks.iter().map(|x| x.0).collect();
        for &i in &ed.sel {
            if !idx.contains(&i) {
                idx.push(i);
            }
        }
        let loops = ed.loops_of(&idx);
        if loops.is_empty() {
            self.note = Some("閉じた範囲がありません".into());
            return;
        }
        let Some(pitch) = num(&self.hatch_pitch) else {
            self.note = Some("ピッチを入れてください".into());
            return;
        };
        let ang = parse_angle(&self.hatch_angle).unwrap_or(45.0);
        ed.begin();
        let n = ed.hatch_loops(&loops, ang, pitch.abs(), self.hatch_cross).len();
        self.note = Some(format!("{n} 本作図しました"));
        self.picks.clear();
        ed.sel.clear();
        self.hatch_range = false;
        self.sel_done = false;
    }

    /// 塗り：点好的多角形作图
    pub fn fill_region(&mut self, ed: &mut Editor) {
        if self.pts.len() < 3 {
            return;
        }
        ed.begin();
        let col = self.fill_color();
        let n = ed.solid_poly(&self.pts, col).len();
        self.note = Some(format!("{n} 個作図しました"));
        self.pts.clear();
    }

    /// 測定の結果（実寸）
    fn measure_note(&self, ed: &Editor) -> String {
        let real = |v: f64| crate::edit::fmt_len(ed.real(v));
        if self.measure_area {
            let a = signed_area(&self.pts).abs() * ed.scale() * ed.scale() / 1e6;
            format!("面積 {:.4} ㎡（{} 点）", a, self.pts.len())
        } else {
            let n = self.pts.len();
            let last = if n >= 2 { dist(self.pts[n - 2], self.pts[n - 1]) } else { 0.0 };
            let ang = if n >= 2 { deg(angle_of(sub(self.pts[n - 1], self.pts[n - 2]))) } else { 0.0 };
            format!("距離 {}　累計 {}　角度 {:.4}°", real(last), real(path_len(&self.pts)), ang)
        }
    }

    /// 选择的按钮（复写・移动・属变・范围消去共用）。照 Jw_cad：选好以后「追加範囲」「除外範囲」再框
    fn sel_bar(&mut self, ui: &mut egui::Ui, ed: &mut Editor) {
        ui.label(format!("選択 {} 個", ed.sel.len()));
        if !self.sel_done {
            if ed.sel.is_empty() {
                if ui.button("全選択").clicked() {
                    ed.sel = (0..ed.doc.ents.len()).filter(|&i| ed.editable(&ed.doc.ents[i].a)).collect();
                    self.range_mode = RangeMode::Normal;
                }
            } else {
                if ui.selectable_label(self.range_mode == RangeMode::Add, "追加範囲").clicked() {
                    self.range_mode = RangeMode::Add;
                    self.pts.clear();
                }
                if ui.selectable_label(self.range_mode == RangeMode::Remove, "除外範囲").clicked() {
                    self.range_mode = RangeMode::Remove;
                    self.pts.clear();
                }
            }
        }
        if self.kind != Kind::Range && ui.add_enabled(!self.sel_done && !ed.sel.is_empty(), egui::Button::new("選択確定")).clicked() {
            self.confirm(ed);
        }
        if ui.add_enabled(!ed.sel.is_empty(), egui::Button::new("選択解除")).clicked() {
            ed.sel.clear();
            self.sel_done = false;
            self.pts.clear();
            self.range_mode = RangeMode::Normal;
        }
    }

    /// 选中图形的中心（复写・移动・図登 的默认基准点）
    fn sel_center(ed: &Editor) -> Option<P> {
        let mut bb = crate::geom::BBox::EMPTY;
        for &i in &ed.sel {
            for q in key_points(&ed.doc.ents[i]) {
                bb.add(q);
            }
        }
        (!bb.is_empty()).then(|| p((bb.x0 + bb.x1) / 2.0, (bb.y0 + bb.y1) / 2.0))
    }

    /// 「選択確定」：复写・移动进入放置阶段（基准点默认在中心）；范围消去就直接消
    pub fn confirm(&mut self, ed: &mut Editor) {
        self.range_mode = RangeMode::Normal;
        if self.kind == Kind::ClipCopy {
            self.copy_selection(ed);
            return;
        }
        if self.kind == Kind::Hatch {
            self.sel_done = true;
            self.pts.clear();
            return;
        }
        if self.kind == Kind::FigReg {
            self.sel_done = true;
            self.pts.clear();
            if self.base.is_none() {
                self.base = Self::sel_center(ed);
            }
            return;
        }
        if self.kind == Kind::Erase {
            ed.begin();
            let s = ed.sel.clone();
            ed.erase(s);
            self.base = None;
            self.pts.clear();
            return;
        }
        self.sel_done = true;
        self.pts.clear();
        if matches!(self.kind, Kind::Copy | Kind::Move) {
            self.base = Self::sel_center(ed);
        }
    }

    fn apply_move(&mut self, ed: &mut Editor, d: P) {
        let d = match self.axis {
            Axis::Free => d,
            Axis::X => p(d.x, 0.0),
            Axis::Y => p(0.0, d.y),
        };
        ed.begin();
        let s = ed.sel.clone();
        if self.kind == Kind::Copy {
            ed.copy(&s, d);
            self.note = Some(format!("{} 個を複写しました", s.len()));
        } else {
            ed.move_(&s, d);
            self.note = Some(format!("{} 個を移動しました", s.len()));
            ed.sel.clear();
            self.sel_done = false;
            self.base = None;
        }
    }

    /// 範囲選択（Jw_cad）：
    /// 始点 L = 框的起点，R 点线 = 连续线；终点 L = 文字以外，R = 含文字，LL/RR = 碰到框的也选；
    /// 选好后 L 点图形 = 文字以外的追加・除外，R = 文字的追加・除外，Shift+R = 连续线的追加・除外
    fn select_click(&mut self, ed: &mut Editor, q: P, b: Btn, tol: f64) {
        let is_text = |e: &Ent| matches!(e.shape, Shape::Text(_));
        let temp = |e: &Ent| matches!(&e.shape, Shape::Point(x) if x.temp);
        if let Some(a) = self.pts.first().copied() {
            let (x0, x1, y0, y1) = (a.x.min(q.x), a.x.max(q.x), a.y.min(q.y), a.y.max(q.y));
            let mut got: Vec<usize> = ed.select_window(a, q);
            if b.dbl {
                // 交差：框线穿过的线・圆・ソリッド・寸法也选
                for (i, e) in ed.doc.ents.iter().enumerate() {
                    if got.contains(&i) || !ed.editable(&e.a) || is_text(e) {
                        continue;
                    }
                    let hit = match Curve::of(e) {
                        Some(c) => {
                            let r = [p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)];
                            (0..4).any(|k| !c.meet(&Curve::Seg(r[k], r[(k + 1) % 4]), false).is_empty())
                        }
                        None => key_points(e).iter().any(|v| v.x >= x0 && v.x <= x1 && v.y >= y0 && v.y <= y1),
                    };
                    if hit {
                        got.push(i);
                    }
                }
            }
            got.retain(|&i| !temp(&ed.doc.ents[i]) && (b.right || !is_text(&ed.doc.ents[i])));
            if self.range_mode == RangeMode::Remove {
                ed.sel.retain(|i| !got.contains(i));
            } else {
                for i in got {
                    if !ed.sel.contains(&i) {
                        ed.sel.push(i);
                    }
                }
            }
            self.pts.clear();
            self.range_mode = RangeMode::Normal;
            return;
        }
        if ed.sel.is_empty() || self.range_mode != RangeMode::Normal {
            // 始点
            if b.right && self.range_mode == RangeMode::Normal {
                if let Some(i) = ed.pick(q, tol, is_curve) {
                    for k in ed.chain_from(i) {
                        if !ed.sel.contains(&k) {
                            ed.sel.push(k);
                        }
                    }
                    return;
                }
            }
            self.pts.push(q);
            return;
        }
        // 追加・除外
        let toggle = |ed: &mut Editor, i: usize| {
            if let Some(k) = ed.sel.iter().position(|&s| s == i) {
                ed.sel.remove(k);
            } else {
                ed.sel.push(i);
            }
        };
        if b.right && b.shift {
            if let Some(i) = ed.pick(q, tol, is_curve) {
                let ch = ed.chain_from(i);
                let on = ed.sel.contains(&i);
                ed.sel.retain(|k| !ch.contains(k));
                if !on {
                    ed.sel.extend(ch);
                }
            }
        } else if b.right {
            match ed.pick(q, tol, is_text) {
                Some(i) => toggle(ed, i),
                None => self.note = Some("文字がありません".into()),
            }
        } else {
            match ed.pick(q, tol, |e| !is_text(e) && !temp(e)) {
                Some(i) => toggle(ed, i),
                None => self.note = Some("図形がありません（範囲を足すときは「追加範囲」）".into()),
            }
        }
    }

    // ---------- 点击 ----------

    /// 这个位置点下去，有没有双击的意思（有的话第一下先不动，等鼠标动了才算单击 —— Jw_cad 的规矩）
    pub fn wants_double(&self, ed: &Editor, q: P, right: bool, tol: f64) -> bool {
        if self.pick.is_some() || self.rd.is_some() {
            return false;
        }
        let on_line = || ed.pick(q, tol, is_line).is_some();
        let on_curve = || ed.pick(q, tol, is_curve).is_some();
        match self.kind {
            Kind::TwoLine => !self.tl_bases.is_empty() && on_line() && (right || self.tl_start.is_some()),
            Kind::Stretch if self.batch => !right && self.batch_target.is_none(),
            Kind::Stretch => right && on_curve(),
            Kind::Center => !right && self.center_ready() && on_line(),
            Kind::Text => self.text_link && !right && self.link_src.is_none() && ed.pick(q, tol, |e| matches!(e.shape, Shape::Text(_))).is_some(),
            Kind::Dim if self.dim_kind == DimKind::Value => right && ed.pick(q, tol, |e| matches!(e.shape, Shape::Text(_))).is_some(),
            Kind::Dim => !right && self.dim_kind == DimKind::Angle && self.pts.is_empty() && self.picks.is_empty() && on_line(),
            Kind::Divide => right && self.div_items.is_empty() && self.div_series.is_none(),
            Kind::Auto => {
                self.auto_a.is_none()
                    && if right { ed.read_point(q, tol).is_some() } else { !on_curve() && ed.read_point(q, tol).is_none() }
            }
            k if self.selecting(k) => !self.pts.is_empty(),
            _ => false,
        }
    }

    /// 现在是不是在用范围选择
    fn selecting(&self, k: Kind) -> bool {
        match k {
            Kind::Range => true,
            Kind::Copy | Kind::Move | Kind::ChangeAttr | Kind::ClipCopy | Kind::FigReg => !self.sel_done,
            Kind::Erase => self.base.is_some(),
            Kind::Hatch => self.hatch_range && !self.sel_done,
            _ => false,
        }
    }

    /// 旧的接口：right = 右键单击
    pub fn click(&mut self, ed: &mut Editor, q: P, right: bool, read: Option<P>, tol: f64) {
        self.click_btn(ed, q, Btn { right, ..Default::default() }, read, tol);
    }

    /// q = 光标位置，read = 右键时的读取点（None = 没读到）
    pub fn click_btn(&mut self, ed: &mut Editor, q: P, b: Btn, read: Option<P>, tol: f64) {
        self.note = None;
        // 右键拖动的读取（2点间中心・线上点）还差一下
        if let Some(rd) = self.rd.take() {
            let x = if b.right { read.or_else(|| ed.read_point(q, tol)) } else { Some(q) };
            let Some(x) = x else {
                self.rd = Some(rd);
                self.note = Some("点がありません".into());
                return;
            };
            let y = match rd {
                Rd::Mid(a) => mid(a, x),
                Rd::OnLine(i) => match ed.curve(i) {
                    Some(Curve::Seg(a, c)) => project(a, c, x),
                    Some(c) => c.nearest(x),
                    None => return,
                },
                Rd::Offset(..) => {
                    self.rd = Some(rd);
                    return;
                }
            };
            self.click_btn(ed, y, Btn { right: true, ..Default::default() }, Some(y), tol);
            return;
        }
        let rev = ed.rev;
        let before = self.sig(ed);
        ed.pick_only = if self.selecting(self.kind) { (false, false) } else { (b.shift, b.ctrl) };
        self.click_inner(ed, q, b, read, tol);
        ed.pick_only = (false, false);
        // 右クリックで何も起きず、読取点もなかった
        if b.right && read.is_none() && self.note.is_none() && self.sig(ed) == before {
            self.note = Some("点がありません".into());
        }
        // AUTO から移った命令は、一回作図したら AUTO に戻る
        if self.auto_ret && self.kind != Kind::Auto && ed.rev != rev && self.pts.is_empty() && self.picks.is_empty() {
            self.auto_ret = false;
            self.kind = Kind::Auto;
        }
    }

    fn click_inner(&mut self, ed: &mut Editor, q: P, b: Btn, read: Option<P>, tol: f64) {
        let right = b.right;
        // 指点的位置：左键 = 光标，右键 = 读取点
        let pt = if right { read } else { Some(q) };
        // 两个键都读取点（寸法的测点等）
        let read_any = |ed: &Editor| if right { read } else { ed.read_point(q, tol) };
        if let Some(m) = self.pick {
            self.pick_click(ed, m, q, pt, tol);
            return;
        }
        if let Some((i, near)) = self.end_move {
            // 伸縮「端点移動」
            if let Some(x) = pt {
                ed.begin();
                ed.move_end(i, near, x);
                self.end_move = None;
            }
            return;
        }
        if self.kind == Kind::FigReg && self.fig_wait_base {
            if let Some(x) = pt {
                self.base = Some(x);
                self.fig_wait_base = false;
            }
            return;
        }
        if self.selecting(self.kind) {
            self.select_click(ed, q, b, tol);
            return;
        }
        let need_pt = |s: &mut Self| -> Option<P> {
            if pt.is_none() && !right {
                s.note = None;
            }
            pt
        };
        match self.kind {
            Kind::None => {}
            Kind::Auto => self.auto_click(ed, q, b, read, tol),
            Kind::Point => {
                if let Some(x) = need_pt(self) {
                    ed.begin();
                    ed.point(x);
                }
            }
            Kind::Line => {
                let Some(x) = need_pt(self) else { return };
                if let Some(&a) = self.pts.first() {
                    let bp = match self.perp_ref.take() {
                        // 垂線：起点在线上，方向是线的法线
                        Some(c) => {
                            let n = match c {
                                Curve::Seg(s, t) => perp(norm(sub(t, s))),
                                Curve::Circ { c, .. } => norm(sub(a, c)),
                            };
                            let mut d = dot(sub(x, a), n);
                            if let Some(l) = num(&self.len) {
                                d = ed.paper(l) * d.signum();
                            }
                            add(a, mul(n, d))
                        }
                        None => self.line_end(ed, a, x, right),
                    };
                    if dist(a, bp) > 1e-9 {
                        ed.begin();
                        ed.line(a, bp);
                    }
                    self.pts.clear();
                    if self.cont {
                        self.pts.push(bp);
                    }
                } else {
                    self.pts.push(x);
                }
            }
            Kind::Rect => {
                let Some(x) = need_pt(self) else { return };
                let size = parse_nums(&self.rect_size);
                if let Some(&a) = self.pts.first() {
                    ed.begin();
                    if size.len() >= 2 {
                        ed.rect_size(a, size[0], size[1], anchor(a, q));
                    } else {
                        ed.rect(a, x);
                    }
                    self.pts.clear();
                } else {
                    self.pts.push(x);
                }
            }
            Kind::Circle => {
                let Some(x) = need_pt(self) else { return };
                self.pts.push(x);
                if self.three {
                    if self.pts.len() == 3 {
                        ed.begin();
                        if ed.circle3(self.pts[0], self.pts[1], self.pts[2]).is_none() {
                            self.note = Some("３点が一直線上にあります".into());
                        }
                        self.pts.clear();
                    }
                } else if let Some(r) = num(&self.radius) {
                    ed.begin();
                    ed.circle(x, r);
                    self.pts.clear();
                } else if self.pts.len() == 2 {
                    ed.begin();
                    let r = dist(self.pts[0], self.pts[1]);
                    ed.push(arc_shape(self.pts[0], r, 0.0, std::f64::consts::TAU));
                    self.pts.clear();
                }
            }
            Kind::Polygon => {
                let Some(x) = need_pt(self) else { return };
                let Some(s) = num(&self.poly_size) else {
                    self.note = Some("寸法を入力してください".into());
                    return;
                };
                ed.begin();
                let a = parse_angle(&self.poly_angle).unwrap_or(90.0);
                ed.polygon(x, self.poly_n as usize, s, self.poly_mode, a);
            }
            Kind::Offset => self.offset_click(ed, q, b, tol),
            Kind::TwoLine => self.two_line_click(ed, q, b, pt, tol),
            Kind::Center => self.center_click(ed, q, b, pt, tol),
            Kind::Divide => self.divide_click(ed, q, b, pt, tol),
            Kind::Corner => {
                if right {
                    // 右クリック = 切断
                    if let Some(i) = ed.pick(q, tol, is_curve) {
                        ed.begin();
                        if !ed.cut_at(i, q) {
                            ed.undo();
                        }
                    }
                    return;
                }
                if let Some(i) = ed.pick(q, tol, is_line) {
                    self.picks.push((i, q));
                    if self.picks.len() == 2 {
                        let ((a, pa), (c, pc)) = (self.picks[0], self.picks[1]);
                        ed.begin();
                        if !ed.corner(a, pa, c, pc) {
                            ed.undo();
                            self.note = Some("平行線です".into());
                        }
                        self.picks.clear();
                    }
                }
            }
            Kind::Stretch => self.stretch_click(ed, q, b, pt, tol),
            Kind::Chamfer => {
                if right {
                    if let Some(i) = ed.pick(q, tol, is_curve) {
                        ed.begin();
                        if !ed.cut_at(i, q) {
                            ed.undo();
                        }
                    }
                    return;
                }
                if let Some(i) = ed.pick(q, tol, is_line) {
                    self.picks.push((i, q));
                    if self.picks.len() == 2 {
                        let ((a, pa), (c, pc)) = (self.picks[0], self.picks[1]);
                        let Some(r) = num(&self.chamfer) else {
                            self.note = Some("寸法を入力してください".into());
                            self.picks.clear();
                            return;
                        };
                        ed.begin();
                        if ed.chamfer(a, pa, c, pc, r, self.round).is_none() {
                            ed.undo();
                            self.note = Some("面取りできません".into());
                        }
                        self.picks.clear();
                    }
                }
            }
            Kind::Tangent => {
                if self.picks.is_empty() && self.pts.is_empty() {
                    if right {
                        if let Some(x) = pt {
                            self.pts.push(x);
                        }
                    } else if let Some(i) = ed.pick(q, tol, is_circ) {
                        self.picks.push((i, q));
                    }
                } else if let Some(j) = ed.pick(q, tol, is_circ) {
                    ed.begin();
                    let r = if let Some(&s) = self.pts.first() {
                        ed.tangent_pc(s, j, q)
                    } else {
                        let (i, h) = self.picks[0];
                        ed.tangent_cc(i, h, j, q)
                    };
                    if r.is_none() {
                        ed.undo();
                        self.note = Some("接線がありません".into());
                    }
                    self.picks.clear();
                    self.pts.clear();
                }
            }
            Kind::TanCircle => self.tan_circle_click(ed, q, b, pt, tol),
            Kind::Erase => {
                if self.between {
                    if right {
                        if let Some(i) = ed.pick(q, tol, |_| true) {
                            ed.begin();
                            ed.erase(vec![i]);
                        }
                    } else if let Some(i) = ed.pick(q, tol, is_curve) {
                        ed.begin();
                        if !ed.erase_between(i, q) {
                            ed.undo();
                        }
                    }
                } else if self.picks.is_empty() {
                    // R = 图形消去，L = 线・圆的部分消し
                    if right {
                        match ed.pick(q, tol, |_| true) {
                            Some(i) => {
                                ed.begin();
                                ed.erase(vec![i]);
                            }
                            None => self.note = Some("図形がありません".into()),
                        }
                    } else if let Some(i) = ed.pick(q, tol, is_curve) {
                        self.picks.push((i, q));
                    }
                } else {
                    let Some(x) = pt else { return };
                    self.pts.push(x);
                    if self.pts.len() == 2 {
                        let i = self.picks[0].0;
                        ed.begin();
                        if dist(self.pts[0], self.pts[1]) < 1e-9 {
                            // 同じ点 = 切断
                            if !ed.cut_at(i, self.pts[0]) {
                                ed.undo();
                            }
                        } else {
                            ed.erase_part(i, self.pts[0], self.pts[1]);
                        }
                        self.picks.clear();
                        self.pts.clear();
                    }
                }
            }
            Kind::TakeAttr => {
                if right {
                    // 右クリック = レイヤ反転表示
                    ed.invert = !ed.invert;
                    self.note = Some(if ed.invert { "レイヤ反転表示中（もう一度右クリックで戻ります）".into() } else { "反転表示を解除しました".into() });
                } else if let Some(i) = ed.pick(q, tol, |_| true) {
                    ed.take_attr(i);
                    ed.invert = false;
                    self.note = Some(format!(
                        "属性を取得：線色{} 線種{} レイヤ[{:X}-{:X}]",
                        ed.pen.color,
                        ed.pen.style,
                        ed.glay(),
                        ed.lay()
                    ));
                }
            }
            Kind::ChangeAttr | Kind::Range | Kind::ClipCopy => {}
            Kind::Copy | Kind::Move => {
                let Some(x) = pt else { return };
                match self.base {
                    None => self.base = Some(x),
                    Some(bp) => self.apply_move(ed, sub(x, bp)),
                }
            }
            Kind::Dim => self.dim_click(ed, q, b, pt, read_any(ed), tol),
            Kind::Text => self.text_click(ed, q, b, pt, tol),
            Kind::Solid => {
                if self.solid_from && self.ring {
                    // 円環ソリッド
                    let Some(i) = ed.pick(q, tol, is_circ) else { return };
                    let Some(rin) = num(&self.ring_r) else {
                        self.note = Some("内側円の半径を入力してください".into());
                        return;
                    };
                    ed.begin();
                    let n = ed.solid_ring(i, rin, self.fill_color()).len();
                    if n == 0 {
                        ed.undo();
                        self.note = Some("内側の半径は円の半径より小さくしてください".into());
                    }
                    return;
                }
                if self.solid_from {
                    // 円・連続線指示：L = 元の図形を残す、R = 消す
                    let Some(i) = ed.pick(q, tol, is_curve) else { return };
                    let Some(ch) = ed.closed_chain(i) else {
                        self.note = Some("閉じた連続線・円ではありません".into());
                        return;
                    };
                    let loops = ed.loops_of(&ch);
                    ed.begin();
                    let n = ed.solid_loops(&loops, self.fill_color()).len();
                    if right {
                        ed.erase(ch);
                    }
                    self.note = Some(format!("{n} 個作図しました"));
                    return;
                }
                if let Some(x) = pt {
                    self.pts.push(x);
                }
            }
            Kind::Hatch => {
                // L = 線を１本ずつ（最初の線に戻ると閉じる）、R = 閉じた連続線・円
                if right {
                    let Some(i) = ed.pick(q, tol, is_curve) else { return };
                    match ed.closed_chain(i) {
                        Some(ch) => {
                            for k in ch {
                                if !ed.sel.contains(&k) {
                                    ed.sel.push(k);
                                }
                            }
                        }
                        None => self.note = Some("閉じた連続線・円ではありません".into()),
                    }
                    return;
                }
                let Some(i) = ed.pick(q, tol, is_curve) else { return };
                if self.picks.first().map(|x| x.0) == Some(i) && self.picks.len() >= 2 {
                    self.note = Some("閉じました。「実行」でハッチします".into());
                    return;
                }
                if !self.picks.iter().any(|x| x.0 == i) {
                    self.picks.push((i, q));
                }
            }
            Kind::Measure => {
                let Some(x) = pt else { return };
                self.pts.push(x);
                self.note = Some(self.measure_note(ed));
            }
            Kind::Tidy => {}
            Kind::Paste => {
                if let Some(x) = pt {
                    self.paste_at(ed, x);
                }
            }
            Kind::Param => {
                if self.pts.len() < 2 {
                    // 范围框用任意点；框好了基准点默认在框的中心
                    self.pts.push(q);
                    if self.pts.len() == 2 {
                        self.base = Some(mid(self.pts[0], self.pts[1]));
                    }
                    return;
                }
                let Some(x) = pt else { return };
                match self.base {
                    None => self.base = Some(x),
                    Some(bp) => self.apply_param(ed, sub(x, bp)),
                }
            }
            Kind::Spline => {
                if let Some(x) = pt {
                    self.pts.push(x);
                }
            }
            Kind::Pen => self.pen_click(ed, q, pt, tol),
            Kind::Symbol => {
                let Some(s) = self.cur_sym().cloned() else {
                    self.note = Some("先に「記号選択…」で記号を選んでください".into());
                    return;
                };
                if s.needs_line && self.picks.is_empty() {
                    match ed.pick(q, tol, is_line) {
                        Some(i) => self.picks.push((i, q)),
                        None => {
                            self.note = Some("線を指示してください".into());
                            return;
                        }
                    }
                    if s.count != 0 {
                        return;
                    }
                }
                let Some((o, ang, attr)) = self.sym_place(ed, q, pt) else { return };
                let mag = num(&self.sym_mag).unwrap_or(1.0);
                ed.begin();
                let n = ed.place_symbol(&s, o, ang, mag, attr.as_ref(), &self.sym_inputs()).len();
                self.note = Some(if n == 0 { "この記号は作図する部材がありません（線の切断・伸縮だけの記号は未対応）".into() } else { format!("{} を作図しました", s.name) });
                self.picks.clear();
            }
            Kind::Envelope => {
                let Some(x) = (if self.pts.is_empty() { pt } else { Some(q) }) else { return };
                if self.pts.is_empty() {
                    self.pts.push(x);
                    return;
                }
                let a = self.pts[0];
                ed.begin();
                let n = if b.shift && !right {
                    format!("中間線消去しました（{} 本）", ed.erase_through_rect(a, x))
                } else if right {
                    format!("範囲内消去しました（{} 本）", ed.erase_in_rect(a, x))
                } else {
                    format!("包絡しました（{} 本）", ed.envelope(a, x))
                };
                self.note = Some(n);
                self.pts.clear();
            }
            Kind::FigReg => {}
            Kind::DimExplode => {
                if let Some(i) = ed.pick(q, tol, |e| matches!(e.shape, Shape::Dim(_))) {
                    ed.begin();
                    ed.explode_dims(&[i]);
                    self.note = Some("寸法を分解しました".into());
                } else {
                    self.note = Some("寸法図形がありません（線と文字で描いた寸法は分解済みです）".into());
                }
            }
            Kind::Figure => {
                let Some(x) = pt else { return };
                let Some(fig) = self.fig.take() else {
                    self.note = Some("先に「図形選択…」で図形を選んでください".into());
                    return;
                };
                let (mag, ang) = self.fig_params();
                ed.begin();
                let (n, skip) = ed.place_figure(&fig, x, mag, ang);
                self.note = Some(if skip > 0 {
                    format!("{n} 個置きました（ブロック {skip} 個はまだ置けません）")
                } else {
                    format!("{n} 個置きました")
                });
                self.fig = Some(fig);
            }
        }
    }

    fn fill_color(&self) -> FillColor {
        if self.fill_any {
            let [r, g, b] = self.fill_rgb;
            FillColor::Rgb(r as u32 | (g as u32) << 8 | (b as u32) << 16)
        } else {
            FillColor::Pen
        }
    }

    /// 複線：L で線を選んで方向 L。間隔が空欄ならマウスの位置が間隔。
    /// R で線を選ぶ = 前回の間隔、方向を R = 直前の複線と連結（コーナー処理）
    fn offset_click(&mut self, ed: &mut Editor, q: P, b: Btn, tol: f64) {
        if self.picks.is_empty() {
            if let Some(i) = ed.pick(q, tol, is_curve) {
                self.picks.push((i, q));
                if b.right {
                    if let Some(d) = self.offset_prev {
                        self.offset = num_str(d);
                    }
                }
            }
            return;
        }
        let (i, _) = self.picks[0];
        let d = match num(&self.offset) {
            Some(d) => d,
            None => match self.offset_prev.filter(|_| b.right) {
                Some(d) => d,
                None => match ed.curve(i) {
                    Some(c) => ed.real(c_dist_ext(&c, q)),
                    None => return,
                },
            },
        };
        if d.abs() < 1e-9 {
            return;
        }
        ed.begin();
        if let Some(j) = ed.offset(i, d, q) {
            self.last_offset = Some((j, next_side(ed, i, j, q)));
            if b.right {
                if let Some((pi, pj)) = self.offset_chain {
                    connect(ed, pi, pj, i, j);
                }
            }
            self.offset_chain = Some((i, j));
        }
        self.offset_prev = Some(d);
        self.picks.clear();
    }

    /// 2線：基準線 → 始点 → 終点。LL で基準線を変える（折れ曲がり）、RR で線と包絡
    fn two_line_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        let v = parse_nums(&self.twoline);
        let (d1, d2) = match v.len() {
            0 => {
                self.note = Some("間隔を入力してください".into());
                return;
            }
            1 => (v[0], v[0]),
            _ => (v[0], v[1]),
        };
        let (d1, d2) = if self.tl_flip { (d2, d1) } else { (d1, d2) };
        let line_at = |ed: &Editor| ed.pick(q, tol, is_line).and_then(|i| match ed.curve(i) {
            Some(Curve::Seg(a, c)) => Some((i, a, c)),
            _ => None,
        });
        if self.tl_bases.is_empty() {
            match line_at(ed) {
                Some((_, a, c)) => self.tl_bases.push((a, c)),
                None => self.note = Some("基準線を指示してください".into()),
            }
            return;
        }
        // LL = 基準線変更（始点を指示した後）
        if b.dbl && !b.right && self.tl_start.is_some() {
            if let Some((_, a, c)) = line_at(ed) {
                self.tl_bases.push((a, c));
            }
            return;
        }
        // RR = その線と包絡
        let end = if b.dbl && b.right {
            let Some((i, a, c)) = line_at(ed) else { return };
            (End::Line(a, c), Some((i, q)))
        } else {
            let Some(x) = pt else { return };
            (End::Pt(x), None)
        };
        match self.tl_start {
            None => {
                self.tl_start = Some(end.0);
                self.tl_env[0] = end.1;
            }
            Some(s) => {
                self.tl_env[1] = end.1;
                match ed.two_line_path(&self.tl_bases, s, end.0, d1, d2) {
                    Some((l, r, hits)) => {
                        let cap = (self.tl_cap || self.tl_cap_always).then(|| num(&self.tl_cap_out).unwrap_or(0.0).max(0.0));
                        ed.begin();
                        ed.two_lines_draw(&l, &r, hits, self.tl_env, [cap, cap]);
                        self.tl_cap = false;
                    }
                    None => self.note = Some("作図できません（平行な基準線）".into()),
                }
                // Jw_cad：基準線は最後のものが残って、次の始点へ
                let last = *self.tl_bases.last().unwrap();
                self.tl_bases = vec![last];
                self.tl_start = None;
                self.tl_env = [None, None];
            }
        }
    }

    /// 中心線の２つの対象が決まったか
    /// 点击前后比较用：有没有起变化
    fn sig(&self, ed: &Editor) -> (u64, usize, usize, usize, bool, bool, usize, usize, bool) {
        (
            ed.rev,
            self.pts.len(),
            self.picks.len(),
            ed.sel.len(),
            ed.invert,
            self.busy(ed),
            self.tan_objs.len() + self.div_items.len() + self.tl_bases.len(),
            self.kind as usize,
            self.rd.is_some() || self.text_src.is_some() || self.stretch_base.is_some(),
        )
    }

    fn center_ready(&self) -> bool {
        self.picks.len() + self.pts.len() >= 2
    }

    /// 中心線：１・２番目は L = 線（円なら円周上の点）、R = 読取点。その後 始点・終点（LL = 線と連結）
    fn center_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        if !self.center_ready() {
            if b.right {
                if let Some(x) = pt {
                    self.pts.push(x);
                }
            } else if let Some(i) = ed.pick(q, tol, is_curve) {
                match ed.curve(i) {
                    Some(c @ Curve::Circ { .. }) => self.pts.push(c.nearest(q)),
                    _ => self.picks.push((i, q)),
                }
            }
            return;
        }
        let Some(l) = self.center_axis(ed) else {
            self.reset(ed);
            return;
        };
        // 端：LL で線を指示 = その線との交点で、線とコーナー処理
        let link = if b.dbl && !b.right { ed.pick(q, tol, is_line) } else { None };
        let x = match link.and_then(|j| ed.curve(j)) {
            Some(Curve::Seg(a, c)) => match line_line(l.0, l.1, a, c) {
                Some(x) => x,
                None => return,
            },
            _ => {
                let Some(x) = pt else { return };
                project(l.0, l.1, x)
            }
        };
        match self.center_start {
            None => {
                self.center_start = Some(x);
                self.center_link[0] = link.map(|j| (j, q));
            }
            Some(s) => {
                // 中心線寸法：始点から指示した方向へその長さ
                let x = match num(&self.center_len) {
                    Some(len) if link.is_none() => {
                        let u = norm(sub(l.1, l.0));
                        let k = if dot(sub(x, s), u) >= 0.0 { 1.0 } else { -1.0 };
                        add(s, mul(u, ed.paper(len) * k))
                    }
                    _ => x,
                };
                ed.begin();
                ed.center_line(l, s, x);
                let mut links: Vec<(usize, P, P)> = Vec::new();
                if let Some((j, h)) = self.center_link[0] {
                    links.push((j, h, s));
                }
                if let Some(j) = link {
                    links.push((j, q, x));
                }
                links.sort_by(|a, b| b.0.cmp(&a.0));
                for (j, h, x) in links {
                    ed.stretch_pt(j, h, x, 0.0);
                }
                self.picks.clear();
                self.pts.clear();
                self.center_start = None;
                self.center_link = [None, None];
            }
        }
    }

    /// 分割の位置（割合、両端を除く）。total = 全長（図寸）、ratio = 線長割合（最後の間隔 / 最初の間隔）
    fn div_ts(&self, ed: &Editor, total: f64) -> Vec<f64> {
        if total <= 1e-12 {
            return vec![];
        }
        if self.div_wari {
            // 割付：指定距離で。振分 = 余りを両側に、割付距離以下 = その距離以下の等分
            let Some(d) = num(&self.div_dist).map(|v| ed.paper(v.abs())).filter(|d| *d > 1e-12) else { return vec![] };
            if self.div_under {
                let n = (total / d - 1e-9).ceil().max(1.0) as usize;
                return (1..n).map(|k| k as f64 / n as f64).collect();
            }
            let k = (total / d + 1e-9).floor() as usize;
            let off = if self.div_furi { (total - k as f64 * d) / 2.0 } else { 0.0 };
            return (0..=k)
                .map(|j| off + j as f64 * d)
                .filter(|x| *x > 1e-9 && *x < total - 1e-9)
                .map(|x| x / total)
                .collect();
        }
        let n = (self.div_n as usize).max(1);
        match self.div_ratio {
            // 比例分割：間隔が 1 から r へ一次で変わる
            Some(r) if n > 1 => {
                let sp: Vec<f64> = (0..n).map(|j| 1.0 + (r - 1.0) * j as f64 / (n - 1) as f64).collect();
                let sum: f64 = sp.iter().sum();
                let mut acc = 0.0;
                sp[..n - 1].iter().map(|v| {
                    acc += v;
                    acc / sum
                })
                .collect()
            }
            _ => (1..n).map(|k| k as f64 / n as f64).collect(),
        }
    }

    /// 分割：L = 線・円、R = 読取点。２点の後は L = 線上・円周上の分割、R = ２点間。
    /// RR = 連続点分割、始点 R → 円の読取点でない所を R = 円全周分割
    fn divide_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        if let Some(series) = &mut self.div_series {
            let Some(x) = pt else { return };
            let last = *series.last().unwrap();
            if dist(x, last) < 1e-9 || b.shift {
                if dist(x, last) >= 1e-9 {
                    series.push(x);
                }
                let s = series.clone();
                let total: f64 = s.windows(2).map(|w| dist(w[0], w[1])).sum();
                let mut ts = self.div_ts(ed, total);
                if self.div_ends {
                    ts.insert(0, 0.0);
                    ts.push(1.0);
                }
                ed.begin();
                ed.div_polyline_ts(&s, &ts);
                self.div_series = None;
            } else {
                series.push(x);
            }
            return;
        }
        if b.dbl && b.right && self.div_items.is_empty() {
            if let Some(x) = pt {
                self.div_series = Some(vec![x]);
            }
            return;
        }
        // 円全周分割：始点（R）の後、円の読取点でない所を R
        if let ([Div::Pt(s0)], true, None) = (&self.div_items[..], b.right, pt) {
            let s0 = *s0;
            if let Some(i) = ed.pick(q, tol, is_circ) {
                if let Some(Curve::Circ { c, r, .. }) = ed.curve(i) {
                    let ccw = norm_ang(angle_of(sub(q, c)) - angle_of(sub(s0, c))) < std::f64::consts::PI;
                    let ts = self.div_ts(ed, std::f64::consts::TAU * r);
                    ed.begin();
                    ed.div_full_circle(i, s0, ccw, &ts);
                    self.div_items.clear();
                }
                return;
            }
        }
        // ２点の後の R は、どこでも「２点間」
        let two_pts = matches!(self.div_items[..], [Div::Pt(_), Div::Pt(_)]);
        let item = if b.right {
            match pt {
                Some(x) => Div::Pt(x),
                None if two_pts => Div::Pt(q),
                None => return,
            }
        } else {
            match ed.pick(q, tol, is_curve) {
                Some(i) => Div::Curve(i, q),
                None => return,
            }
        };
        self.div_items.push(item);
        let items = self.div_items.clone();
        let done = match items[..] {
            [Div::Curve(i, pi), Div::Curve(j, pj)] => {
                let (ci, cj) = (ed.curve(i), ed.curve(j));
                match (ci, cj) {
                    (Some(Curve::Circ { c: c1, r: r1, .. }), Some(Curve::Circ { c: c2, r: r2, .. })) => {
                        let total = if dist(c1, c2) > 1e-9 { dist(c1, c2) } else { (r1 - r2).abs() };
                        let ts = self.div_ts(ed, total);
                        ed.begin();
                        ed.div_circles_ts(i, j, &ts);
                    }
                    (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) => {
                        // 線長割合：最初の間隔 : 最後の間隔 = 線 A の長さ : 線 B の長さ
                        if let Some(la) = self.div_ratio {
                            self.div_ratio = Some(dist(a2, b2) / la.max(1e-12));
                        }
                        let parallel = line_line(a1, b1, a2, b2).is_none();
                        if self.div_angle && !parallel {
                            let ts = self.div_ts(ed, 1.0);
                            ed.begin();
                            ed.div_angle_lines(i, pi, j, pj, &ts);
                        } else {
                            let total = if parallel {
                                dist(a2, project(a1, b1, a2))
                            } else {
                                let (x1, y1, x2, y2) = if (dot(sub(b1, a1), sub(b2, a2)) < 0.0) != self.div_rev { (a1, b1, b2, a2) } else { (a1, b1, a2, b2) };
                                dist(x1, x2).max(dist(y1, y2))
                            };
                            let ts = self.div_ts(ed, total);
                            ed.begin();
                            match self.div_umame {
                                Some(m) => ed.div_umame(i, j, &ts, m as usize, self.div_rev),
                                None => ed.div_lines_ts(i, j, &ts, self.div_rev),
                            };
                        }
                    }
                    _ => {}
                }
                true
            }
            [Div::Curve(i, _), Div::Pt(x)] | [Div::Pt(x), Div::Curve(i, _)] => {
                match ed.curve(i) {
                    Some(Curve::Circ { c, .. }) => {
                        let ts = self.div_ts(ed, dist(c, x));
                        ed.begin();
                        ed.div_circle_point_ts(i, x, &ts);
                    }
                    Some(Curve::Seg(a, c)) => {
                        let ts = self.div_ts(ed, dist(x, project(a, c, x)));
                        ed.begin();
                        ed.div_line_point_ts(i, x, &ts);
                    }
                    None => {}
                }
                true
            }
            [Div::Pt(_), Div::Pt(_)] => false,
            [Div::Pt(s0), Div::Pt(t0), Div::Curve(i, _)] => {
                let ts = self.div_ts(ed, ed.along_len(i, s0, t0));
                ed.begin();
                ed.div_along_ts(i, s0, t0, &ts);
                true
            }
            [Div::Pt(s0), Div::Pt(t0), Div::Pt(_)] => {
                let ts = self.div_ts(ed, dist(s0, t0));
                ed.begin();
                ed.div_points_ts(s0, t0, &ts);
                true
            }
            _ => false,
        };
        if done {
            self.div_items.clear();
            self.div_rev = false;
            self.div_ratio = None;
            self.div_umame = None;
        }
    }

    /// 伸縮：L で線を選んで L = 任意点・R = 読取点まで。RR = 基準線、その後 L の線は基準線まで。R = 切断
    fn stretch_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        let pr = num(&self.protrude).unwrap_or(0.0);
        if self.batch {
            self.batch_click(ed, q, b, pt, tol);
            return;
        }
        if b.dbl && b.right {
            if let Some(j) = ed.pick(q, tol, is_curve) {
                self.stretch_base = Some((j, q));
                self.picks.clear();
            }
            return;
        }
        if self.picks.is_empty() {
            let Some(i) = ed.pick(q, tol, is_curve) else { return };
            if b.right {
                ed.begin();
                if !ed.cut_at(i, q) {
                    ed.undo();
                }
                return;
            }
            if let Some((j, jq)) = self.stretch_base {
                if i != j {
                    ed.begin();
                    if !ed.stretch_to(i, q, j, Some(jq), pr) {
                        ed.undo();
                    }
                }
                return;
            }
            if matches!(ed.curve(i), Some(Curve::Circ { full: true, .. })) {
                self.note = Some("円は伸縮できません（先に切断してください）".into());
                return;
            }
            self.picks.push((i, q));
            return;
        }
        let Some(x) = pt else { return };
        let (i, pi) = self.picks[0];
        ed.begin();
        if !ed.stretch_pt(i, pi, x, pr) {
            ed.undo();
        }
        self.picks.clear();
    }

    /// 伸縮「一括処理」：基準線（L）／任意点（LL）／読取点（R）→ 始めの線 → 終わりの線 → 追加・除外（L）→ R で実行
    fn batch_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        let Some(target) = self.batch_target else {
            if b.right {
                if let Some(x) = pt {
                    self.batch_target = Some(Target::Pt(x));
                }
            } else if b.dbl {
                self.batch_target = Some(Target::Pt(q));
            } else if let Some(j) = ed.pick(q, tol, is_line) {
                self.batch_target = Some(Target::Line(j, q));
            }
            return;
        };
        let tgt_line = match target {
            Target::Line(j, _) => Some(j),
            Target::Pt(_) => None,
        };
        let Some((i0, q0, same)) = self.batch_first else {
            if let Some(i) = ed.pick(q, tol, |e| is_line(e)) {
                self.batch_first = Some((i, q, b.right));
            }
            return;
        };
        if self.batch_seg.is_none() {
            if ed.pick(q, tol, is_line).is_none() {
                return;
            }
            let seg = Curve::Seg(q0, q);
            let a0 = ed.doc.ents[i0].a.clone();
            let same = same || b.right;
            ed.sel = (0..ed.doc.ents.len())
                .filter(|&k| Some(k) != tgt_line && ed.editable(&ed.doc.ents[k].a) && is_line(&ed.doc.ents[k]))
                .filter(|&k| !same || (ed.doc.ents[k].a.color == a0.color && ed.doc.ents[k].a.style == a0.style && ed.doc.ents[k].a.layer == a0.layer))
                .filter(|&k| ed.curve(k).map_or(false, |c| !c.meet(&seg, false).is_empty()))
                .collect();
            self.batch_seg = Some((q0, q));
            return;
        }
        if b.right {
            self.batch_run(ed);
            return;
        }
        if let Some(i) = ed.pick(q, tol, is_line) {
            if let Some(k) = ed.sel.iter().position(|&s| s == i) {
                ed.sel.remove(k);
            } else if Some(i) != tgt_line {
                ed.sel.push(i);
            }
        }
    }

    /// 一括処理を実行
    pub fn batch_run(&mut self, ed: &mut Editor) {
        let (Some(target), Some((s0, s1))) = (self.batch_target, self.batch_seg) else { return };
        let pr = num(&self.protrude).unwrap_or(0.0);
        let seg = Curve::Seg(s0, s1);
        let mut sel = ed.sel.clone();
        sel.sort_unstable();
        ed.begin();
        let mut n = 0;
        for i in sel {
            let Some(c) = ed.curve(i) else { continue };
            // 選んだ線上の「残す側」：横切った位置
            let hit = c.meet(&seg, true).into_iter().next().unwrap_or_else(|| c.nearest(s0));
            let ok = match target {
                Target::Line(j, jq) => ed.stretch_to(i, hit, j, Some(jq), pr),
                Target::Pt(x) => ed.stretch_pt(i, hit, x, pr),
            };
            n += ok as usize;
        }
        self.note = Some(format!("{n} 本を伸縮しました"));
        ed.sel.clear();
        self.batch_target = None;
        self.batch_first = None;
        self.batch_seg = None;
    }

    /// 接円：１〜３番目 L = 線・円、R = 読取点。半径あり = ２つの後に位置、なし = ３つで作図
    fn tan_circle_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        let r = num(&self.tan_r).map(|v| ed.paper(v));
        if let (Some(r), 2) = (r, self.tan_objs.len()) {
            let cs = Editor::tan_centers([self.tan_objs[0].0, self.tan_objs[1].0], r);
            match cs.into_iter().min_by(|a, b| dist(*a, q).total_cmp(&dist(*b, q))) {
                Some(c) => {
                    ed.begin();
                    ed.push(arc_shape(c, r, 0.0, std::f64::consts::TAU));
                }
                None => self.note = Some("接円がありません".into()),
            }
            self.tan_objs.clear();
            return;
        }
        let obj = if b.right {
            match pt {
                Some(x) => (TanObj::Pt(x), x),
                None => return,
            }
        } else {
            match ed.pick(q, tol, is_curve).and_then(|i| ed.curve(i)) {
                Some(c) => (TanObj::of(c), q),
                None => return,
            }
        };
        self.tan_objs.push(obj);
        if r.is_none() && self.tan_objs.len() == 3 {
            let o = &self.tan_objs;
            match Editor::tan_circle3([o[0].0, o[1].0, o[2].0], [o[0].1, o[1].1, o[2].1]) {
                Some((c, rr)) => {
                    ed.begin();
                    ed.push(arc_shape(c, rr, 0.0, std::f64::consts::TAU));
                }
                None => self.note = Some("接円がありません".into()),
            }
            self.tan_objs.clear();
        }
    }

    /// 寸法の位置の数（＝ は引出線始点・寸法線の２つ、－ は寸法線だけ）
    fn dim_npos(&self) -> usize {
        if self.dim_eq {
            2
        } else {
            1
        }
    }
    /// 寸法線の位置と引出線始点
    fn dim_at(&self) -> Option<(P, Option<P>)> {
        let n = self.dim_npos();
        (self.pts.len() >= n).then(|| (self.pts[n - 1], if n == 2 { Some(self.pts[0]) } else { None }))
    }

    /// 寸法：位置は L 任意点・R 読取点。測る点は L・R どちらも読取点。
    /// ２つ目以降：R で終点だけ = 前の終点から続ける、L = 新しい始点
    fn dim_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, read: Option<P>, tol: f64) {
        if self.picks.first().map(|x| x.0) == Some(usize::MAX) {
            // 線角：取线的角度
            if let Some(i) = ed.pick(q, tol, is_line) {
                if let Some(Curve::Seg(a, c)) = ed.curve(i) {
                    let mut d = deg(angle_of(sub(c, a)));
                    d = (d + 360.0) % 180.0;
                    self.dim_dir = DimDir::Angle(d);
                    self.note = Some(format!("寸法の傾き {d:.4}°"));
                }
            }
            self.picks.clear();
            return;
        }
        let npos = self.dim_npos();
        match self.dim_kind {
            DimKind::Radius | DimKind::Diameter => {
                // L = 寸法値を円の内側、R = 外側
                if let Some(i) = ed.pick(q, tol, is_circ) {
                    ed.begin();
                    ed.dim_radius_side(i, q, self.dim_kind == DimKind::Diameter, b.right);
                }
            }
            DimKind::Angle => {
                // ２線間：LL で１本目、２本目（左回り）、位置
                if self.dim_two || (b.dbl && !b.right && self.pts.is_empty() && self.picks.is_empty()) {
                    self.dim_two = true;
                    if self.picks.len() < 2 {
                        if let Some(i) = ed.pick(q, tol, is_line) {
                            self.picks.push((i, q));
                        }
                        return;
                    }
                    let Some(x) = pt else { return };
                    let ((i, pi), (j, pj)) = (self.picks[0], self.picks[1]);
                    ed.begin();
                    ed.dim_two_lines(i, pi, j, pj, x);
                    self.picks.clear();
                    self.dim_two = false;
                    return;
                }
                // 原点 → 位置 → 始点 → 終点（左回り）
                if self.pts.len() < 1 + npos {
                    if let Some(x) = pt {
                        self.pts.push(x);
                    }
                    return;
                }
                let Some(x) = read else {
                    self.note = Some("点がありません".into());
                    return;
                };
                if self.pts.len() == 1 + npos {
                    self.pts.push(x);
                    return;
                }
                let c = self.pts[0];
                let rd = dist(c, self.pts[npos]);
                let rl = (npos == 2).then(|| dist(c, self.pts[1]));
                let s = angle_of(sub(self.pts[1 + npos], c));
                let w = norm_ang(angle_of(sub(x, c)) - s);
                ed.begin();
                ed.dim_arc_lead(c, rd, rl, s, w, &format!("{}°", crate::edit::fmt_num(deg(w), 2)));
                self.pts.truncate(1 + npos);
            }
            DimKind::ArcLen => {
                // 円 → 位置 → 始点 → 終点（左回り）
                let Some(&(i, _)) = self.picks.first() else {
                    if let Some(i) = ed.pick(q, tol, is_circ) {
                        self.picks.push((i, q));
                    }
                    return;
                };
                let Some(Curve::Circ { c, r, .. }) = ed.curve(i) else { return };
                if self.pts.len() < npos {
                    if let Some(x) = pt {
                        self.pts.push(x);
                    }
                    return;
                }
                let Some(x) = read else {
                    self.note = Some("点がありません".into());
                    return;
                };
                if self.pts.len() == npos {
                    self.pts.push(x);
                    return;
                }
                let s = angle_of(sub(self.pts[npos], c));
                let w = norm_ang(angle_of(sub(x, c)) - s);
                let rd = dist(c, self.pts[npos - 1]);
                let rl = (npos == 2).then(|| dist(c, self.pts[0]));
                ed.begin();
                let txt = format!("⌒{}", crate::edit::fmt_len(ed.real(r * w)));
                ed.dim_arc_lead(c, rd, rl, s, w, &txt);
                self.pts.truncate(npos);
            }
            DimKind::Value => self.dim_value_click(ed, q, b, pt, read, tol),
            DimKind::Line | DimKind::Cumulative if self.dim_batch && self.pts.len() >= npos => {
                // 一括処理：始めの線 → 終わりの線（R = 同じ属性だけ）→「実行」
                let Some(i) = ed.pick(q, tol, is_line) else { return };
                match self.dim_b_first {
                    None => self.dim_b_first = Some((i, q, b.right)),
                    Some((i0, q0, same)) => {
                        let seg = Curve::Seg(q0, q);
                        let a0 = ed.doc.ents[i0].a.clone();
                        let same = same || b.right;
                        let u = p(self.dim_ang().cos(), self.dim_ang().sin());
                        ed.sel = (0..ed.doc.ents.len())
                            .filter(|&k| ed.editable(&ed.doc.ents[k].a))
                            .filter(|&k| !same || (ed.doc.ents[k].a.color == a0.color && ed.doc.ents[k].a.style == a0.style))
                            .filter(|&k| match ed.curve(k) {
                                // 寸法線と平行な線は対象外
                                Some(c @ Curve::Seg(a, e)) => cross(norm(sub(e, a)), u).abs() > 1e-6 && !c.meet(&seg, false).is_empty(),
                                _ => false,
                            })
                            .collect();
                        self.dim_b_first = None;
                        self.note = Some(format!("{} 本選びました。「実行」で寸法を作図", ed.sel.len()));
                    }
                }
            }
            DimKind::Line | DimKind::Cumulative => {
                if self.pts.len() < npos {
                    if let Some(x) = pt {
                        self.pts.push(x);
                    }
                    return;
                }
                let Some(x) = read else {
                    self.note = Some("点がありません".into());
                    return;
                };
                let (at, lead) = self.dim_at().unwrap();
                if self.dim_kind == DimKind::Cumulative {
                    // 累進：始点（起点）の後は続けて終点
                    match (self.dim_start, self.dim_last) {
                        (None, _) => self.dim_start = Some(x),
                        (Some(base), prev) => {
                            ed.begin();
                            ed.dim_cumulative(base, prev.unwrap_or(base), x, self.dim_ang(), at, lead, prev.is_none());
                            self.dim_last = Some(x);
                        }
                    }
                    return;
                }
                let from = match (self.dim_start, self.dim_last) {
                    (Some(s), _) => Some(s),
                    (None, Some(l)) if b.right => Some(l),
                    _ => None,
                };
                match from {
                    None => self.dim_start = Some(x),
                    Some(s) => {
                        ed.begin();
                        ed.dim_lead(s, x, self.dim_ang(), at, lead);
                        if self.dim_first.is_none() {
                            self.dim_first = Some(s);
                        }
                        self.dim_count += 1;
                        self.dim_start = None;
                        self.dim_last = Some(x);
                    }
                }
            }
        }
    }

    /// 寸法値：L で始点・終点（読取点）= 値だけ作図。R で寸法値 = 移動、RR で寸法値 = 変更
    fn dim_value_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, read: Option<P>, tol: f64) {
        let is_text = |e: &Ent| matches!(e.shape, Shape::Text(_));
        if let Some(i) = self.dim_val_move {
            let Some(x) = pt else { return };
            if let Shape::Text(t) = &ed.doc.ents[i].shape {
                let c = mid(t.start, t.end);
                ed.begin();
                ed.move_(&[i], sub(x, c));
            }
            self.dim_val_move = None;
            return;
        }
        if b.right {
            let Some(i) = ed.pick(q, tol, is_text) else { return };
            if b.dbl {
                if let Shape::Text(t) = &ed.doc.ents[i].shape {
                    self.dim_val_text = t.s.clone();
                }
                self.dim_val_edit = Some(i);
            } else {
                self.dim_val_move = Some(i);
            }
            return;
        }
        let Some(x) = read else {
            self.note = Some("点がありません".into());
            return;
        };
        match self.dim_start {
            None => self.dim_start = Some(x),
            Some(s0) => {
                ed.begin();
                ed.dim_value_only(s0, x);
                self.dim_start = None;
            }
        }
    }

    /// 寸法の一括処理を実行：選んだ線の寸法線に近い端点を並べて寸法（累進なら累進）
    pub fn dim_batch_run(&mut self, ed: &mut Editor) {
        let Some((at, lead)) = self.dim_at() else { return };
        let ang = self.dim_ang();
        let u = p(ang.cos(), ang.sin());
        let n = perp(u);
        let off = dot(at, n);
        let mut pts: Vec<P> = ed
            .sel
            .iter()
            .filter_map(|&k| match ed.curve(k) {
                Some(Curve::Seg(a, e)) => Some(if (dot(a, n) - off).abs() <= (dot(e, n) - off).abs() { a } else { e }),
                _ => None,
            })
            .collect();
        pts.sort_by(|a, b| dot(*a, u).total_cmp(&dot(*b, u)));
        pts.dedup_by(|a, b| (dot(*a, u) - dot(*b, u)).abs() < 1e-9);
        if pts.len() < 2 {
            self.note = Some("線が２本以上要ります".into());
            return;
        }
        ed.begin();
        if self.dim_kind == DimKind::Cumulative {
            for k in 1..pts.len() {
                ed.dim_cumulative(pts[0], pts[k - 1], pts[k], ang, at, lead, k == 1);
            }
        } else {
            for w in pts.windows(2) {
                ed.dim_lead(w[0], w[1], ang, at, lead);
            }
        }
        ed.sel.clear();
        self.dim_batch = false;
        self.note = Some(format!("{} 個の寸法を作図しました", pts.len() - 1));
    }

    /// 文字：L で既存の文字 = 変更・移動、R = 複写。何もない所 = 新しく書く
    fn text_click(&mut self, ed: &mut Editor, q: P, b: Btn, pt: Option<P>, tol: f64) {
        let is_text = |e: &Ent| matches!(e.shape, Shape::Text(_));
        if self.text_link {
            // 連：L = 連結する文字、LL = 移動、R = その位置で切断
            if let Some(i) = self.link_move {
                let Some(x) = pt else { return };
                if let Shape::Text(t) = &ed.doc.ents[i].shape {
                    let d = sub(x, t.start);
                    ed.begin();
                    ed.move_(&[i], d);
                }
                self.link_move = None;
                return;
            }
            let Some(j) = ed.pick(q, tol, is_text) else { return };
            if b.dbl && !b.right {
                self.link_src = None;
                self.link_move = Some(j);
                return;
            }
            match self.link_src {
                None if b.right => {
                    ed.begin();
                    if !ed.text_cut(j, q) {
                        ed.undo();
                    }
                }
                None => {
                    // 指示位置が文字の前半 = 前付け
                    if let Shape::Text(t) = &ed.doc.ents[j].shape {
                        self.link_front = dist(q, t.start) < dist(q, t.end);
                    }
                    self.link_src = Some(j);
                }
                Some(i) => {
                    ed.begin();
                    if !ed.text_join(i, j, self.link_front, b.right) {
                        ed.undo();
                    }
                    self.link_src = None;
                }
            }
            return;
        }
        if let Some((i, copy)) = self.text_src {
            let Some(x) = pt else { return };
            let s = self.text.clone();
            ed.begin();
            if copy {
                ed.copy_text_to(i, x, &s);
            } else {
                if let Shape::Text(t) = &ed.doc.ents[i].shape {
                    if t.s != s {
                        ed.set_text(i, &s);
                    }
                }
                if let Shape::Text(t) = &ed.doc.ents[i].shape {
                    let d = sub(x, t.start);
                    ed.move_(&[i], d);
                }
            }
            self.text_src = None;
            return;
        }
        if let Some(i) = ed.pick(q, tol, is_text) {
            if let Shape::Text(t) = &ed.doc.ents[i].shape {
                self.text = t.s.clone();
            }
            self.text_src = Some((i, b.right));
            return;
        }
        let Some(x) = pt else { return };
        if self.text.is_empty() {
            self.note = Some("文字を入力してください".into());
            return;
        }
        ed.begin();
        let s = self.text.clone();
        ed.text(x, &s);
    }

    /// 右ドラッグの読取り（クロックメニュー AM 0・3・6・9 時）。
    /// すぐ点が決まれば返す（右クリックの読取点として使う）。もう一点要るものは待ち状態にする
    pub fn right_drag_read(&mut self, ed: &mut Editor, q0: P, clock: usize, tol: f64) -> Option<P> {
        let curve = ed.pick(q0, tol, is_curve);
        let read = ed.read_point(q0, tol);
        match clock {
            0 => {
                // 線コマンド：線・円から上へ = 垂線
                if self.kind == Kind::Line && self.pts.is_empty() {
                    if let Some(c) = curve.and_then(|i| ed.curve(i)) {
                        let s = match c {
                            Curve::Seg(a, b) => project(a, b, q0),
                            _ => c.nearest(q0),
                        };
                        self.pts.push(s);
                        self.perp_ref = Some(c);
                        self.note = Some("垂線：終点を指示してください".into());
                        return None;
                    }
                }
                // 円周 1/4 点
                match curve.and_then(|i| ed.curve(i)) {
                    Some(Curve::Circ { c, r, .. }) => {
                        let ks = [0.0, 90.0, 180.0, 270.0].map(|d| polar(c, r, rad(d)));
                        ks.into_iter().min_by(|a, b| dist(*a, q0).total_cmp(&dist(*b, q0)))
                    }
                    _ => {
                        self.note = Some("円がありません".into());
                        None
                    }
                }
            }
            3 => {
                if let Some(r) = read {
                    self.rd = Some(Rd::Mid(r));
                    self.note = Some("２点間中心：もう一方の点を指示してください".into());
                    return None;
                }
                match curve.and_then(|i| ed.curve(i)) {
                    Some(Curve::Seg(a, b)) => Some(mid(a, b)),
                    Some(Curve::Circ { c, .. }) => Some(c),
                    None => {
                        self.note = Some("線・円がありません".into());
                        None
                    }
                }
            }
            6 => match read {
                Some(r) => {
                    self.rd = Some(Rd::Offset(r, String::new()));
                    None
                }
                None => {
                    self.note = Some("点がありません".into());
                    None
                }
            },
            9 => match curve {
                Some(i) => {
                    self.rd = Some(Rd::OnLine(i));
                    self.note = Some("線上点：点を指示してください（その点から線への垂線の足）".into());
                    None
                }
                None => {
                    self.note = Some("線・円がありません".into());
                    None
                }
            },
            _ => None,
        }
    }

    /// オフセット（右 AM6 時）の入力ダイアログ。OK なら点を返す
    pub fn offset_dialog(&mut self, ctx: &egui::Context, ed: &Editor) -> Option<P> {
        let Some(Rd::Offset(base, s)) = &mut self.rd else { return None };
        let base = *base;
        let mut ok = false;
        let mut cancel = false;
        egui::Window::new("オフセット").collapsible(false).resizable(false).show(ctx, |ui| {
            ui.label("読取点からのずれ（実寸）を X,Y で入力");
            let r = ui.add(egui::TextEdit::singleline(s).desired_width(160.0));
            r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                ok = true;
            }
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel = ui.button("キャンセル").clicked();
            });
        });
        if cancel {
            self.rd = None;
            return None;
        }
        if !ok {
            return None;
        }
        let v = parse_nums(s);
        self.rd = None;
        let (dx, dy) = (v.first().copied().unwrap_or(0.0), v.get(1).copied().unwrap_or(0.0));
        Some(add(base, p(ed.paper(dx), ed.paper(dy))))
    }

    /// 左ドラッグの命令固有の動き（伸縮 8 時 = 端点移動、包絡 9 時 = 中間線消去）。やったら true
    pub fn left_drag_special(&mut self, ed: &mut Editor, q0: P, clock: usize, tol: f64) -> bool {
        match (self.kind, clock) {
            (Kind::Stretch, 8) => {
                if let Some(i) = ed.pick(q0, tol, is_line) {
                    self.end_move = Some((i, q0));
                    self.note = Some("端点移動：移動先を指示してください".into());
                    return true;
                }
                false
            }
            (Kind::Envelope, 9) if !self.pts.is_empty() => {
                let a = self.pts[0];
                ed.begin();
                let n = ed.erase_through_rect(a, q0);
                self.note = Some(format!("中間線消去しました（{n} 本）"));
                self.pts.clear();
                true
            }
            _ => false,
        }
    }

    /// AUTO モード
    fn auto_click(&mut self, ed: &mut Editor, q: P, b: Btn, read: Option<P>, tol: f64) {
        let pt = if b.right { read } else { Some(q) };
        let Some((i, pi)) = self.auto_a else {
            let curve = ed.pick(q, tol, is_curve);
            if b.right {
                if let Some(r) = read {
                    // 読取点：R = 線の始点、RR = 円の中心
                    self.auto_go(ed, if b.dbl { Kind::Circle } else { Kind::Line });
                    self.pts.push(r);
                } else if let Some(i) = curve {
                    // 線上 R = 複線（前回の間隔）
                    self.auto_go(ed, Kind::Offset);
                    self.picks.push((i, q));
                    if let Some(d) = self.offset_prev {
                        self.offset = num_str(d);
                    }
                } else {
                    // 何もない所 R = 矩形
                    self.auto_go(ed, Kind::Rect);
                }
                return;
            }
            match curve {
                Some(i) => self.auto_a = Some((i, q)),
                None => {
                    self.auto_go(ed, if b.dbl { Kind::Circle } else { Kind::Line });
                    self.pts.push(q);
                }
            }
            return;
        };
        let Some(ca) = ed.curve(i) else {
            self.auto_a = None;
            return;
        };
        match self.auto_sub {
            AutoSub::Part(s) => {
                // 部分消しの終点。R で A 上の読取点でない所 = 節間消し
                if b.right && read.is_none() && ed.pick(q, tol, is_curve) == Some(i) {
                    ed.begin();
                    ed.erase_between(i, q);
                } else if let Some(x) = pt {
                    ed.begin();
                    if dist(s, x) < 1e-9 {
                        ed.cut_at(i, s);
                    } else {
                        ed.erase_part(i, s, x);
                    }
                } else {
                    return;
                }
            }
            AutoSub::Stretch => {
                let Some(x) = pt else { return };
                ed.begin();
                ed.stretch_pt(i, pi, x, 0.0);
            }
            AutoSub::None => {
                let other = ed.pick(q, tol, is_curve);
                if other == Some(i) {
                    if b.right && read.is_none() {
                        ed.begin();
                        ed.erase(vec![i]);
                    } else if let Some(x) = pt {
                        self.auto_sub = AutoSub::Part(x);
                        return;
                    } else {
                        return;
                    }
                } else if let (Some(j), false) = (other, b.right) {
                    ed.begin();
                    if !ed.corner(i, pi, j, q) {
                        ed.undo();
                    }
                } else if let (Some(j), true, None) = (other, b.right, read) {
                    ed.begin();
                    ed.stretch_to(i, pi, j, Some(q), 0.0);
                } else {
                    let Some(x) = pt else { return };
                    // 線の範囲内 = 部分消しの始点、範囲外 = 伸縮
                    let inside = match ca {
                        Curve::Seg(a, c) => (0.0..=1.0).contains(&param(a, c, x)),
                        c => c.on_arc(x),
                    };
                    self.auto_sub = if inside { AutoSub::Part(x) } else { AutoSub::Stretch };
                    return;
                }
            }
        }
        self.auto_a = None;
        self.auto_sub = AutoSub::None;
    }

    /// AUTO から別の命令へ（作図が終わったら戻る）
    fn auto_go(&mut self, ed: &mut Editor, k: Kind) {
        self.kind = k;
        self.pts.clear();
        self.picks.clear();
        ed.sel.clear();
        self.auto_ret = true;
    }

    /// 线的终点：水平・垂直、倾斜角、长度的约束
    fn line_end(&self, ed: &Editor, a: P, x: P, read: bool) -> P {
        let mut dir = sub(x, a);
        if let Some(d) = parse_angle(&self.angle) {
            // 固定角度：取和光标同侧的方向
            let u = p(rad(d).cos(), rad(d).sin());
            let s = if dot(dir, u) >= 0.0 { 1.0 } else { -1.0 };
            let u = mul(u, s);
            if read && num(&self.len).is_none() {
                return project(a, add(a, u), x);
            }
            dir = mul(u, dot(dir, u).abs().max(1e-9));
        } else if self.hv {
            dir = if dir.x.abs() >= dir.y.abs() { p(dir.x, 0.0) } else { p(0.0, dir.y) };
            if read && num(&self.len).is_none() {
                return add(a, dir);
            }
        }
        match num(&self.len) {
            Some(l) => add(a, mul(norm(dir), ed.paper(l))),
            None => add(a, dir),
        }
    }

    /// 中心線の軸（無限線上の２点）：線と線・点と点・線と点
    fn center_axis(&self, ed: &Editor) -> Option<(P, P)> {
        match (self.picks.len(), self.pts.len()) {
            (2, _) => ed.center_of_lines(self.picks[0].0, self.picks[1].0, self.picks[0].1, self.picks[1].1),
            (0, n) if n >= 2 => Some(Editor::center_of_points(self.pts[0], self.pts[1])),
            (1, n) if n >= 1 => {
                let Some(Curve::Seg(a, b)) = ed.curve(self.picks[0].0) else { return None };
                let x = self.pts[0];
                let m = mid(x, project(a, b, x));
                Some((m, add(m, sub(b, a))))
            }
            _ => None,
        }
    }

    // ---------- 预览 ----------

    /// 光标在 q（读取点 read）时要画的临时图形，和要高亮的图形
    pub fn preview(&self, ed: &Editor, q: P) -> (Vec<Prev>, Vec<usize>) {
        let mut v = Vec::new();
        let mut hl: Vec<usize> = self.picks.iter().map(|x| x.0).filter(|&i| i != usize::MAX).collect();
        hl.extend(ed.sel.iter().copied());
        hl.extend(self.stretch_base.map(|x| x.0));
        hl.extend(self.auto_a.map(|x| x.0));
        hl.extend(self.end_move.map(|x| x.0));
        if let Some(Rd::OnLine(i)) = self.rd {
            hl.push(i);
        }
        if let Some((i, _)) = self.text_src {
            hl.push(i);
        }
        if let Some(Target::Line(j, _)) = self.batch_target {
            hl.push(j);
        }
        if let Some((i, _, _)) = self.batch_first {
            hl.push(i);
        }
        hl.retain(|&i| i < ed.doc.ents.len());
        if self.pick.is_some() {
            for &x in &self.pick_pts {
                v.push(Prev::Mark(x));
                v.push(Prev::Line(x, q));
            }
            if let Some(i) = self.pick_line {
                hl.push(i);
            }
            return (v, hl);
        }
        match self.kind {
            Kind::Symbol => {
                let need_pos = self.cur_sym().map_or(false, |s| !s.needs_line || (s.count != 0 && !self.picks.is_empty()));
                if need_pos {
                    if let (Some(s), Some((o, ang, _))) = (self.cur_sym(), self.sym_place(ed, q, Some(q))) {
                        let mag = num(&self.sym_mag).unwrap_or(1.0);
                        for (_, _, _, sh) in crate::symbol::symbol_shapes(ed, s, o, ang, mag, &self.sym_inputs()) {
                            push_shape(&mut v, &sh);
                        }
                    }
                }
            }
            Kind::Param => {
                if self.pts.len() == 1 {
                    v.push(Prev::Rect(self.pts[0], q));
                } else if self.pts.len() == 2 {
                    v.push(Prev::Rect(self.pts[0], self.pts[1]));
                    if let Some(b) = self.base {
                        v.push(Prev::Mark(b));
                        v.push(Prev::Line(b, q));
                    }
                }
            }
            Kind::Pen => {
                let div = self.pen_div.max(2) as usize;
                let mut v2 = self.pen.clone();
                // ドラッグ中はその点、そうでなければカーソルまでの次の区間
                match self.pen_live {
                    Some((a, b)) => v2.push(Anchor { p: a, h_in: sub(mul(a, 2.0), b), h_out: b }),
                    None if !self.pen.is_empty() => v2.push(Anchor { p: q, h_in: q, h_out: q }),
                    None => {}
                }
                for w in pen_points(&v2, false, div).windows(2) {
                    v.push(Prev::Line(w[0], w[1]));
                }
                for (k, a) in v2.iter().enumerate() {
                    if self.pen_live.is_none() && k == v2.len() - 1 && k == self.pen.len() {
                        continue;
                    }
                    v.push(Prev::Mark(a.p));
                    for h in [a.h_in, a.h_out] {
                        if dist(h, a.p) > 1e-12 {
                            v.push(Prev::Line(a.p, h));
                            v.push(Prev::Mark(h));
                        }
                    }
                }
            }
            Kind::Spline if !self.pts.is_empty() => {
                let mut ps = self.pts.clone();
                ps.push(q);
                for w in crate::edit::spline_pts(&ps, self.spline_div.max(1) as usize).windows(2) {
                    v.push(Prev::Line(w[0], w[1]));
                }
                for &x in &self.pts {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::Dim if self.dim_kind == DimKind::Angle => {
                for &x in &self.pts {
                    v.push(Prev::Mark(x));
                }
                if let Some(&c) = self.pts.first() {
                    v.push(Prev::Line(c, q));
                }
            }
            Kind::Figure => {
                if let Some(fig) = &self.fig {
                    let (mag, ang) = self.fig_params();
                    let ws = ed.scale();
                    for e in fig.ents.iter().take(3000) {
                        let k = fig.scales[e.a.glayer as usize & 15] / ws * mag;
                        let mut e = e.clone();
                        crate::edit::transform(&mut e, fig.base, q, k, rad(ang));
                        push_shape(&mut v, &e.shape);
                    }
                }
            }
            Kind::Paste => {
                let d = sub(q, self.clip_base);
                for e in &self.clip {
                    let mut s = e.shape.clone();
                    crate::edit::translate(&mut s, d);
                    push_shape(&mut v, &s);
                }
            }
            Kind::Line if self.perp_ref.is_none() => {
                if let Some(&a) = self.pts.first() {
                    v.push(Prev::Line(a, self.line_end(ed, a, q, false)));
                }
            }
            Kind::Rect => {
                if let Some(&a) = self.pts.first() {
                    let s = parse_nums(&self.rect_size);
                    let (c0, c1) = if s.len() >= 2 { ed.rect_corners(a, s[0], s[1], anchor(a, q)) } else { (a, q) };
                    v.push(Prev::Rect(c0, c1));
                } else if let [w, h, ..] = parse_nums(&self.rect_size)[..] {
                    let (c0, c1) = ed.rect_corners(q, w, h, (1, 1));
                    v.push(Prev::Rect(c0, c1));
                }
            }
            Kind::Circle => {
                if self.three {
                    if self.pts.len() == 2 {
                        if let Some((c, r)) = circle3(self.pts[0], self.pts[1], q) {
                            v.push(Prev::Circle(c, r));
                        }
                    }
                } else if let Some(r) = num(&self.radius) {
                    v.push(Prev::Circle(q, ed.paper(r)));
                } else if let Some(&c) = self.pts.first() {
                    v.push(Prev::Circle(c, dist(c, q)));
                }
            }
            Kind::Polygon => {
                if let Some(s) = num(&self.poly_size) {
                    let a = parse_angle(&self.poly_angle).unwrap_or(90.0);
                    let pts = crate::edit::polygon_pts(q, self.poly_n as usize, ed.paper(s), self.poly_mode, a);
                    for k in 0..pts.len() {
                        v.push(Prev::Line(pts[k], pts[(k + 1) % pts.len()]));
                    }
                }
            }
            Kind::Offset => {
                if let Some(&(i, _)) = self.picks.first() {
                    let d = num(&self.offset).map(|d| ed.paper(d)).or_else(|| ed.curve(i).map(|c| c_dist_ext(&c, q)));
                    if let Some(d) = d.filter(|d| d.abs() > 1e-9) {
                        if let Some(s) = crate::edit::offset_shape(&ed.doc.ents[i], d, q) {
                            push_shape(&mut v, &s);
                        }
                    }
                }
            }
            Kind::Line if self.perp_ref.is_some() => {
                if let (Some(&a), Some(c)) = (self.pts.first(), self.perp_ref) {
                    let n = match c {
                        Curve::Seg(s0, t) => perp(norm(sub(t, s0))),
                        Curve::Circ { c, .. } => norm(sub(a, c)),
                    };
                    v.push(Prev::Line(a, add(a, mul(n, dot(sub(q, a), n)))));
                }
            }
            Kind::Center => {
                if self.center_ready() {
                    if let Some(l) = self.center_axis(ed) {
                        match self.center_start {
                            Some(s0) => v.push(Prev::Line(s0, project(l.0, l.1, q))),
                            None => {
                                let f = project(l.0, l.1, q);
                                let u = norm(sub(l.1, l.0));
                                v.push(Prev::Line(sub(f, mul(u, 1000.0)), add(f, mul(u, 1000.0))));
                            }
                        }
                    }
                }
                for &x in &self.pts {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::TwoLine if !self.tl_bases.is_empty() => {
                let nums = parse_nums(&self.twoline);
                let (d1, d2) = match nums.len() {
                    0 => (0.0, 0.0),
                    1 => (nums[0], nums[0]),
                    _ => (nums[0], nums[1]),
                };
                let (d1, d2) = if self.tl_flip { (d2, d1) } else { (d1, d2) };
                match self.tl_start {
                    Some(st) => {
                        if let Some((l, r, _)) = ed.two_line_path(&self.tl_bases, st, End::Pt(q), d1, d2) {
                            for side in [l, r] {
                                for w in side.windows(2) {
                                    v.push(Prev::Line(w[0], w[1]));
                                }
                            }
                        }
                    }
                    None => {
                        let (a, c) = self.tl_bases[0];
                        let u = norm(sub(c, a));
                        v.push(Prev::Line(sub(a, mul(u, 1000.0)), add(a, mul(u, 1000.0))));
                    }
                }
            }
            Kind::Stretch if self.batch => {
                if let (Some((_, q0, _)), None) = (self.batch_first, self.batch_seg) {
                    v.push(Prev::Line(q0, q));
                }
                if let Some((a, c)) = self.batch_seg {
                    v.push(Prev::Line(a, c));
                }
                if let Some(Target::Pt(x)) = self.batch_target {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::Stretch if self.end_move.is_some() => {
                if let Some((i, near)) = self.end_move {
                    if let Some(Curve::Seg(a, c)) = ed.curve(i) {
                        let keep = if dist(near, a) <= dist(near, c) { c } else { a };
                        v.push(Prev::Line(keep, q));
                    }
                }
            }
            Kind::Auto => {
                if let AutoSub::Part(x) = self.auto_sub {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::TanCircle if self.tan_objs.len() == 2 => {
                if let Some(r) = num(&self.tan_r) {
                    let r = ed.paper(r);
                    let cs = Editor::tan_centers([self.tan_objs[0].0, self.tan_objs[1].0], r);
                    if let Some(c) = cs.into_iter().min_by(|a, b| dist(*a, q).total_cmp(&dist(*b, q))) {
                        v.push(Prev::Circle(c, r));
                    }
                }
            }
            Kind::Copy | Kind::Move | Kind::ChangeAttr | Kind::Erase | Kind::Range | Kind::ClipCopy | Kind::FigReg | Kind::Envelope | Kind::Hatch => {
                if let Some(&a) = self.pts.first() {
                    if !self.sel_done {
                        v.push(Prev::Rect(a, q));
                    }
                }
                if matches!(self.kind, Kind::Copy | Kind::Move) && self.sel_done {
                    if let Some(b) = self.base {
                        v.push(Prev::Mark(b));
                        let mut d = sub(q, b);
                        d = match self.axis {
                            Axis::Free => d,
                            Axis::X => p(d.x, 0.0),
                            Axis::Y => p(0.0, d.y),
                        };
                        for &i in &ed.sel {
                            let mut s = ed.doc.ents[i].shape.clone();
                            crate::edit::translate(&mut s, d);
                            push_shape(&mut v, &s);
                        }
                    }
                }
                for &x in self.pts.iter().skip(if self.sel_done { 0 } else { 1 }) {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::Dim if matches!(self.dim_kind, DimKind::Line | DimKind::Cumulative) => {
                let u = p(self.dim_ang().cos(), self.dim_ang().sin());
                let n = perp(u);
                match self.dim_at() {
                    Some((at, _)) => {
                        let off = dot(at, n);
                        let on = |x: P| add(mul(u, dot(x, u)), mul(n, off));
                        match self.dim_start.or(self.dim_last) {
                            Some(s0) => {
                                v.push(Prev::Line(on(s0), on(q)));
                                v.push(Prev::Line(s0, on(s0)));
                                v.push(Prev::Line(q, on(q)));
                            }
                            None => v.push(Prev::Line(sub(on(q), mul(u, 1000.0)), add(on(q), mul(u, 1000.0)))),
                        }
                    }
                    None => {
                        let off = dot(q, n);
                        let c = add(mul(u, dot(q, u)), mul(n, off));
                        v.push(Prev::Line(sub(c, mul(u, 1000.0)), add(c, mul(u, 1000.0))));
                    }
                }
                for &x in &self.pts {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::Solid | Kind::Measure if !self.pts.is_empty() => {
                for w in self.pts.windows(2) {
                    v.push(Prev::Line(w[0], w[1]));
                }
                let last = *self.pts.last().unwrap();
                v.push(Prev::Line(last, q));
                if self.kind != Kind::Measure || self.measure_area {
                    v.push(Prev::Line(q, self.pts[0]));
                }
                for &x in &self.pts {
                    v.push(Prev::Mark(x));
                }
            }
            Kind::Text if !self.text.is_empty() => {
                let k = ed.moji_kind as usize;
                let (w, h, g) = if (1..=10).contains(&k) {
                    let m = ed.doc.moji[k];
                    (m.w, m.h, m.gap)
                } else {
                    ed.moji_any
                };
                let t = crate::edit::make_text(q, &self.text, w, h, g, ed.moji_deg, 0, "");
                let up = mul(perp(norm(sub(t.end, t.start))), h);
                v.push(Prev::Line(t.start, t.end));
                v.push(Prev::Line(t.end, add(t.end, up)));
                v.push(Prev::Line(add(t.end, up), add(t.start, up)));
                v.push(Prev::Line(add(t.start, up), t.start));
            }
            _ => {
                for &x in &self.pts {
                    v.push(Prev::Mark(x));
                }
            }
        }
        if matches!(self.kind, Kind::Line | Kind::Rect | Kind::Circle | Kind::Divide | Kind::Erase | Kind::Tangent) {
            for &x in &self.pts {
                v.push(Prev::Mark(x));
            }
        }
        hl.sort_unstable();
        hl.dedup();
        (v, hl)
    }
}

fn push_shape(v: &mut Vec<Prev>, s: &Shape) {
    match s {
        Shape::Line { a, b } => v.push(Prev::Line(*a, *b)),
        Shape::Arc(a) if a.full => v.push(Prev::Circle(a.c, a.r)),
        Shape::Arc(a) => v.push(Prev::Arc(a.c, a.r, a.start + a.tilt, a.sweep)),
        Shape::Point(pt) => v.push(Prev::Mark(pt.p)),
        Shape::Text(t) => v.push(Prev::Line(t.start, t.end)),
        _ => {}
    }
}

/// 矩形的基准点位置：光标在基准点的哪一侧，矩形就往哪一侧长
fn anchor(base: P, cur: P) -> (i32, i32) {
    (if cur.x >= base.x { -1 } else { 1 }, if cur.y >= base.y { -1 } else { 1 })
}

/// 複線「連続」：下一次往同一方向偏的方向点
fn next_side(ed: &Editor, i: usize, j: usize, side: P) -> P {
    match (ed.curve(i), ed.curve(j)) {
        (Some(Curve::Seg(a, _)), Some(Curve::Seg(c, _))) => add(side, sub(c, a)),
        (Some(Curve::Circ { c, r: r1, .. }), Some(Curve::Circ { r: r2, .. })) => {
            if r2 > r1 {
                add(c, p(r2 * 4.0 + 1.0, 0.0))
            } else {
                c
            }
        }
        _ => side,
    }
}

/// 无限长线・整圆到 q 的距离
fn c_dist_ext(c: &Curve, q: P) -> f64 {
    match *c {
        Curve::Seg(a, b) => dist(q, project(a, b, q)),
        Curve::Circ { c, r, .. } => (dist(q, c) - r).abs(),
    }
}

/// 複線の連結：元の線 pi と i がつながっていれば、その複線 pj と j をコーナー処理
fn connect(ed: &mut Editor, pi: usize, pj: usize, i: usize, j: usize) {
    let (Some(o1), Some(o2)) = (ed.curve(pi), ed.curve(i)) else { return };
    if o1.meet(&o2, false).is_empty() {
        return;
    }
    let (Some(Curve::Seg(a1, b1)), Some(Curve::Seg(a2, b2))) = (ed.curve(pj), ed.curve(j)) else { return };
    let Some(x) = line_line(a1, b1, a2, b2) else { return };
    let far = |a: P, b: P| if dist(a, x) > dist(b, x) { a } else { b };
    ed.corner(pj, far(a1, b1), j, far(a2, b2));
}

/// Jw_cad の命令号 → 本程序的命令
pub fn kind_of(n: u32) -> Option<Kind> {
    Some(match n {
        1 => Kind::Auto,
        2 | 10 => Kind::Line,
        3 => Kind::Rect,
        4 => Kind::Circle,
        5 => Kind::Text,
        6 => Kind::Point,
        7 => Kind::Dim,
        8 => Kind::TwoLine,
        9 => Kind::Center,
        11 => Kind::Offset,
        12 => Kind::Corner,
        13 => Kind::Stretch,
        14 => Kind::Chamfer,
        15 => Kind::Erase,
        16 => Kind::Copy,
        17 => Kind::Move,
        18 => Kind::Tangent,
        19 => Kind::TanCircle,
        23 => Kind::Polygon,
        26 => Kind::Divide,
        31 => Kind::Measure,
        35 => Kind::Hatch,
        36 => Kind::Tidy,
        33 => Kind::Range,
        34 => Kind::Paste,
        75 => Kind::ClipCopy,
        27 => Kind::Figure,
        24 => Kind::Spline,
        25 => Kind::Envelope,
        28 => Kind::Symbol,
        29 => Kind::Param,
        42 => Kind::ChangeAttr,
        43 => Kind::Solid,
        78 => Kind::TakeAttr,
        _ => return None,
    })
}

/// 命令号 → 按钮的名字（时钟菜单的提示用）
pub fn cmd_label(n: i32) -> Option<&'static str> {
    let k = kind_of(u32::try_from(n).ok()?)?;
    PALETTE.iter().find(|x| x.0 == k).map(|x| x.1)
}
