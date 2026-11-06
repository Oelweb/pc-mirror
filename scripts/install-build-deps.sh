#!/usr/bin/env bash
set -euo pipefail
pacman -S --needed --noconfirm \
  mingw-w64-ucrt-x86_64-{gcc,cmake,ninja,pkgconf,libplist,openssl} \
  mingw-w64-ucrt-x86_64-{gstreamer,gst-plugins-base,gst-plugins-good,gst-plugins-bad,gst-libav,qt6-base}
