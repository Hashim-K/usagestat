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

docker run --rm -i \
  -v "$PWD:/work" \
  -w /work \
  usagestat-publish-fedora-copr \
  bash -lc '
    set -euo pipefail
    git config --global --add safe.directory /work
    topdir=/tmp/usagestat-rpmbuild
    mkdir -p "$topdir"/{BUILD,BUILDROOT,RPMS,SOURCES,SPECS,SRPMS}
    git archive --format=tar.gz --prefix=usagestat-'"$version"'/ \
      -o "$topdir/SOURCES/v'"$version"'.tar.gz" HEAD
    rpmbuild -bb \
      --define "_topdir $topdir" \
      --define "_sourcedir $topdir/SOURCES" \
      packaging/rpm/usagestat.spec
    dnf install -y "$topdir"/RPMS/*/usagestat-'"$version"'-*.rpm
    usagestat --version
    usagestat test https
    cp "$topdir"/RPMS/*/*.rpm /work/'"$dist_dir"'/
  '

ls -lh "$dist_dir"/*.rpm
