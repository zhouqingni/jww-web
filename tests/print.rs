use jwwview::print::{parse_lpstat_a, parse_lpstat_d};

#[test]
fn lpstat_parse() {
    let a = "Brother_HL_L2375DW accepting requests since Sat Nov  1 10:00:00 2026\nCanon_iR accepting requests since Fri Oct 31 09:00:00 2026\n";
    assert_eq!(parse_lpstat_a(a), vec!["Brother_HL_L2375DW", "Canon_iR"]);
    assert_eq!(parse_lpstat_d("system default destination: Canon_iR\n").as_deref(), Some("Canon_iR"));
    assert_eq!(parse_lpstat_d("システムのデフォルトの送信先: Canon_iR\n").as_deref(), Some("Canon_iR"));
    assert_eq!(parse_lpstat_d("no system default destination\n"), None);
}
