#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tools/publish/scripts/local-release-build.sh [version]

Build local release artifacts with cross without pushing tags or publishing.
When version is omitted, the workspace package version is read from Cargo.toml.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

version="${1:-}"
if [[ -z "$version" ]]; then
  version="$(
    sed -n '/^\[workspace.package\]/,/^\[/{s/^version = "\(.*\)"/\1/p}' Cargo.toml \
      | head -n1
  )"
fi

if [[ -z "$version" ]]; then
  echo "Could not determine version" >&2
  exit 1
fi

if ! command -v cross >/dev/null 2>&1; then
  cargo install cross --git https://github.com/cross-rs/cross
fi

dist_dir="dist/releases/v${version}"
rm -rf "$dist_dir"
mkdir -p "$dist_dir"

declare -A targets=(
  [x86_64-unknown-linux-gnu]=usagestat-linux-x86_64
  [aarch64-unknown-linux-gnu]=usagestat-linux-aarch64
)

for target in "${!targets[@]}"; do
  artifact="${targets[$target]}"
  package_dir="$(mktemp -d)"

  cross build --release --locked --target "$target" -p usagestat-cli

  cp "target/${target}/release/usagestat" "${dist_dir}/${artifact}"
  chmod 755 "${dist_dir}/${artifact}"

  cp "${dist_dir}/${artifact}" "${package_dir}/usagestat"
  cp LICENSE "${package_dir}/LICENSE"
  tar -C "$package_dir" -czf "${dist_dir}/${artifact}.tar.gz" usagestat LICENSE

  (cd "$dist_dir" && sha256sum "$artifact" > "${artifact}.sha256")
  (cd "$dist_dir" && sha256sum "${artifact}.tar.gz" > "${artifact}.tar.gz.sha256")

  rm -rf "$package_dir"
done

ls -lh "$dist_dir"
