#!/usr/bin/env bash
# Build only the trusted Action source. Never build the consumer's CLI.
set -euo pipefail
export RUSTUP_TOOLCHAIN=${INPUT_RUST_TOOLCHAIN:-stable}
source_dir=$(cd -- "${ACTION_PATH:?Action path is required}/.." && pwd)
: "${GITHUB_OUTPUT:?Output file is required}" "${RUNNER_TEMP:?Runner temporary directory is required}"
fingerprint=$(
  cd "$source_dir"
  { find Cargo.toml Cargo.lock src -type f -print0 | LC_ALL=C sort -z | xargs -0 sha256sum
    printf '%s' "${INPUT_RUST_TOOLCHAIN:-stable}"
  } | sha256sum | cut -d ' ' -f 1
)
directory="$RUNNER_TEMP/changenote-cli/$fingerprint"
version=$(awk -F '"' '/^version = "/ { print $2; exit }' "$source_dir/Cargo.toml")
case ${1:?Expected key, install, or activate} in
  key)
    printf 'directory=%s\nkey=changenote-%s-%s-%s\n' "$directory" "$(uname -s)" "$(uname -m)" "$fingerprint" >> "$GITHUB_OUTPUT"
    ;;
  install)
    cargo install --path "$source_dir" --locked --root "$directory"
    ;;
  activate)
    [[ -f $directory/bin/changenote && ! -L $directory/bin/changenote ]] || { echo 'Cached CLI must be a regular file.' >&2; exit 1; }
    [[ $("$directory/bin/changenote" --version) == "changenote $version" ]] || { echo 'Cached CLI version differs from the Action source.' >&2; exit 1; }
    printf '%s\n' "$directory/bin" >> "${GITHUB_PATH:?Path file is required}"
    ;;
  *) echo 'Unknown CLI setup operation.' >&2; exit 1 ;;
esac
