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

tools/publish/scripts/docker-build.sh fedora-copr

if [[ "${TRACKED_ONLY:-0}" == "1" ]]; then
  tar_mode="tracked"
else
  tar_mode="worktree"
fi

docker run --rm -i \
  -e "USAGESTAT_VERSION=$version" \
  -e "USAGESTAT_TAR_MODE=$tar_mode" \
  -e "USAGESTAT_DIST_DIR=$dist_dir" \
  -v "$PWD:/work" \
  -w /work \
  usagestat-publish-fedora-copr \
  bash -lc '
    set -euo pipefail
    git config --global --add safe.directory /work
    topdir=/tmp/usagestat-rpmbuild
    mkdir -p "$topdir"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
    if [[ "$USAGESTAT_TAR_MODE" == "tracked" ]]; then
      git archive --format=tar.gz --prefix="usagestat-${USAGESTAT_VERSION}/" \
        -o "$topdir/SOURCES/v${USAGESTAT_VERSION}.tar.gz" HEAD
    else
      src_dir="/tmp/usagestat-src/usagestat-${USAGESTAT_VERSION}"
      mkdir -p "$src_dir"
      tar -C /work \
        --exclude=.git \
        --exclude=target \
        --exclude=dist \
        --exclude=inspo \
        -cf - . \
        | tar -C "$src_dir" --strip-components=1 -xf -
      tar -C /tmp/usagestat-src -czf "$topdir/SOURCES/v${USAGESTAT_VERSION}.tar.gz" "usagestat-${USAGESTAT_VERSION}"
    fi
    rpmbuild -bb \
      --define "_topdir $topdir" \
      --define "_sourcedir $topdir/SOURCES" \
      packaging/rpm/usagestat.spec
    dnf install -y "$topdir"/RPMS/*/usagestat-"${USAGESTAT_VERSION}"-*.rpm
    usagestat --version
    (cd /tmp && usagestat --json list --provider codex | tee /tmp/usagestat-providers.json)
    grep -q "\"id\": \"codex\"" /tmp/usagestat-providers.json
    usagestat test https
    cp "$topdir"/RPMS/*/*.rpm "/work/${USAGESTAT_DIST_DIR}/"
  '

ls -lh "$dist_dir"/*.rpm
