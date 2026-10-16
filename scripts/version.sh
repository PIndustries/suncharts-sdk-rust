#!/usr/bin/env bash
# P.Industries SDK versioning: YY.MM.DD.<build-of-day> (UTC date, no leading zeros, e.g. 26.10.16.1).
# The canonical version lives in ./VERSION and is the git tag, exactly (no "v" prefix).
# Ecosystems that need 3-part SemVer (Cargo, npm) get YY.(MM*100+DD).BUILD, e.g. 26.10.16.1 -> 26.1016.1,
# 27.1.5.2 -> 27.105.2 (ordering preserved). Composer and PEP 440 accept the 4-part form unchanged.
#   scripts/version.sh current        # contents of VERSION
#   scripts/version.sh next           # next YY.MM.DD.N for today (N = 1 + highest N tagged today)
#   scripts/version.sh semver [ver]   # 3-part SemVer mapping
#   scripts/version.sh set <ver>      # write VERSION and every manifest / user-agent string
set -euo pipefail
cd "$(dirname "$0")/.."
semver() { awk -F. '{ if (NF != 4) exit 1; printf "%d.%d.%d\n", $1, $2 * 100 + $3, $4 }' <<<"$1"; }
case "${1:-current}" in
  current) cat VERSION ;;
  next)
    today=$(TZ=UTC date +%y.%m.%d | awk -F. '{printf "%d.%d.%d", $1, $2, $3}')
    tags=$( (git ls-remote --tags origin 2>/dev/null | sed 's#.*refs/tags/##; s#\^{}##'; git tag) | sort -u)
    n=$(grep -E "^${today//./\\.}\.[0-9]+$" <<<"$tags" | awk -F. '{print $4}' | sort -n | tail -1 || true)
    echo "$today.$(( ${n:-0} + 1 ))" ;;
  semver) semver "${2:-$(cat VERSION)}" ;;
  set)
    v=${2:?usage: version.sh set YY.MM.DD.N}
    [[ $v =~ ^[0-9]{1,2}\.[0-9]{1,2}\.[0-9]{1,2}\.[0-9]+$ ]] || { echo "bad version $v" >&2; exit 2; }
    s=$(semver "$v")
    echo "$v" > VERSION
    sub() { [ -f "$1" ] && perl -0pi -e "$2" "$1" || true; }
    # Rust (Cargo.toml + Cargo.lock entry of this crate)
    sub Cargo.toml 's/^version = "[^"]*"/version = "'"$s"'"/m'
    sub Cargo.lock 's/(name = "suncharts-sdk"\nversion = )"[^"]*"/$1"'"$s"'"/'
    # npm (package.json + package-lock.json root entries) and the TS user agent
    sub package.json 's/^  "version": "[^"]*"/  "version": "'"$s"'"/m'
    sub package-lock.json 's/^  "version": "[^"]*"/  "version": "'"$s"'"/m; s/("packages": \{\n    "": \{\n      "name": "[^"]*",\n      "version": )"[^"]*"/$1"'"$s"'"/'
    sub src/client.ts 's#suncharts-sdk-typescript/[0-9.]+#suncharts-sdk-typescript/'"$v"'#'
    # Python (PEP 440 accepts the 4-part form)
    sub pyproject.toml 's/^version = "[^"]*"/version = "'"$v"'"/m'
    sub src/suncharts/__init__.py 's/__version__ = "[^"]*"/__version__ = "'"$v"'"/'
    sub src/suncharts/client.py 's#suncharts-sdk-python/[0-9.]+#suncharts-sdk-python/'"$v"'#'
    # PHP (Composer takes the version from the git tag; only the user agent is pinned)
    sub src/Client.php 's#suncharts-sdk-php/[0-9.]+#suncharts-sdk-php/'"$v"'#g'
    echo "version $v (semver mapping $s)" ;;
  *) echo "usage: $0 current|next|semver [ver]|set <ver>" >&2; exit 2 ;;
esac
