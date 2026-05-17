#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tools/publish/scripts/docker-build.sh <arch-aur|fedora-copr|ubuntu-ppa>
EOF
}

if [[ $# -ne 1 ]]; then
  usage >&2
  exit 2
fi

name="$1"
case "$name" in
  arch-aur|fedora-copr|ubuntu-ppa) ;;
  *)
    usage >&2
    exit 2
    ;;
esac

docker build \
  -f "tools/publish/docker/${name}.Dockerfile" \
  -t "usagestat-publish-${name}" \
  .
