#!/bin/sh
# 网页版：编译成 wasm，打包成一个独立的 HTML（双击就能用，不需要服务器）
# 需要：rustup target add wasm32-unknown-unknown、cargo install wasm-bindgen-cli（版本和 Cargo.lock 一致）
set -e
cd "$(dirname "$0")/.."
T=${CARGO_TARGET_DIR:-target}
cargo build --release --target wasm32-unknown-unknown --bin jwwview
wasm-bindgen --target web --no-typescript --out-dir "$T/web" "$T/wasm32-unknown-unknown/release/jwwview.wasm"
python3 web/pack.py "$T/web/jwwview.js" "$T/web/jwwview_bg.wasm" web/template.html "$T/web/jww-web.html"
ls -la "$T/web/jww-web.html"
