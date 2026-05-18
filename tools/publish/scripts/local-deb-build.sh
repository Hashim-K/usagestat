#!/usr/bin/env bash
set -euo pipefail

version="$(
  sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' Cargo.toml \
    | head -n1
)"

if [[ -z "$version" ]]; then
  echo "Could not determine version" >&2
  exit 1
fi

dist_dir="dist/packages/v${version}"
mkdir -p "$dist_dir"

tools/publish/scripts/docker-build.sh ubuntu-ppa

docker run --rm -i \
  -v "$PWD:/work" \
  -w /work \
  usagestat-publish-ubuntu-ppa \
  bash -lc '
    set -euo pipefail
    export CARGO_TARGET_DIR=/tmp/usagestat-target
    cargo deb -p usagestat-cli --output /tmp/usagestat.deb
    dpkg -i /tmp/usagestat.deb
    usagestat --version
    (cd /tmp && usagestat --json list --provider codex | tee /tmp/usagestat-providers.json)
    grep -q "\"id\": \"codex\"" /tmp/usagestat-providers.json
    usagestat test https
    cp /tmp/usagestat.deb /work/'"$dist_dir"'/usagestat_'"$version"'_amd64.deb
  '

ls -lh "$dist_dir"/*.deb
