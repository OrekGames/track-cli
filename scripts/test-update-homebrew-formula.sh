#!/usr/bin/env bash
# Regression tests for scripts/update-homebrew-formula.sh
#
# Usage: bash scripts/test-update-homebrew-formula.sh
#
# Uses a local file:// "release" directory as the download source so the
# tests do not touch the network, plus a fake formula fixture.
set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT="$SCRIPT_DIR/update-homebrew-formula.sh"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

PASS=0
FAIL=0

ok() { echo "PASS: $1"; PASS=$((PASS+1)); }
ko() { echo "FAIL: $1"; FAIL=$((FAIL+1)); }

# Build a fresh formula fixture (mirrors homebrew/Formula/track.rb shape: the
# version is carried literally in every url).
make_formula() {
    cat > "$1" <<'EOF'
class Track < Formula
  desc "CLI for issue tracking systems (YouTrack, Jira, GitHub, GitLab)"
  homepage "https://github.com/OrekGames/track-cli"
  license "MIT"
  on_macos do
    on_arm do
      url "https://github.com/OrekGames/track-cli/releases/download/v1.0.0/track-1.0.0-aarch64-apple-darwin.tar.gz"
      sha256 "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    end
    on_intel do
      url "https://github.com/OrekGames/track-cli/releases/download/v1.0.0/track-1.0.0-x86_64-apple-darwin.tar.gz"
      sha256 "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
    end
  end
  on_linux do
    on_arm do
      url "https://github.com/OrekGames/track-cli/releases/download/v1.0.0/track-1.0.0-aarch64-unknown-linux-gnu.tar.gz"
      sha256 "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
    end
    on_intel do
      url "https://github.com/OrekGames/track-cli/releases/download/v1.0.0/track-1.0.0-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    end
  end
end
EOF
}

# Legacy formula shape (explicit `version`, #{version} urls), as published to
# the tap up to 1.18.2. Still read as a --baseline and still renderable.
make_legacy_formula() {
    make_formula "$1"
    sed -i -e 's|^  license "MIT"|  version "1.0.0"\n  license "MIT"|' \
        -e 's|/download/v1\.0\.0/track-1\.0\.0-|/download/v#{version}/track-#{version}-|' "$1"
}

# True if all four urls in formula $1 are at version $2.
has_version() {
    [ "$(grep -c "url \".*/download/v$2/track-$2-.*\.tar\.gz\"" "$1")" -eq 4 ]
}

# Build a local "release" with four archives and a checksums file.
make_release() { # $1=dir $2=version
    local dir="$1" ver="$2" t
    mkdir -p "$dir"
    for t in aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-gnu x86_64-unknown-linux-gnu; do
        echo "payload-$t" > "$dir/track-${ver}-${t}.tar.gz"
    done
    (cd "$dir" && sha256sum track-*.tar.gz > checksums-sha256.txt)
}

run_case() { # $1=name $2=expected_rc $3...=script args + env via env-vars
    local name="$1" want_rc="$2"
    shift 2
    local rc=0
    "$@" >"$WORK/out.log" 2>&1 || rc=$?
    if [ "$rc" -eq "$want_rc" ]; then ok "$name (rc=$rc)"; else ko "$name (rc=$rc, want $want_rc)"; sed 's/^/    /' "$WORK/out.log"; fi
}

# Environment for the script: local file download, temp dirs.
common_env() { # $1=release dir $2=workdir
    echo "TRACK_ARCHIVE_URL_TEMPLATE=file://$1/{FILE} TRACK_DOWNLOAD_DIR=$2/archives TRACK_KEEP_DOWNLOADS=0"
}

sha_of() { sha256sum "$1" | awk '{print $1}'; }

# --- Test 1: normal update downloads, verifies, updates ----------------------
T="$WORK/t1"; mkdir -p "$T"
make_release "$T/release" "1.1.0"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.1.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -eq 0 ] && has_version "$T/formula.rb" 1.1.0 && \
   grep -q "$(grep 'aarch64-apple-darwin' "$T/release/checksums-sha256.txt" | awk '{print $1}')" "$T/formula.rb"; then
    ok "update to new version"
else
    ko "update to new version (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi
# downloads cleaned up
if [ ! -e "$T/archives" ]; then ok "downloads cleaned up"; else ko "downloads cleaned up (dir left behind)"; fi

# --- Test 2: idempotent same-version retry -----------------------------------
BEFORE=$(cat "$T/formula.rb")
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.1.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -eq 0 ] && [ "$BEFORE" = "$(cat "$T/formula.rb")" ] && grep -q "nothing to do" "$WORK/out.log"; then
    ok "idempotent same-version retry"
else
    ko "idempotent same-version retry (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi

# --- Test 3: downgrade refused -----------------------------------------------
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.0.9 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "refusing downgrade" "$WORK/out.log"; then ok "downgrade refused"; else ko "downgrade refused (rc=$rc)"; fi

# --- Test 4: missing entry refused -------------------------------------------
T="$WORK/t4"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
grep -v "aarch64-apple-darwin" "$T/release/checksums-sha256.txt" > "$T/partial.txt"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/partial.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "missing entries" "$WORK/out.log"; then ok "missing entry refused"; else ko "missing entry refused (rc=$rc)"; fi

# --- Test 5: wrong-version entry refused --------------------------------------
T="$WORK/t5"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
make_release "$T/other" "1.1.0"
cat "$T/release/checksums-sha256.txt" <(grep "aarch64-apple-darwin" "$T/other/checksums-sha256.txt") > "$T/mixed.txt"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/mixed.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "wrong version" "$WORK/out.log"; then ok "wrong-version entry refused"; else ko "wrong-version entry refused (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"; fi

# --- Test 6: duplicate entry refused ------------------------------------------
T="$WORK/t6"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
cat "$T/release/checksums-sha256.txt" <(head -1 "$T/release/checksums-sha256.txt") > "$T/dup.txt"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/dup.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "duplicate" "$WORK/out.log"; then ok "duplicate entry refused"; else ko "duplicate entry refused (rc=$rc)"; fi

# --- Test 7: malformed line refused --------------------------------------------
T="$WORK/t7"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
echo "not-a-sha  track-1.2.0-aarch64-apple-darwin.tar.gz" > "$T/bad.txt"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/bad.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "malformed checksums line" "$WORK/out.log"; then ok "malformed checksum refused"; else ko "malformed checksum refused (rc=$rc)"; fi

# --- Test 8: corrupt archive (bytes mismatch) refused, formula untouched -------
T="$WORK/t8"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
echo "CORRUPTED" >> "$T/release/track-1.2.0-aarch64-apple-darwin.tar.gz"
make_formula "$T/formula.rb"
BEFORE=$(cat "$T/formula.rb")
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "SHA256 mismatch" "$WORK/out.log" && [ "$BEFORE" = "$(cat "$T/formula.rb")" ]; then
    ok "corrupt archive refused, formula untouched"
else
    ko "corrupt archive refused, formula untouched (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi
if [ ! -e "$T/archives" ]; then ok "downloads cleaned up after failed verification"; else ko "downloads cleaned up after failed verification (dir left behind)"; fi

# --- Test 9: missing download target refused ----------------------------------
T="$WORK/t9"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
rm "$T/release/track-1.2.0-x86_64-unknown-linux-gnu.tar.gz"
make_formula "$T/formula.rb"
BEFORE=$(cat "$T/formula.rb")
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "failed to download" "$WORK/out.log" && [ "$BEFORE" = "$(cat "$T/formula.rb")" ]; then
    ok "missing archive refused, formula untouched"
else
    ko "missing archive refused, formula untouched (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi
if [ ! -e "$T/archives" ]; then ok "downloads cleaned up after failed download"; else ko "downloads cleaned up after failed download (dir left behind)"; fi

# --- Test 10: same version but different checksums updates checksums -----------
T="$WORK/t10"; mkdir -p "$T"
make_release "$T/release" "1.0.0"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.0.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
want=$(sha_of "$T/release/track-1.0.0-aarch64-apple-darwin.tar.gz")
if [ $rc -eq 0 ] && grep -q "$want" "$T/formula.rb"; then ok "same-version checksum repair"; else ko "same-version checksum repair (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"; fi

# --- Test 11: prerelease versions compare on their numeric core ----------------
T="$WORK/t11"; mkdir -p "$T"
make_release "$T/rc" "1.2.0-rc.1"
make_release "$T/release" "1.2.0"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/rc" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0-rc.1 '$T/rc/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -eq 0 ] && has_version "$T/formula.rb" 1.2.0-rc.1; then ok "update to prerelease"; else ko "update to prerelease (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"; fi
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -eq 0 ] && has_version "$T/formula.rb" 1.2.0; then ok "prerelease to release of same core"; else ko "prerelease to release of same core (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"; fi
eval "env $(common_env "$T/rc" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0-rc.1 '$T/rc/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "refusing downgrade" "$WORK/out.log" && has_version "$T/formula.rb" 1.2.0; then
    ok "release to prerelease of same core refused"
else
    ko "release to prerelease of same core refused (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi

# --- Test 12: TRACK_KEEP_DOWNLOADS=1 keeps downloads after a failure ----------
T="$WORK/t12"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
echo "CORRUPTED" >> "$T/release/track-1.2.0-aarch64-apple-darwin.tar.gz"
make_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") TRACK_KEEP_DOWNLOADS=1 bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && [ -f "$T/archives/track-1.2.0-aarch64-apple-darwin.tar.gz" ]; then
    ok "downloads kept on failure with TRACK_KEEP_DOWNLOADS=1"
else
    ko "downloads kept on failure with TRACK_KEEP_DOWNLOADS=1 (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi

# --- Test 13: --baseline sets the downgrade floor ------------------------------
# Mirrors CI: render the in-repo template (low placeholder version) while
# guarding against the tap's published formula.
T="$WORK/t13"; mkdir -p "$T"
make_release "$T/old" "1.1.0"
make_release "$T/release" "1.2.0"
make_formula "$T/formula.rb"
make_legacy_formula "$T/legacy.rb"
sed 's/version "1.0.0"/version "1.2.0"/' "$T/legacy.rb" > "$T/tap.rb"
BEFORE=$(cat "$T/formula.rb")
eval "env $(common_env "$T/old" "$T") bash '$SCRIPT' --formula '$T/formula.rb' --baseline '$T/tap.rb' 1.1.0 '$T/old/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "refusing downgrade" "$WORK/out.log" && [ "$BEFORE" = "$(cat "$T/formula.rb")" ]; then
    ok "downgrade below legacy-format baseline refused, formula untouched"
else
    ko "downgrade below legacy-format baseline refused, formula untouched (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi
TAP_BEFORE=$(cat "$T/tap.rb")
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' --baseline '$T/tap.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -eq 0 ] && has_version "$T/formula.rb" 1.2.0 && [ "$TAP_BEFORE" = "$(cat "$T/tap.rb")" ]; then
    ok "baseline version renders formula, baseline untouched"
else
    ko "baseline version renders formula, baseline untouched (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi
run_case "missing baseline refused" 1 \
    bash "$SCRIPT" --formula "$T/formula.rb" --baseline "$T/missing.rb" 1.2.0 "$T/release/checksums-sha256.txt"

# --- Test 14: a current-format baseline (version only in the urls) ------------
T="$WORK/t14"; mkdir -p "$T"
make_release "$T/old" "1.1.0"
make_formula "$T/formula.rb"
sed 's|/download/v1\.0\.0/track-1\.0\.0-|/download/v1.2.0/track-1.2.0-|' "$T/formula.rb" > "$T/tap.rb"
eval "env $(common_env "$T/old" "$T") bash '$SCRIPT' --formula '$T/formula.rb' --baseline '$T/tap.rb' 1.1.0 '$T/old/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "refusing downgrade: .* is at 1.2.0" "$WORK/out.log"; then
    ok "downgrade below url-versioned baseline refused"
else
    ko "downgrade below url-versioned baseline refused (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi

# --- Test 15: urls that disagree on the version are refused -------------------
T="$WORK/t15"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
make_formula "$T/formula.rb"
sed -i '0,/v1\.0\.0\/track-1\.0\.0-/s//v1.0.1\/track-1.0.1-/' "$T/formula.rb"
BEFORE=$(cat "$T/formula.rb")
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
if [ $rc -ne 0 ] && grep -q "url versions disagree" "$WORK/out.log" && [ "$BEFORE" = "$(cat "$T/formula.rb")" ]; then
    ok "disagreeing url versions refused, formula untouched"
else
    ko "disagreeing url versions refused, formula untouched (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi

# --- Test 16: a legacy-format formula still renders ---------------------------
T="$WORK/t16"; mkdir -p "$T"
make_release "$T/release" "1.2.0"
make_legacy_formula "$T/formula.rb"
eval "env $(common_env "$T/release" "$T") bash '$SCRIPT' --formula '$T/formula.rb' 1.2.0 '$T/release/checksums-sha256.txt'" >"$WORK/out.log" 2>&1
rc=$?
want=$(sha_of "$T/release/track-1.2.0-x86_64-unknown-linux-gnu.tar.gz")
if [ $rc -eq 0 ] && grep -q 'version "1.2.0"' "$T/formula.rb" && \
   [ "$(grep -c 'track-#{version}-' "$T/formula.rb")" -eq 4 ] && grep -q "$want" "$T/formula.rb"; then
    ok "legacy-format formula renders"
else
    ko "legacy-format formula renders (rc=$rc)"; sed 's/^/    /' "$WORK/out.log"
fi

echo
echo "Passed: $PASS, Failed: $FAIL"
[ $FAIL -eq 0 ]
