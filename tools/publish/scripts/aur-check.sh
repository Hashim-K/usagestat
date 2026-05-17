#!/usr/bin/env bash
set -euo pipefail

tools/publish/scripts/docker-build.sh arch-aur

docker run --rm -i \
  -v "$PWD:/work" \
  -w /work/packaging/aur/usagestat-bin \
  usagestat-publish-arch-aur \
  bash -lc 'makepkg --verifysource --syncdeps --noconfirm && makepkg --printsrcinfo && namcap PKGBUILD'
