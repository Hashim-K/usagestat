#!/usr/bin/env bash
set -euo pipefail

repo="${1:-hashimkarim/usagestat}"
spec="${2:-packaging/rpm/usagestat.spec}"

tools/publish/scripts/docker-build.sh fedora-copr

docker run --rm -i \
  -v "$PWD:/work" \
  -v "$HOME/.config/copr:/root/.config/copr" \
  -w /work \
  usagestat-publish-fedora-copr \
  copr-cli build "$repo" --enable-net on "$spec"
