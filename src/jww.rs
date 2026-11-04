//! .jww 读取。格式依据 Jw_cad 官方说明 https://www.jwcad.net/jwdatafmt.txt
//!
//! 内容是 MFC CArchive 的序列化：没有对齐填充，必须一个字节不差地顺序读下去。
//! 读偏了就立刻报错停下，不返回"半张图"（读偏后坐标会整体错乱，而且不报错）。
//!
//! 坐标、文字尺寸都是**图寸**（纸面上的 mm），原点在纸的中心。
//! 实寸 = 图寸 × 该图层组的缩尺分母。

use std::fmt;

pub const VERSION_MIN: u32 = 300;

#[derive(Debug)]
pub struct JwwError(pub String);

impl fmt::Display for JwwError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for JwwError {}

type R<T> = Result<T, JwwError>;

fn err<T>(msg: impl Into<String>) -> R<T> {
    Err(JwwError(msg.into()))
}

/// CArchive 的读取游标
struct Rd<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> R<&'a [u8]> {
        if self.p + n > self.b.len() {
            return err(format!("超出文件末尾（位置 {}）", self.p));
        }
        let v = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(v)
    }
    fn u8(&mut self) -> R<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> R<i32> {
        Ok(self.u32()? as i32)
    }
    fn f64(&mut self) -> R<f64> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn skip(&mut self, n: usize) -> R<()> {
        self.take(n).map(|_| ())
    }
    fn pt(&mut self) -> R<P> {
        Ok(P { x: self.f64()?, y: self.f64()? })
    }

    /// MFC 的 CString。Unicode 版前面带 0xFF 0xFFFE 标记
    fn cstr(&mut self) -> R<String> {
        let mut wide = false;
        let mut n = self.u8()? as usize;
        if n == 0xFF {
            let mut w = self.u16()?;
            if w == 0xFFFE {
                wide = true;
                n = self.u8()? as usize;
                if n != 0xFF {
                    return self.cstr_body(n, wide);
                }
                w = self.u16()?;
            }
            n = if w == 0xFFFF { self.u32()? as usize } else { w as usize };
        }
        self.cstr_body(n, wide)
    }

    fn cstr_body(&mut self, n: usize, wide: bool) -> R<String> {
        if wide {
            let raw = self.take(n * 2)?;
            let u: Vec<u16> = raw.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
            Ok(String::from_utf16_lossy(&u))
        } else {
            let raw = self.take(n)?;
            Ok(encoding_rs::SHIFT_JIS.decode(raw).0.into_owned())
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct P {
    pub x: f64,
    pub y: f64,
}

/// 所有图形共通的 CData 部分
#[derive(Clone, Debug, Default)]
pub struct Attr {
    /// 曲线属性号
    pub group: u32,
    /// 线种（点是 1 或 100，实体是实体种类，文字是基点位置）
    pub style: u8,
    /// 线色（实体是颜色号，10 = 任意色）
    pub color: u16,
    /// 线宽（文字里是"寸法值设定标志"）。Ver.3.51 以前没有
    pub width: u16,
    pub layer: u16,
    pub glayer: u16,
    /// 属性标志（图形、寸法、建具、包络……）
    pub flag: u16,
}

#[derive(Clone, Debug)]
pub struct Text {
    pub start: P,
    pub end: P,
    /// 文字种（不含 +10000 斜体、+20000 粗体）
    pub kind: u32,
    pub italic: bool,
    pub bold: bool,
    pub size_x: f64,
    pub size_y: f64,
    pub gap: f64,
    /// 角度（度）
    pub deg: f64,
    pub font: String,
    pub s: String,
}

#[derive(Clone, Debug)]
pub struct Arc {
    pub c: P,
    pub r: f64,
    /// 开始角、圆弧角、倾斜角（弧度）
    pub start: f64,
    pub sweep: f64,
    pub tilt: f64,
    /// 扁平率（纵/横）
    pub flat: f64,
    pub full: bool,
}

#[derive(Clone, Debug)]
pub struct Point {
    pub p: P,
    /// 仮点（只显示，不打印）
    pub temp: bool,
    /// 点代码（矢印・ポイントマーカー）。0 = 普通实点
    pub code: u32,
    pub angle: f64,
    pub scale: f64,
}

#[derive(Clone, Debug)]
pub struct Dim {
    pub line: Box<Ent>,
    pub text: Box<Ent>,
    /// Ver.4.20 以后：SXF 模式、补助线 1/2、矢印点 1/2、基准点 1/2
    pub sxf: Option<DimSxf>,
}

#[derive(Clone, Debug)]
pub struct DimSxf {
    pub mode: u16,
    pub lines: [Box<Ent>; 2],
    pub points: [Box<Ent>; 4],
}

#[derive(Clone, Debug)]
pub enum Shape {
    Line { a: P, b: P },
    Arc(Arc),
    Point(Point),
    Text(Text),
    Dim(Dim),
    /// 4 个顶点，文件里的顺序就是绕一圈的顺序（样例图实测：按官方说明的
    /// 「第1, 第4, 第2, 第3」去连会画成交叉的蝴蝶形，Jw_cad 里是实心方块）。
    /// 线种 101 以上是圆实体，pts 的意思另见 `solid_circle`
    Solid { pts: [P; 4], rgb: Option<u32> },
    Block { base: P, sx: f64, sy: f64, rot: f64, number: u32 },
}

#[derive(Clone, Debug)]
pub struct Ent {
    pub a: Attr,
    pub shape: Shape,
}

#[derive(Clone, Debug)]
pub struct BlockDef {
    pub a: Attr,
    pub number: u32,
    pub referred: bool,
    pub time: i64,
    pub name: String,
    pub ents: Vec<Ent>,
}

#[derive(Clone, Debug, Default)]
pub struct GLayer {
    /// 0 不显示、1 只显示、2 可编辑、3 写入
    pub state: u32,
    pub write_lay: u32,
    /// 缩尺分母
    pub scale: f64,
    pub protect: u32,
    pub lay_state: [u32; 16],
    pub lay_protect: [u32; 16],
    pub lay_name: [String; 16],
    pub name: String,
}

/// 线色 → 画面色・画面线宽・打印色・打印线宽・实点半径
#[derive(Clone, Copy, Debug, Default)]
pub struct Pen {
    /// COLORREF（0x00BBGGRR）
    pub color: u32,
    pub width: u32,
    pub prt_color: u32,
    pub prt_width: u32,
    pub prt_point_r: f64,
}

/// 线种的点阵图案
#[derive(Clone, Copy, Debug, Default)]
pub struct LType {
    pub pattern: u32,
    pub unit_dot: u32,
    pub pitch: u32,
    pub prt_pitch: u32,
    /// 随机线才有：画面振幅、打印振幅
    pub rand_wide: u32,
    pub prt_rand_wide: u32,
}

#[derive(Clone, Debug, Default)]
pub struct UdLType {
    pub name: String,
    pub segments: u32,
    pub pitch: [f64; 10],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MojiKind {
    pub w: f64,
    pub h: f64,
    pub gap: f64,
    pub color: u32,
}

#[derive(Clone, Debug)]
pub struct Image {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct Doc {
    pub version: u32,
    pub memo: String,
    /// 0〜4 A0〜A4，8 2A，9 3A，10 4A，11 5A，12 10m，13 50m，14 100m
    pub paper: u32,
    pub write_glay: u32,
    pub glayers: Vec<GLayer>,
    /// 线描画最大幅（负值 = 线宽以 1/100mm 为单位）
    pub max_draw_wid: i32,
    /// 上次打印的范围原点、打印倍率、90°回转（一位）・基准点位置（十位）
    pub prt_origin: P,
    pub prt_scale: f64,
    pub prt_set: u32,
    /// 下标 = 线色号。0〜9 标准，100〜356 SXF 扩展色（下标 100+n）
    pub pens: Vec<Pen>,
    pub ud_color_names: Vec<String>,
    /// 下标 = 线种号。2〜9 标准，11〜15 随机线，16〜19 倍长线，30〜62 SXF 扩展
    pub ltypes: Vec<LType>,
    pub ud_ltypes: Vec<UdLType>,
    pub draw_screen_point: u32,
    pub draw_prt_point: u32,
    pub bitmap_first: u32,
    pub color_print: u32,
    /// 打印 dpi（Ver.6 以后存在文件里，300 或 600）
    pub prt_dpi: u32,
    pub solid_any_color: u32,
    pub solid_color: u32,
    /// 文字种 1〜10（下标 0 不用）
    pub moji: [MojiKind; 11],
    pub ents: Vec<Ent>,
    pub blocks: Vec<BlockDef>,
    pub images: Vec<Image>,
    /// 从文件说明（memo）之后到图形列表之前的原始字节。存回 .jww 时原样写出，不丢任何设定。
    /// 开头的用纸・写入图层组・图层状态由 `save` 用上面的字段改写
    pub settings_raw: Vec<u8>,
    /// 读入时各图形类的 schema 号（Jw_cad 写的是版本号）
    pub schemas: Vec<(String, u16)>,
    /// 图层名・图层组名在 settings_raw 里的范围（存档时按 lay_name / name 重新写）
    pub names_range: (usize, usize),
    /// settings_raw 里「寸法設定 1〜5」（u32 ×5）的位置
    pub dim_set_at: usize,
}

impl Doc {
    /// 纸的大小（mm，横 × 纵）
    pub fn paper_size(&self) -> (f64, f64) {
        match self.paper {
            0 => (1189.0, 841.0),
            1 => (841.0, 594.0),
            2 => (594.0, 420.0),
            3 => (420.0, 297.0),
            4 => (297.0, 210.0),
            8 => (1682.0, 1189.0),
            9 => (2378.0, 1682.0),
            10 => (3364.0, 2378.0),
            11 => (4756.0, 3364.0),
            12 => (10000.0, 7071.0),
            13 => (50000.0, 35355.0),
            14 => (100000.0, 70711.0),
            _ => (420.0, 297.0),
        }
    }

    pub fn paper_name(&self) -> &'static str {
        match self.paper {
            0 => "A-0", 1 => "A-1", 2 => "A-2", 3 => "A-3", 4 => "A-4",
            8 => "2A", 9 => "3A", 10 => "4A", 11 => "5A",
            12 => "10m", 13 => "50m", 14 => "100m",
            _ => "?",
        }
    }

    pub fn block(&self, number: u32) -> Option<&BlockDef> {
        self.blocks.iter().find(|b| b.number == number)
    }
}

/// CArchive 的读入表：类和对象都按出现顺序登记，从 1 开始编号
struct Load {
    classes: Vec<Option<String>>,
    schemas: Vec<(String, u16)>,
}

impl Load {
    fn new() -> Self {
        Load { classes: vec![None], schemas: Vec::new() }
    }

    /// 读对象前面的类标记，返回类名
    fn klass(&mut self, r: &mut Rd) -> R<String> {
        let at = r.p;
        let tag = r.u16()?;
        let idx = match tag {
            0xFFFF => {
                let schema = r.u16()?;
                let n = r.u16()? as usize;
                let nm = String::from_utf8_lossy(r.take(n)?).into_owned();
                self.schemas.push((nm.clone(), schema));
                self.classes.push(Some(nm.clone()));
                self.classes.push(None); // 紧跟着的对象本身也占一个号
                return Ok(nm);
            }
            0x7FFF => {
                let big = r.u32()?;
                if big & 0x8000_0000 == 0 {
                    return err(format!("图形列表里出现了对象引用（位置 {at}）"));
                }
                (big & 0x7FFF_FFFF) as usize
            }
            t if t & 0x8000 != 0 => (t & 0x7FFF) as usize,
            _ => return err(format!("未知的标记 {tag:04x}（位置 {at}）")),
        };
        match self.classes.get(idx) {
            Some(Some(nm)) => {
                let nm = nm.clone();
                self.classes.push(None);
                Ok(nm)
            }
            _ => err(format!("类引用损坏 {tag:04x}（位置 {at}）")),
        }
    }
}

fn count(r: &mut Rd) -> R<usize> {
    let n = r.u16()?;
    Ok(if n == 0xFFFF { r.u32()? as usize } else { n as usize })
}

fn attr(r: &mut Rd, ver: u32) -> R<Attr> {
    Ok(Attr {
        group: r.u32()?,
        style: r.u8()?,
        color: r.u16()?,
        width: if ver >= 351 { r.u16()? } else { 0 },
        layer: r.u16()?,
        glayer: r.u16()?,
        flag: r.u16()?,
    })
}

fn sen(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    Ok(Ent { a, shape: Shape::Line { a: r.pt()?, b: r.pt()? } })
}

fn ten(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    let p = r.pt()?;
    let temp = r.u32()? != 0;
    let (code, angle, scale) = if a.style == 100 {
        (r.u32()?, r.f64()?, r.f64()?)
    } else {
        (0, 0.0, 1.0)
    };
    Ok(Ent { a, shape: Shape::Point(Point { p, temp, code, angle, scale }) })
}

fn enko(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    let arc = Arc {
        c: r.pt()?,
        r: r.f64()?,
        start: r.f64()?,
        sweep: r.f64()?,
        tilt: r.f64()?,
        flat: r.f64()?,
        full: r.u32()? != 0,
    };
    Ok(Ent { a, shape: Shape::Arc(arc) })
}

fn moji(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    let start = r.pt()?;
    let end = r.pt()?;
    let k = r.u32()?;
    let t = Text {
        start,
        end,
        kind: k % 10000,
        italic: (k / 10000) & 1 != 0,
        bold: (k / 10000) & 2 != 0,
        size_x: r.f64()?,
        size_y: r.f64()?,
        gap: r.f64()?,
        deg: r.f64()?,
        font: r.cstr()?,
        s: r.cstr()?,
    };
    Ok(Ent { a, shape: Shape::Text(t) })
}

fn sunpou(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    // 寸法里的线、文字直接就地写出（不带类标记）
    let line = Box::new(sen(r, ver)?);
    let text = Box::new(moji(r, ver)?);
    let sxf = if ver >= 420 {
        let mode = r.u16()?;
        let l1 = Box::new(sen(r, ver)?);
        let l2 = Box::new(sen(r, ver)?);
        let p = [
            Box::new(ten(r, ver)?),
            Box::new(ten(r, ver)?),
            Box::new(ten(r, ver)?),
            Box::new(ten(r, ver)?),
        ];
        Some(DimSxf { mode, lines: [l1, l2], points: p })
    } else {
        None
    };
    Ok(Ent { a, shape: Shape::Dim(Dim { line, text, sxf }) })
}

fn solid(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    let pts = [r.pt()?, r.pt()?, r.pt()?, r.pt()?];
    let rgb = if a.color == 10 { Some(r.u32()?) } else { None };
    Ok(Ent { a, shape: Shape::Solid { pts, rgb } })
}

fn block(r: &mut Rd, ver: u32) -> R<Ent> {
    let a = attr(r, ver)?;
    Ok(Ent {
        a,
        shape: Shape::Block {
            base: r.pt()?,
            sx: r.f64()?,
            sy: r.f64()?,
            rot: r.f64()?,
            number: r.u32()?,
        },
    })
}

fn data(r: &mut Rd, ver: u32, nm: &str) -> R<Ent> {
    match nm {
        "CDataSen" => sen(r, ver),
        "CDataTen" => ten(r, ver),
        "CDataEnko" => enko(r, ver),
        "CDataMoji" => moji(r, ver),
        "CDataSunpou" => sunpou(r, ver),
        "CDataSolid" => solid(r, ver),
        "CDataBlock" => block(r, ver),
        _ => err(format!("还不认识的图形类 {nm}（位置 {}）", r.p)),
    }
}

fn data_list(r: &mut Rd, ver: u32, load: &mut Load) -> R<Vec<Ent>> {
    let n = count(r)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let nm = load.klass(r)?;
        v.push(data(r, ver, &nm)?);
    }
    Ok(v)
}

fn block_defs(r: &mut Rd, ver: u32, load: &mut Load) -> R<Vec<BlockDef>> {
    let n = count(r)?;
    let mut v = Vec::with_capacity(n);
    for _ in 0..n {
        let nm = load.klass(r)?;
        if nm != "CDataList" {
            return err(format!("块定义列表里出现了 {nm}（位置 {}）", r.p));
        }
        let a = attr(r, ver)?;
        let number = r.u32()?;
        let referred = r.u32()? != 0;
        // CTime：Jw_cad 写的是 32 位（实测，不是新版 MFC 的 64 位）
        let time = r.i32()? as i64;
        let name = r.cstr()?;
        if std::env::var_os("JWW_TRACE").is_some() {
            eprintln!("block #{number} ref={referred} time={time} name={name:?} at {}", r.p);
        }
        let ents = data_list(r, ver, load)?;
        v.push(BlockDef { a, number, referred, time, name, ents });
    }
    Ok(v)
}

/// 设定部分。顺序完全照官方说明
fn settings(r: &mut Rd, ver: u32, d: &mut Doc) -> R<()> {
    d.paper = r.u32()?;
    d.write_glay = r.u32()?;
    for _ in 0..16 {
        let mut g = GLayer {
            state: r.u32()?,
            write_lay: r.u32()?,
            scale: r.f64()?,
            protect: r.u32()?,
            ..Default::default()
        };
        for k in 0..16 {
            g.lay_state[k] = r.u32()?;
            g.lay_protect[k] = r.u32()?;
        }
        d.glayers.push(g);
    }
    r.skip(14 * 4)?; // 预留
    d.dim_set_at = r.p;
    r.skip(5 * 4)?; // 寸法设定 1〜5
    r.skip(4)?; // 预留
    d.max_draw_wid = r.i32()?;
    d.prt_origin = r.pt()?;
    d.prt_scale = r.f64()?;
    d.prt_set = r.u32()?;
    r.skip(4 + 8 * 5)?; // 目盛
    d.names_range.0 = r.p;
    for n in 0..16 {
        for k in 0..16 {
            d.glayers[n].lay_name[k] = r.cstr()?;
        }
    }
    for n in 0..16 {
        d.glayers[n].name = r.cstr()?;
    }
    d.names_range.1 = r.p;
    r.skip(8 * 2 + 4 + 8)?; // 日影
    if ver >= 300 {
        r.skip(8 * 2)?; // 天空图
    }
    r.skip(4)?; // 2.5D 单位
    r.skip(8 * 3)?; // 保存时画面倍率
    r.skip(8 * 3)?; // 范围记忆
    if ver >= 300 {
        r.skip(8 * (8 * 3 + 4))?; // 标记跳转 ×8
        r.skip(8 * 3 + 4 + 8 * 3 + 4)?; // 文字描画状态
    } else {
        r.skip(4 * 8 * 3)?;
    }
    r.skip(8 * 11)?; // 複線间隔 10 + 两侧複線留线
    d.pens = vec![Pen::default(); 357];
    for n in 0..10 {
        d.pens[n].color = r.u32()?;
        d.pens[n].width = r.u32()?;
    }
    for n in 0..10 {
        d.pens[n].prt_color = r.u32()?;
        d.pens[n].prt_width = r.u32()?;
        d.pens[n].prt_point_r = r.f64()?;
    }
    d.ltypes = vec![LType::default(); 63];
    for n in 2..=9 {
        let t = &mut d.ltypes[n];
        t.pattern = r.u32()?;
        t.unit_dot = r.u32()?;
        t.pitch = r.u32()?;
        t.prt_pitch = r.u32()?;
    }
    for n in 11..=15 {
        let t = &mut d.ltypes[n];
        t.pattern = r.u32()?;
        t.rand_wide = r.u32()?;
        t.pitch = r.u32()?;
        t.prt_rand_wide = r.u32()?;
        t.prt_pitch = r.u32()?;
    }
    for n in 16..=19 {
        let t = &mut d.ltypes[n];
        t.pattern = r.u32()?;
        t.unit_dot = r.u32()?;
        t.pitch = r.u32()?;
        t.prt_pitch = r.u32()?;
    }
    d.draw_screen_point = r.u32()?;
    d.draw_prt_point = r.u32()?;
    d.bitmap_first = r.u32()?;
    r.skip(4 * 2)?; // 逆描画・逆检索
    d.color_print = r.u32()?;
    r.skip(4 * 4)?; // 图层顺・色顺・连续・灰色
    let n1 = r.u32()?;
    d.prt_dpi = if ver >= 600 {
        if n1 % 100 / 10 == 1 { 300 } else { 600 }
    } else {
        300
    };
    r.skip(4)?; // 作图时间
    r.skip(4 + 4 * 3 + 8 * 4 + 8)?; // 2.5D 视点
    r.skip(8 + 8 * 2 + 8)?; // 线长・矩形・圆半径的最终值
    d.solid_any_color = r.u32()?;
    d.solid_color = r.u32()?;
    if ver >= 420 {
        for n in 0..=256 {
            d.pens[n + 100].color = r.u32()?;
            d.pens[n + 100].width = r.u32()?;
        }
        for n in 0..=256 {
            d.ud_color_names.push(r.cstr()?);
            d.pens[n + 100].prt_color = r.u32()?;
            d.pens[n + 100].prt_width = r.u32()?;
            d.pens[n + 100].prt_point_r = r.f64()?;
        }
        for n in 0..=32 {
            let t = &mut d.ltypes[n + 30];
            t.pattern = r.u32()?;
            t.unit_dot = r.u32()?;
            t.pitch = r.u32()?;
            t.prt_pitch = r.u32()?;
        }
        for _ in 0..=32 {
            let mut u = UdLType { name: r.cstr()?, segments: r.u32()?, ..Default::default() };
            for j in 0..10 {
                u.pitch[j] = r.f64()?;
            }
            d.ud_ltypes.push(u);
        }
    }
    for i in 1..=10 {
        d.moji[i] = MojiKind { w: r.f64()?, h: r.f64()?, gap: r.f64()?, color: r.u32()? };
    }
    r.skip(8 * 3 + 4 * 2)?; // 写入文字
    r.skip(8 * 2)?; // 文字位置整理
    r.skip(4 + 8 * 3 + 8 * 3)?; // 文字基准点偏移
    Ok(())
}

/// Jw_cad 里占半格的字：Shift-JIS 一个字节的字（ASCII、半角片假名）
pub fn is_half(c: char) -> bool {
    matches!(c, '\u{20}'..='\u{7E}' | '\u{FF61}'..='\u{FF9F}' | '\u{A5}' | '\u{203E}')
}

pub fn read_bytes(b: &[u8]) -> R<Doc> {
    if b.len() < 12 || &b[..8] != b"JwwData." {
        return err("不是 JWW 文件");
    }
    let mut r = Rd { b, p: 8 };
    let version = r.u32()?;
    if version < VERSION_MIN {
        return err(format!("太旧的 JWW（版本 {version}），请用 Jw_cad 重新保存"));
    }
    let mut d = Doc {
        version,
        memo: r.cstr()?,
        paper: 0,
        write_glay: 0,
        glayers: Vec::new(),
        max_draw_wid: 0,
        prt_origin: P::default(),
        prt_scale: 1.0,
        prt_set: 0,
        pens: Vec::new(),
        ud_color_names: Vec::new(),
        ltypes: Vec::new(),
        ud_ltypes: Vec::new(),
        draw_screen_point: 0,
        draw_prt_point: 0,
        bitmap_first: 0,
        color_print: 0,
        prt_dpi: 300,
        solid_any_color: 0,
        solid_color: 0,
        moji: [MojiKind::default(); 11],
        ents: Vec::new(),
        blocks: Vec::new(),
        images: Vec::new(),
        settings_raw: Vec::new(),
        schemas: Vec::new(),
        names_range: (0, 0),
        dim_set_at: 0,
    };
    let s0 = r.p;
    settings(&mut r, version, &mut d)?;
    d.settings_raw = b[s0..r.p].to_vec();
    d.names_range = (d.names_range.0 - s0, d.names_range.1 - s0);
    d.dim_set_at -= s0;

    // 确认落在了图形列表的开头：个数之后第一个必定是新类标记 0xFFFF
    let at = r.p;
    let peek = {
        let mut q = Rd { b, p: r.p };
        let n = count(&mut q)?;
        (n, q.p)
    };
    if peek.0 > 0 && b.get(peek.1..peek.1 + 2) != Some(&[0xFF, 0xFF][..]) {
        return err(format!("找不到图形列表（位置 {at}）。可能是没见过的 Jw_cad 版本"));
    }

    let mut load = Load::new();
    d.ents = data_list(&mut r, version, &mut load)?;
    if std::env::var_os("JWW_TRACE").is_some() {
        eprintln!("v{version} ents={} end at {} / {}", d.ents.len(), r.p, b.len());
    }
    d.blocks = block_defs(&mut r, version, &mut load)?;
    d.schemas = load.schemas;
    if version >= 700 && r.p < b.len() {
        let n = r.u32()?;
        for _ in 0..n {
            let name = r.cstr()?;
            let size = r.u32()? as usize;
            let data = r.take(size)?.to_vec();
            d.images.push(Image { name, data });
        }
    }
    if r.p != b.len() {
        return err(format!("文件末尾还剩 {} 字节没读", b.len() - r.p));
    }
    Ok(d)
}

pub fn read(path: &std::path::Path) -> R<Doc> {
    let b = std::fs::read(path).map_err(|e| JwwError(format!("打不开 {}: {e}", path.display())))?;
    read_bytes(&b)
}

/// 図形ファイル（.jws）：Jw_cad の部品。官方没有公开格式，按实际文件推出来的：
/// "JwsData." 后面用 '.' 填到 200 字节 → 版本号 → 基准点 → 16 个图层组缩尺 → （文字等设定）
/// → 包围盒两点 → 图形列表（和 .jww 一样）→ ブロック定義（新版本才有）
#[derive(Clone, Debug)]
pub struct Jws {
    pub version: u32,
    /// 基准点（图寸）
    pub base: P,
    /// 各图层组的缩尺（图形按 实寸 = 图寸 × 缩尺 记）
    pub scales: [f64; 16],
    pub ents: Vec<Ent>,
    pub blocks: Vec<BlockDef>,
    /// 图形列表之前的原始字节（存 .jws 时当模板）、列表之后的字节
    pub head: Vec<u8>,
    pub tail: Vec<u8>,
    pub schemas: Vec<(String, u16)>,
}

pub fn read_jws_bytes(b: &[u8]) -> R<Jws> {
    if b.len() < 0xD0 || &b[..8] != b"JwsData." {
        return err("不是 JWS（図形）文件");
    }
    let mut r = Rd { b, p: 0xC8 };
    let version = r.u32()?;
    let base = r.pt()?;
    let mut scales = [1.0; 16];
    for s in scales.iter_mut() {
        *s = r.f64()?;
    }
    // 后面的设定长短随版本变：找第一个类标记「0xFFFF + 版本号 + 类名长 + "CData"」，
    // 它前面就是图形个数（WORD，0xFFFF 时后跟 DWORD）
    let start = r.p;
    let hit = (start..b.len().saturating_sub(11)).find(|&i| {
        b[i] == 0xFF && b[i + 1] == 0xFF && &b[i + 6..i + 11] == b"CData"
    });
    let Some(h) = hit else { return err("JWS の図形が見つかりません") };
    let list_at = if h >= start + 6 && b[h - 6] == 0xFF && b[h - 5] == 0xFF {
        h - 6
    } else if h >= start + 2 {
        h - 2
    } else {
        return err("JWS の図形一覧の位置がおかしい");
    };
    let mut r = Rd { b, p: list_at };
    let mut load = Load::new();
    let ents = data_list(&mut r, version, &mut load)?;
    let list_end = r.p;
    let blocks = if r.p < b.len() { block_defs(&mut r, version, &mut load).unwrap_or_default() } else { vec![] };
    Ok(Jws {
        version,
        base,
        scales,
        ents,
        blocks,
        head: b[..list_at].to_vec(),
        tail: b[list_end..].to_vec(),
        schemas: load.schemas,
    })
}

pub fn read_jws(path: &std::path::Path) -> R<Jws> {
    let b = std::fs::read(path).map_err(|e| JwwError(format!("打不开 {}: {e}", path.display())))?;
    read_jws_bytes(&b)
}
