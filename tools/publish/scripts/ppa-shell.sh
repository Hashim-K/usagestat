#!/usr/bin/env bash
set -euo pipefail

tools/publish/scripts/docker-build.sh ubuntu-ppa

cat <<'EOF'
Opening an Ubuntu PPA publishing shell.

GPG is mounted so source packages can be signed. Do not commit anything from
~/.gnupg or generated secret material.
EOF

docker run --rm -it \
  -v "$PWD:/work" \
  -v "$HOME/.gnupg:/root/.gnupg" \
  -w /work \
  usagestat-publish-ubuntu-ppa \
  /bin/bash
