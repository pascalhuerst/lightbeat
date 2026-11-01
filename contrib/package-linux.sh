#!/usr/bin/env bash
# Make the Linux tarball of a release build.
#   contrib/package-linux.sh <binary> <version> <output dir>
set -euo pipefail

binary=$1
version=$2
out=$3
root=$(cd "$(dirname "$0")/.." && pwd)
name="lightbeat-${version}-linux-x86_64"
stage=$(mktemp -d)/"$name"

mkdir -p "$stage/contrib" "$out"
install -m 755 "$binary" "$stage/lightbeat"
cp "$root/LICENSE" "$stage/"
cp "$root/contrib/70-lightbeat-x1.rules" "$stage/contrib/"
cp "$root/assets/icon-256.png" "$stage/lightbeat.png"
cat > "$stage/README.txt" <<TXT
LightBeat ${version}

Run ./lightbeat from any directory. Your settings, setup and projects are kept
in ~/.config/lightbeat (or \$XDG_CONFIG_HOME/lightbeat); set LIGHTBEAT_HOME to
use another directory.

Needs the ALSA and udev libraries (libasound2, libudev1), which most desktops
have. JACK is optional and found at run time.

Traktor Kontrol X1: copy contrib/70-lightbeat-x1.rules to /etc/udev/rules.d/,
run "sudo udevadm control --reload && sudo udevadm trigger" and replug it.
TXT

tar -C "$(dirname "$stage")" -czf "$out/$name.tar.gz" "$name"
echo "$out/$name.tar.gz"
