#!/usr/bin/env bash
# Builds CaptainFM-<version>-x86_64.AppImage from the dist binary (docs/RELEASING.md, section 3).
#
#   scripts/appimage.sh [OUTDIR]      OUTDIR defaults to dist-release/v<version>/
#
# Needs target/dist/captainfm (./build.sh, or cargo build --profile dist), the tools in ~/Work/CaptainFM-tools
# (linuxdeploy, linuxdeploy-plugin-qt, appimagetool; their SHA256SUMS are checked), qmake6 and jxrlib (the KDE
# image plugins need it). Build in a scratch folder; nothing is added to the git tree.
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
TOOLS=${TOOLS:-$HOME/Work/CaptainFM-tools}
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
OUT=${1:-$ROOT/dist-release/v$VERSION}
BIN=$ROOT/target/dist/captainfm
[ -x "$BIN" ] || { echo "appimage.sh: $BIN is missing; build it first (./build.sh)" >&2; exit 1; }
(cd "$TOOLS" && sha256sum -c SHA256SUMS >/dev/null) || { echo "appimage.sh: tool checksums do not match" >&2; exit 1; }

# Every KDE image plugin must find its libraries, or linuxdeploy stops on the first one that does not.
missing=$(for f in /usr/lib/qt6/plugins/imageformats/kimg_*.so; do ldd "$f" | grep 'not found' || true; done)
[ -z "$missing" ] || { echo "appimage.sh: missing libraries for the image plugins (install jxrlib?):" >&2; echo "$missing" >&2; exit 1; }

WORK=$(mktemp -d "${TMPDIR:-/tmp}/captainfm-appimage.XXXXXX")
mkdir -p "$WORK/AppDir/usr/bin" "$OUT"
cp "$BIN" "$WORK/AppDir/usr/bin/captainfm"
ln -s captainfm "$WORK/AppDir/usr/bin/cfm"

export APPIMAGE_EXTRACT_AND_RUN=1 NO_STRIP=1 ARCH=x86_64 QMAKE=${QMAKE:-/usr/bin/qmake6}
export QML_SOURCES_PATHS=$ROOT/qml PATH=$TOOLS:$PATH
# Wayland is what this app is for, and offscreen is what the headless check below runs on; the plugin only adds xcb.
export EXTRA_PLATFORM_PLUGINS="libqwayland.so;libqoffscreen.so"
export EXTRA_QT_PLUGINS="wayland-decoration-client;wayland-graphics-integration-client;wayland-shell-integration;multimedia;iconengines"

cd "$WORK"
"$TOOLS/linuxdeploy-x86_64.AppImage" --appdir AppDir --plugin qt --executable AppDir/usr/bin/captainfm \
  --desktop-file "$ROOT/packaging/arch/captainfm.desktop" --icon-file "$ROOT/packaging/captainfm.svg" >"$WORK/linuxdeploy.log" 2>&1 \
  || { echo "appimage.sh: linuxdeploy failed, see $WORK/linuxdeploy.log" >&2; exit 1; }

# The plugin does not copy these plugin folders (without them Qt cannot open a Wayland window).
for d in wayland-shell-integration wayland-graphics-integration-client wayland-decoration-client; do
  [ -d AppDir/usr/plugins/$d ] || cp -r "/usr/lib/qt6/plugins/$d" AppDir/usr/plugins/
done

# linuxdeploy's AppRun starts the binary through a symlink, which makes Qt look for its platform plugins in AppDir/
# instead of next to the real binary, so it finds none ("Could not find the Qt platform plugin"). Start the real one.
cat > AppDir/AppRun <<'EOF'
#!/usr/bin/env bash
set -e
this_dir="$(readlink -f "$(dirname "$0")")"
source "$this_dir"/apprun-hooks/"linuxdeploy-plugin-qt-hook.sh"
export QT_PLUGIN_PATH="$this_dir/usr/plugins${QT_PLUGIN_PATH:+:$QT_PLUGIN_PATH}"
export QML_IMPORT_PATH="$this_dir/usr/qml${QML_IMPORT_PATH:+:$QML_IMPORT_PATH}"
export QML2_IMPORT_PATH="$QML_IMPORT_PATH"
exec "$this_dir/usr/bin/captainfm" "$@"
EOF
chmod +x AppDir/AppRun

IMAGE=$OUT/CaptainFM-$VERSION-x86_64.AppImage
"$TOOLS/appimagetool-x86_64.AppImage" AppDir "$IMAGE" >"$WORK/appimagetool.log" 2>&1 \
  || { echo "appimage.sh: appimagetool failed, see $WORK/appimagetool.log" >&2; exit 1; }
chmod +x "$IMAGE"

# Check 1 and 2 of the release steps: the version prints, and a headless run draws a folder and saves a picture.
"$IMAGE" --version
SHOT=$WORK/check.png
HOME=$WORK/home QT_QPA_PLATFORM=offscreen CFM_SHOT=$SHOT CFM_KEYS="<F9> <F9>" CFM_SIZE=900x500 timeout 90 "$IMAGE" /usr/share >/dev/null 2>&1 || true
[ -s "$SHOT" ] || { echo "appimage.sh: the headless run drew nothing; do not publish $IMAGE" >&2; exit 1; }
echo "ok: $IMAGE ($(du -h "$IMAGE" | cut -f1)); checked: --version and a headless screenshot ($SHOT)"
echo "still to check by hand: a different distro (glibc!), a live Wayland window"
