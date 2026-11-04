"""把 wasm-bindgen 的 JS 和 wasm 塞进一个 HTML。用法：pack.py glue.js app.wasm template.html out.html"""
import base64, sys

js, wasm, tpl, out = sys.argv[1:5]
glue = open(js, encoding="utf-8").read()
b64 = base64.b64encode(open(wasm, "rb").read()).decode()
boot = f"""
{glue}
const b64 = "{b64}";
const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
__wbg_init({{ module_or_path: bin }});
"""
html = open(tpl, encoding="utf-8").read().replace("/*APP*/", boot)
open(out, "w", encoding="utf-8").write(html)
