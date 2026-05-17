#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: tools/publish/scripts/docker-run.sh <arch-aur|fedora-copr|ubuntu-ppa> [command...]

Environment:
  MOUNT_SSH=1    Mount ~/.ssh into the container.
  MOUNT_GNUPG=1  Mount ~/.gnupg into the container.
  MOUNT_COPR=1   Mount ~/.config/copr into the container.
EOF
}

if [[ $# -lt 1 ]]; then
  usage >&2
  exit 2
fi

name="$1"
shift

case "$name" in
  arch-aur|fedora-copr|ubuntu-ppa) ;;
  *)
    usage >&2
    exit 2
    ;;
esac

args=(
  --rm
  -v "$PWD:/work"
  -w /work
)

if [[ -t 0 ]]; then
  args+=(-it)
else
  args+=(-i)
fi

if [[ "${MOUNT_SSH:-0}" == "1" ]]; then
  if [[ "$name" == "arch-aur" ]]; then
    args+=(-v "$HOME/.ssh:/home/builder/.ssh")
  else
    args+=(-v "$HOME/.ssh:/root/.ssh")
  fi
fi

if [[ "${MOUNT_GNUPG:-0}" == "1" ]]; then
  args+=(-v "$HOME/.gnupg:/root/.gnupg")
fi

if [[ "${MOUNT_COPR:-0}" == "1" ]]; then
  args+=(-v "$HOME/.config/copr:/root/.config/copr")
fi

if [[ $# -eq 0 ]]; then
  set -- /bin/bash
fi

docker run "${args[@]}" "usagestat-publish-${name}" "$@"
