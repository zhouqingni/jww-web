//! 直接印刷：PDF（印刷枠・カラー設定は PDF 出力と同じ）を作って、OS の印刷に渡す。
//! Mac・Linux は CUPS（lpstat / lp），Windows は PDF を開く既定アプリの「PrintTo」。

use std::path::Path;
use std::process::Command;

/// 「lpstat -a」の出力からプリンタ名
pub fn parse_lpstat_a(s: &str) -> Vec<String> {
    s.lines().filter_map(|l| l.split_whitespace().next()).map(|x| x.to_string()).collect()
}

/// 「lpstat -d」の出力から既定プリンタ（英語・日本語どちらの表示でも、最後の「:」の後）
pub fn parse_lpstat_d(s: &str) -> Option<String> {
    let l = s.lines().next()?;
    let (_, name) = l.rsplit_once(|c| c == ':' || c == '：')?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// プリンタ一覧と既定プリンタ
pub fn printers() -> (Vec<String>, Option<String>) {
    #[cfg(not(windows))]
    {
        let out = |args: &[&str]| {
            Command::new("lpstat").args(args).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
        };
        (parse_lpstat_a(&out(&["-a"])), parse_lpstat_d(&out(&["-d"])))
    }
    #[cfg(windows)]
    {
        let ps = |cmd: &str| {
            Command::new("powershell")
                .args(["-NoProfile", "-Command", cmd])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default()
        };
        let names: Vec<String> = ps("Get-Printer | ForEach-Object { $_.Name }").lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
        let def = ps("(Get-CimInstance Win32_Printer -Filter 'Default=True').Name").trim().to_string();
        (names, (!def.is_empty()).then_some(def))
    }
}

/// PDF を印刷。media は用紙名（A3 など）
pub fn print_pdf(pdf: &Path, printer: Option<&str>, copies: u32, media: &str) -> Result<(), String> {
    let copies = copies.max(1);
    #[cfg(not(windows))]
    {
        let mut c = Command::new("lp");
        if let Some(p) = printer {
            c.args(["-d", p]);
        }
        c.args(["-n", &copies.to_string(), "-o", &format!("media={media}")]);
        c.arg(pdf);
        let o = c.output().map_err(|e| format!("lp を実行できません：{e}"))?;
        if o.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&o.stderr).trim().to_string())
        }
    }
    #[cfg(windows)]
    {
        let _ = media;
        let path = pdf.display().to_string().replace('\'', "''");
        let cmd = match printer {
            Some(p) => format!("Start-Process -FilePath '{path}' -Verb PrintTo -ArgumentList '\"{}\"'", p.replace('\'', "''")),
            None => format!("Start-Process -FilePath '{path}' -Verb Print"),
        };
        for _ in 0..copies {
            let o = Command::new("powershell").args(["-NoProfile", "-Command", &cmd]).output().map_err(|e| e.to_string())?;
            if !o.status.success() {
                return Err(String::from_utf8_lossy(&o.stderr).trim().to_string());
            }
        }
        Ok(())
    }
}
