//! 批量读 .jws，报告读不了的，并检查「读 → 写」是否一字不差（没有ブロック的文件）：
//! cargo run --example jws_scan -- <文件夹>
fn walk(p: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            let q = e.path();
            if q.is_dir() {
                walk(&q, out);
            } else if q.extension().map(|x| x.eq_ignore_ascii_case("jws")).unwrap_or(false) {
                out.push(q);
            }
        }
    }
}
fn main() {
    let mut v = Vec::new();
    for a in std::env::args().skip(1) {
        walk(std::path::Path::new(&a), &mut v);
    }
    let (mut ok, mut bad, mut same, mut diff, mut withblk, mut nbbox) = (0, 0, 0, 0, 0, 0);
    let mut vers = std::collections::BTreeMap::new();
    for p in &v {
        let b = std::fs::read(p).unwrap();
        match jwwview::jww::read_jws_bytes(&b) {
            Ok(j) => {
                *vers.entry(j.version).or_insert(0) += 1;
                ok += 1;
                if !j.blocks.is_empty() {
                    withblk += 1;
                    continue;
                }
                let mut w = jwwview::jwwsave::write_jws(&j, &j.ents, j.base, &j.scales);
                // 文件头的包围盒是 Jw_cad 算的外框，单独比
                let bb = j.head.len() - 32;
                let near = |x: &[u8], y: &[u8]| {
                    (0..4).all(|k| {
                        let f = |z: &[u8]| f64::from_le_bytes(z[8 * k..8 * k + 8].try_into().unwrap());
                        (f(x) - f(y)).abs() < 0.01
                    })
                };
                let bbox_same = w.len() == b.len() && near(&w[bb..bb + 32], &b[bb..bb + 32]);
                if w.len() == b.len() {
                    if !bbox_same {
                        nbbox += 1;
                    }
                    w[bb..bb + 32].copy_from_slice(&b[bb..bb + 32]);
                }
                if w == b {
                    same += 1
                } else {
                    diff += 1;
                    if diff <= 5 {
                        let at = w.iter().zip(&b).position(|(x, y)| x != y).unwrap_or(w.len().min(b.len()));
                        println!("DIFF {} at {at} (len {} vs {})", p.display(), w.len(), b.len());
                    }
                }
            }
            Err(e) => {
                bad += 1;
                if bad <= 3 {
                    println!("NG {}: {}", p.display(), e.0);
                }
            }
        }
    }
    println!("total {} read {} bad {} | roundtrip same {} diff {} (with blocks {}; bbox 不同 {}) versions {:?}", v.len(), ok, bad, same, diff, withblk, nbbox, vers);
}
