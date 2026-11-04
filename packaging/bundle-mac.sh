#!/bin/sh
# 把 Intel / Apple シリコン两种程序合成一个 JWW Viewer.app，打成 JWW-Viewer-mac.zip（在 macOS 上运行）
set -e
T=${CARGO_TARGET_DIR:-target}
APP="JWW Viewer.app"
rm -rf "$APP" JWW-Viewer-mac.zip
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "$T/x86_64-apple-darwin/release/jwwview" "$T/aarch64-apple-darwin/release/jwwview" \
  -output "$APP/Contents/MacOS/jwwview"
cp packaging/Info.plist "$APP/Contents/Info.plist"
# 图标
ICS=icon.iconset
rm -rf "$ICS" && mkdir "$ICS"
for s in 16 32 128 256 512; do
  sips -z $s $s packaging/icon.png --out "$ICS/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2))
  sips -z $d $d packaging/icon.png --out "$ICS/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICS" -o "$APP/Contents/Resources/AppIcon.icns"
rm -rf "$ICS"
# 没有开发者证书，做"临时签名"。Apple シリコンの Mac は署名なしだと開けない
codesign --force --deep -s - "$APP"
lipo -info "$APP/Contents/MacOS/jwwview"
ditto -c -k --keepParent "$APP" JWW-Viewer-mac.zip
