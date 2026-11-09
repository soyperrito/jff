#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
NFPM_BIN="${NFPM_BIN:-nfpm}"

if ! command -v "$NFPM_BIN" >/dev/null 2>&1; then
    echo "error: nfpm not found; install it or set NFPM_BIN" >&2
    exit 1
fi

cargo build --release --locked

rm -rf packaging/build dist
mkdir -p packaging/build/themes packaging/build/completions dist

cp target/release/jff packaging/build/jff
chmod 0755 packaging/build/jff

temporary="$(mktemp -d)"
trap 'rm -rf "$temporary"' EXIT
JFF_HOME="$temporary" target/release/jff config init >/dev/null
cp "$temporary/config.json" packaging/build/config.default.json
chmod 0644 packaging/build/config.default.json

cp themes/*.json packaging/build/themes/
chmod 0644 packaging/build/themes/*.json

target/release/jff man > packaging/build/jff.1
chmod 0644 packaging/build/jff.1

target/release/jff completions bash > packaging/build/completions/jff.bash
target/release/jff completions zsh > packaging/build/completions/_jff
target/release/jff completions fish > packaging/build/completions/jff.fish
chmod 0644 packaging/build/completions/*

VERSION="$VERSION" "$NFPM_BIN" package -f packaging/nfpm.yaml -p deb -t dist/
VERSION="$VERSION" "$NFPM_BIN" package -f packaging/nfpm.yaml -p rpm -t dist/

printf '\nGenerated packages for version %s:\n' "$VERSION"
find dist -maxdepth 1 -type f -printf '  %f\n' | sort
