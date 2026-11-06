#!/usr/bin/env bash
# MSYS2-Pakete für den UxPlay-Empfänger und die GStreamer-Runtime (kein Qt mehr nötig).
set -euo pipefail
pacman -S --needed --noconfirm \
  mingw-w64-ucrt-x86_64-{gcc,cmake,ninja,pkgconf,libplist,openssl} \
  mingw-w64-ucrt-x86_64-{gstreamer,gst-plugins-base,gst-plugins-good,gst-plugins-bad,gst-libav}
