#!/usr/bin/env bash
# Builds the two compiled floor probes next to this script.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

gcc -O2 -o gtk-floor gtk-floor.c $(pkg-config --cflags --libs gtk4 gtk4-layer-shell-0)

g++ -O2 -fPIC -o qt-floor qt-floor.cpp \
  $(pkg-config --cflags --libs Qt6Widgets Qt6Gui Qt6Core) \
  -I/usr/include/LayerShellQt -lLayerShellQtInterface

echo "built gtk-floor and qt-floor"
