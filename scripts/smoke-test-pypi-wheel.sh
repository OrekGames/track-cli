#!/usr/bin/env bash
# Install orekgames-track from a directory of wheels the way a user would,
# with pip and with uv, and check the installed `track` runs.
#
# Usage: ./scripts/smoke-test-pypi-wheel.sh <wheels-dir> <version>
#
# The package is installed by name with the index switched off, so the
# installer has to pick the wheel whose platform tag fits this machine: a
# wrong tag fails here instead of on a user's system. Runs on Linux, macOS
# and Windows (Git Bash).
set -euo pipefail

if [[ $# -ne 2 ]]; then
    echo "Usage: $0 <wheels-dir> <version>" >&2
    exit 1
fi

WHEELS="$1"
VERSION="$2"
REQUIREMENT="orekgames-track==${VERSION}"

case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*) PYTHON=python; BIN=Scripts; EXE=.exe ;;
    *) PYTHON=python3; BIN=bin; EXE= ;;
esac

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

check_version() { # $1=installer $2=path to track
    local output
    output="$("$2" --version | tr -d '\r')"
    if [[ "$output" != "track ${VERSION}" ]]; then
        echo "Error: track installed with $1 reported '$output', expected 'track ${VERSION}'" >&2
        exit 1
    fi
    echo "$1: $output"
}

"$PYTHON" -m venv "$WORK/venv"
VENV_PYTHON="$WORK/venv/$BIN/python$EXE"

"$VENV_PYTHON" -m pip install --quiet --no-index --find-links "$WHEELS" "$REQUIREMENT"
check_version pip "$WORK/venv/$BIN/track$EXE"

# uv comes from PyPI; only the package under test is installed offline.
"$VENV_PYTHON" -m pip install --quiet uv
UV_TOOL_DIR="$WORK/uv-tools" UV_TOOL_BIN_DIR="$WORK/uv-bin" \
    "$WORK/venv/$BIN/uv$EXE" tool install --quiet --no-index --find-links "$WHEELS" "$REQUIREMENT"
check_version uv "$WORK/uv-bin/track$EXE"
