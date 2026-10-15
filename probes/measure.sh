#!/usr/bin/env bash
# Three toolkit floors on the same geometry: one empty layer surface per monitor,
# anchored across the top, 50 px tall, nothing drawn, no exclusive zone.
#
# The surfaces are invisible but they do sit on the top layer, so clicks in the
# top 50 px go to them while this runs. It takes about half a minute.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

GL_ENV=(
  __NV_DISABLE_EXPLICIT_SYNC=1
  __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/10_nvidia.json
)

report() {
  local name=$1 pid=$2 rss anon clean vram
  rss=$(awk '/^Rss:/{print $2/1024}' "/proc/$pid/smaps_rollup" 2>/dev/null)
  anon=$(awk '/^Anonymous:/{print $2/1024}' "/proc/$pid/smaps_rollup" 2>/dev/null)
  clean=$(awk '/^Private_Clean:/{c=$2} /^Shared_Clean:/{s=$2} END{print (c+s)/1024}' \
    "/proc/$pid/smaps_rollup" 2>/dev/null)
  vram=$(nvidia-smi 2>/dev/null | awk -v p="$pid" '$0 ~ ("[^0-9]" p "[^0-9]") && /MiB/ {print $(NF-1)}' | head -1)
  printf '%-24s rss %7.1f   anon %7.1f   mapped %7.1f   vram %s\n' \
    "$name" "${rss:-0}" "${anon:-0}" "${clean:-0}" "${vram:-none}"
}

probe() {
  local name=$1; shift
  env "${GL_ENV[@]}" "$@" >/dev/null 2>&1 &
  local pid=$!
  sleep 6
  if kill -0 "$pid" 2>/dev/null; then
    report "$name" "$pid"
  else
    printf '%-24s did not start\n' "$name"
  fi
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null
  sleep 1
}

echo "megabytes, one empty 2560x50 layer surface per monitor"
echo
echo "-- with a GL context in the process"
probe "gtk4 gl"        ./gtk-floor
probe "quickshell gl"  qs -p "$PWD/qs-floor.qml"
echo
echo "-- without one"
probe "gtk4 cairo"     env GSK_RENDERER=cairo ./gtk-floor
probe "quickshell sw"  env QT_QUICK_BACKEND=software qs -p "$PWD/qs-floor.qml"
probe "qt6 widgets"    ./qt-floor
probe "qt6 widgets shm" env QT_WAYLAND_CLIENT_BUFFER_INTEGRATION=none ./qt-floor
echo
echo "QPainter always draws into shared memory, but Qt's Wayland plugin loads its"
echo "EGL buffer integration regardless; the shm line is the same binary with that"
echo "integration refused."
