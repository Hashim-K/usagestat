#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
lib_dir="${HOME}/.local/lib/usagestat-dev"
data_dir="${HOME}/.local/share/usagestat-dev"

cd "$repo_root"

cargo build --release --locked -p usagestat-cli -p usagestat-daemon

mkdir -p "$bin_dir" "$lib_dir" "$data_dir"
install -m 0755 target/release/usagestat "$lib_dir/usagestat-dev"
install -m 0755 target/release/usagestatd "$lib_dir/usagestatd-dev"

rm -rf "$data_dir/plugins"
cp -a plugins "$data_dir/plugins"

cat > "$bin_dir/usagestat-dev" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
export USAGESTAT_PLUGIN_DIR="${USAGESTAT_PLUGIN_DIR:-$HOME/.local/share/usagestat-dev/plugins}"
exec "$HOME/.local/lib/usagestat-dev/usagestat-dev" "$@"
EOF
chmod 755 "$bin_dir/usagestat-dev"

if [[ -e "$bin_dir/usagestat" ]]; then
  echo "warning: $bin_dir/usagestat exists and may shadow published packages" >&2
fi

"$bin_dir/usagestat-dev" --version
