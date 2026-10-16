#!/bin/bash
# Update the Homebrew formula with a new version and checksums.
#
# Usage: ./scripts/update-homebrew-formula.sh [--formula <path>] <version> <checksums-file>
#
# The checksums file must contain lines like:
#   abc123...  track-0.2.0-aarch64-apple-darwin.tar.gz
#
# Safety guarantees:
#   * Exactly one valid SHA256 entry is required for each of the four
#     versioned Homebrew archives, matched on the full filename (never on
#     platform substrings). Duplicate, missing, malformed, or wrong-version
#     entries are fatal errors.
#   * All four archives are downloaded and their bytes verified against the
#     checksums before the formula is modified.
#   * Version downgrades are refused.
#   * Re-running with the same version is idempotent: if the formula already
#     matches (version + all four checksums), the script exits 0 without
#     changing anything.
set -euo pipefail

FORMULA_PATH="homebrew/Formula/track.rb"
DOWNLOAD_DIR="${TRACK_DOWNLOAD_DIR:-dist/homebrew-archives}"
ASSET_URL_TEMPLATE="${TRACK_ARCHIVE_URL_TEMPLATE:-$(printf '%s' 'https://github.com/OrekGames/track-cli/releases/download/' 'v{VERSION}/' '{FILE}')}"
KEEP_DOWNLOADS="${TRACK_KEEP_DOWNLOADS:-0}"

usage() {
    echo "Usage: $0 [--formula <path>] <version> <checksums-file>"
    echo "Example: $0 0.2.0 dist/checksums-sha256.txt"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --formula)
            FORMULA_PATH="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        --*)
            echo "Error: unknown option: $1" >&2
            usage
            ;;
        *)
            break
            ;;
    esac
done

VERSION="${1:-}"
CHECKSUMS_FILE="${2:-}"

if [[ -z "$VERSION" || -z "$CHECKSUMS_FILE" ]]; then
    usage
fi

if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]]; then
    echo "Error: invalid version '$VERSION' (expected semver, e.g. 1.18.1)" >&2
    exit 1
fi

if [[ ! -f "$CHECKSUMS_FILE" ]]; then
    echo "Error: checksums file not found: $CHECKSUMS_FILE" >&2
    exit 1
fi

if [[ ! -f "$FORMULA_PATH" ]]; then
    echo "Error: formula file not found: $FORMULA_PATH" >&2
    exit 1
fi

# The four versioned archives the Homebrew formula references.
TARGETS=(
    "aarch64-apple-darwin"
    "x86_64-apple-darwin"
    "aarch64-unknown-linux-gnu"
    "x86_64-unknown-linux-gnu"
)

expected_file() {
    echo "track-${VERSION}-${1}.tar.gz"
}

# --- Enforce no downgrades before any other work ---------------------------

CURRENT_VERSION=$(sed -n 's/^  version "\(.*\)"/\1/p' "$FORMULA_PATH" | head -n1)

if [[ -z "$CURRENT_VERSION" ]]; then
    echo "Error: could not read current version from $FORMULA_PATH" >&2
    exit 1
fi

version_lt() {
    # Compare dot-separated numeric versions; true if $1 < $2
    local -a a b
    local i x y
    IFS=. read -r -a a <<< "$1"
    IFS=. read -r -a b <<< "$2"
    for i in 0 1 2 3 4; do
        x=${a[$i]:-0} y=${b[$i]:-0}
        if (( x < y )); then return 0; fi
        if (( x > y )); then return 1; fi
    done
    return 1
}

if version_lt "$VERSION" "$CURRENT_VERSION"; then
    echo "Error: refusing downgrade: formula is at $CURRENT_VERSION, refusing to move to $VERSION" >&2
    exit 1
fi

# --- Parse and strictly validate the checksums file -------------------------

declare -A FILE_SHA

while IFS= read -r line || [ -n "$line" ]; do
    [[ -z "$line" ]] && continue
    if ! [[ "$line" =~ ^([a-f0-9]{64})[[:space:]]+([^[:space:]]+)[[:space:]]*$ ]]; then
        echo "Error: malformed checksums line: '$line'" >&2
        exit 1
    fi
    sha="${BASH_REMATCH[1]}"
    filename="${BASH_REMATCH[2]}"
    if [[ -n "${FILE_SHA[$filename]:-}" ]]; then
        echo "Error: duplicate checksum entry for '$filename'" >&2
        exit 1
    fi
    FILE_SHA[$filename]="$sha"
done < "$CHECKSUMS_FILE"

missing=()
for target in "${TARGETS[@]}"; do
    file=$(expected_file "$target")
    if [[ -z "${FILE_SHA[$file]:-}" ]]; then
        missing+=("$file")
    fi
done

if [[ ${#missing[@]} -gt 0 ]]; then
    echo "Error: checksums file is missing entries for:" >&2
    for f in "${missing[@]}"; do
        echo "  $f" >&2
    done
    echo "Found entries:" >&2
    sed 's/^/  /' "$CHECKSUMS_FILE" >&2
    exit 1
fi

# Refuse checksum entries for the same platforms but a different version:
# this is how a wrong-release checksums file gets detected.
for filename in "${!FILE_SHA[@]}"; do
    is_expected=false
    for target in "${TARGETS[@]}"; do
        if [[ "$filename" == "track-${VERSION}-${target}.tar.gz" ]]; then
            is_expected=true
            break
        fi
    done
    if [[ "$is_expected" == true ]]; then
        continue
    fi
    for target in "${TARGETS[@]}"; do
        if [[ "$filename" == track-*-${target}.tar.gz ]]; then
            echo "Error: checksums file contains entry for wrong version: '$filename' (expected version ${VERSION})" >&2
            exit 1
        fi
    done
done

echo "Checksums validated for version $VERSION:"
for target in "${TARGETS[@]}"; do
    file=$(expected_file "$target")
    printf '  %-27s %s\n' "$target:" "${FILE_SHA[$file]}"
done

if [[ "$VERSION" == "$CURRENT_VERSION" ]]; then
    already_current=true
    for target in "${TARGETS[@]}"; do
        file=$(expected_file "$target")
        want="${FILE_SHA[$file]}"
        have=$(awk -v target="$target" '
            index($0, "track-#{version}-" target ".tar.gz") {
                inblock=1
                next
            }
            inblock && /sha256 "/ {
                gsub(/.*sha256 "/, ""); gsub(/".*/, "")
                print
                exit
            }
        ' "$FORMULA_PATH")
        if [[ "$have" != "$want" ]]; then
            already_current=false
            break
        fi
    done
    if [[ "$already_current" == true ]]; then
        echo "Formula already at version $VERSION with matching checksums; nothing to do."
        exit 0
    fi
    echo "Formula is at version $VERSION but checksums differ; updating checksums."
fi

# --- Download and verify every archive before touching the formula ----------

mkdir -p "$DOWNLOAD_DIR"

for target in "${TARGETS[@]}"; do
    file=$(expected_file "$target")
    want="${FILE_SHA[$file]}"
    path="$DOWNLOAD_DIR/$file"
    url=$(printf '%s\n' "$ASSET_URL_TEMPLATE" | sed -e "s/{VERSION}/$VERSION/g" -e "s/{FILE}/$file/g")

    echo "Downloading $file ..."
    if ! curl -fLsS --retry 3 -o "$path" "$url"; then
        echo "Error: failed to download $url" >&2
        exit 1
    fi
    if ! [[ -f "$path" && -s "$path" ]]; then
        echo "Error: downloaded archive is empty or missing: $path" >&2
        exit 1
    fi
    got=$(sha256sum "$path" | awk '{print $1}')
    if [[ "$got" != "$want" ]]; then
        echo "Error: SHA256 mismatch for $file" >&2
        echo "  expected: $want" >&2
        echo "  actual:   $got" >&2
        exit 1
    fi
    echo "  verified SHA256: $got"
done

# --- Render the formula ------------------------------------------------------

TMP_FORMULA=$(mktemp)
trap 'rm -f "$TMP_FORMULA" "${TMP_FORMULA}.out"' EXIT

awk -v version="$VERSION" '
    /^  version "/ {
        if (updated_version) {
            print "Error: multiple version lines in formula" > "/dev/stderr"
            exit 1
        }
        printf "  version \"%s\"\n", version
        updated_version = 1
        next
    }
    { print }
' "$FORMULA_PATH" > "$TMP_FORMULA"

if [[ ! -s "$TMP_FORMULA" ]]; then
    echo "Error: rendered formula is empty" >&2
    exit 1
fi

for target in "${TARGETS[@]}"; do
    file=$(expected_file "$target")
    want="${FILE_SHA[$file]}"
    if ! awk -v target="$target" -v sha="$want" '
        index($0, "track-#{version}-" target ".tar.gz") {
            print
            if ((getline line) <= 0) {
                print "Error: no sha256 line after url for " target > "/dev/stderr"
                exit 1
            }
            if (line !~ /sha256 "/) {
                print "Error: expected sha256 line after url for " target > "/dev/stderr"
                exit 1
            }
            sub(/sha256 "[^"]*"/, "sha256 \"" sha "\"", line)
            print line
            replaced = 1
            next
        }
        { print }
        END {
            if (!replaced) {
                print "Error: url not found in formula for target: " target > "/dev/stderr"
                exit 1
            }
        }
    ' "$TMP_FORMULA" > "${TMP_FORMULA}.out"; then
        echo "Error: failed to update checksum for $file" >&2
        exit 1
    fi
    mv "${TMP_FORMULA}.out" "$TMP_FORMULA"
done

if [[ "$KEEP_DOWNLOADS" != "1" ]]; then
    rm -rf "$DOWNLOAD_DIR"
fi

mv "$TMP_FORMULA" "$FORMULA_PATH"
trap - EXIT

echo "Formula updated to version $VERSION:"
sed -n '/^  version /p; /sha256 "/p' "$FORMULA_PATH"
