#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
dev_bin="${bin_dir}/usagestat-dev"
plugin_dir="${HOME}/.local/share/usagestat-dev/plugins"

cargo build --release -p usagestat-cli --manifest-path "${repo_root}/Cargo.toml"

mkdir -p "${bin_dir}"
install -m 0755 "${repo_root}/target/release/usagestat" "${dev_bin}"
mkdir -p "${plugin_dir}"
cp -a "${repo_root}/plugins/." "${plugin_dir}/"

echo "Installed ${dev_bin}"
echo "Synced plugins to ${plugin_dir}"
"${dev_bin}" --version
