#!/usr/bin/env bash
# Make the macOS app bundle of a release build and zip it.
#   contrib/package-macos.sh <binary> <version> <build number> <output dir>
#
# The app is signed ad hoc, which is what a binary needs to run on Apple
# Silicon at all, but it is not notarized: the first launch needs right-click,
# Open (or `xattr -dr com.apple.quarantine LightBeat.app`).
set -euo pipefail

binary=$1
version=$2
build=$3
out=$4
root=$(cd "$(dirname "$0")/.." && pwd)
arch=$(uname -m)
work=$(mktemp -d)
app="$work/LightBeat.app"

mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$out"
install -m 755 "$binary" "$app/Contents/MacOS/lightbeat"
sed -e "s/@VERSION@/${version%%-*}/" -e "s/@BUILD@/${build}/" \
    "$root/contrib/macos/Info.plist" > "$app/Contents/Info.plist"

# The icon, from the pre-rendered sizes.
iconset="$work/LightBeat.iconset"
mkdir -p "$iconset"
for pair in "16 icon_16x16" "32 icon_16x16@2x" "32 icon_32x32" "64 icon_32x32@2x" \
            "128 icon_128x128" "256 icon_128x128@2x" "256 icon_256x256" \
            "512 icon_256x256@2x" "512 icon_512x512" "1024 icon_512x512@2x"; do
    set -- $pair
    cp "$root/assets/icon-$1.png" "$iconset/$2.png"
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/LightBeat.icns"

codesign --force --deep --sign - "$app"

zip="$out/LightBeat-${version}-macos-${arch}.zip"
ditto -c -k --keepParent "$app" "$zip"
echo "$zip"
