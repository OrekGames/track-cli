#!/usr/bin/env bash
# Install track from a GitHub release archive.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/OrekGames/track-cli/main/scripts/install.sh | bash
#   TRACK_VERSION=1.15.1 bash scripts/install.sh
#
# Environment:
#   TRACK_VERSION      Optional release version, with or without a leading "v".
#   TRACK_INSTALL_DIR  Optional install directory. Defaults to ~/.tracker-cli.
#   TRACK_SKIP_PATH    Set to 1 to skip shell startup file changes.

set -euo pipefail

REPO="OrekGames/track-cli"
GITHUB_API_URL="https://api.github.com/repos/${REPO}"
GITHUB_RELEASE_URL="https://github.com/${REPO}/releases/download"
BINARY_NAME="track"

log() {
    printf '%s\n' "$*"
}

fail() {
    printf 'track installer: %s\n' "$*" >&2
    exit 1
}

need_command() {
    command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

normalize_version() {
    local version="$1"
    version="${version#v}"
    [[ -n "$version" ]] || fail "TRACK_VERSION cannot be empty"
    printf '%s\n' "$version"
}

latest_version() {
    local tag
    tag="$(curl -fsSL -H "User-Agent: track-installer" "${GITHUB_API_URL}/releases/latest" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | sed -n '1p')"
    [[ -n "$tag" ]] || fail "could not determine the latest release version"
    normalize_version "$tag"
}

detect_target() {
    local os arch
    os="$(uname -s)"
    arch="$(uname -m)"

    case "$arch" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            arch="aarch64"
            ;;
        *)
            fail "unsupported architecture: $arch"
            ;;
    esac

    case "$os" in
        Darwin)
            printf '%s-apple-darwin\n' "$arch"
            ;;
        Linux)
            printf '%s-unknown-linux-gnu\n' "$arch"
            ;;
        *)
            fail "unsupported operating system: $os"
            ;;
    esac
}

sha256_file() {
    local file="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        fail "required command not found: sha256sum or shasum"
    fi
}

verify_checksum() {
    local checksums_file="$1"
    local archive_file="$2"
    local archive_name="$3"
    local expected actual

    expected="$(awk -v name="$archive_name" '{ filename = $2; sub(/^\*/, "", filename); if (filename == name) print $1 }' "$checksums_file" | sed -n '1p')"
    [[ -n "$expected" ]] || fail "checksum not found for ${archive_name}"

    actual="$(sha256_file "$archive_file")"
    expected="$(printf '%s\n' "$expected" | tr '[:upper:]' '[:lower:]')"
    actual="$(printf '%s\n' "$actual" | tr '[:upper:]' '[:lower:]')"

    [[ "$expected" == "$actual" ]] || fail "checksum verification failed for ${archive_name}"
}

append_shell_config() {
    local config_file="$1"
    local shell_type="$2"
    local needs_path=1
    local needs_completion=1

    if grep -Fq "$INSTALL_DIR" "$config_file" 2>/dev/null; then
        needs_path=0
    elif [[ "$INSTALL_DIR" == "$HOME/.tracker-cli" ]] && grep -Fq ".tracker-cli" "$config_file" 2>/dev/null; then
        needs_path=0
    fi

    if grep -Fq "$COMPLETIONS_DIR" "$config_file" 2>/dev/null; then
        needs_completion=0
    elif [[ "$COMPLETIONS_DIR" == "$HOME/.tracker-cli/completions" ]] && grep -Fq "tracker-cli/completions" "$config_file" 2>/dev/null; then
        needs_completion=0
    fi

    if [[ "$needs_path" -eq 0 && "$needs_completion" -eq 0 ]]; then
        log "  ${config_file}: already configured"
        return
    fi

    {
        printf '\n# Added by track CLI installer\n'
        if [[ "$needs_path" -eq 1 ]]; then
            printf 'export PATH="%s:$PATH"\n' "$INSTALL_DIR"
        fi

        if [[ "$needs_completion" -eq 1 ]]; then
            case "$shell_type" in
                zsh)
                    printf 'if [ -d "%s" ]; then\n' "$COMPLETIONS_DIR"
                    printf '  fpath=("%s" $fpath)\n' "$COMPLETIONS_DIR"
                    printf '  autoload -Uz compinit\n'
                    printf '  compinit\n'
                    printf 'fi\n'
                    ;;
                bash)
                    printf 'if [ -f "%s/track.bash" ]; then\n' "$COMPLETIONS_DIR"
                    printf '  source "%s/track.bash"\n' "$COMPLETIONS_DIR"
                    printf 'fi\n'
                    ;;
            esac
        fi
    } >> "$config_file"

    log "  ${config_file}: updated"
}

configure_shell() {
    log ""
    log "Configuring shell PATH and completions..."

    if [[ -f "$HOME/.zshrc" || "${SHELL:-}" == *"zsh"* ]]; then
        touch "$HOME/.zshrc"
        append_shell_config "$HOME/.zshrc" "zsh"
    fi

    if [[ -f "$HOME/.bashrc" || ( "${SHELL:-}" == *"bash"* && ! -f "$HOME/.bash_profile" ) ]]; then
        touch "$HOME/.bashrc"
        append_shell_config "$HOME/.bashrc" "bash"
    fi

    if [[ -f "$HOME/.bash_profile" ]]; then
        append_shell_config "$HOME/.bash_profile" "bash"
    fi

    if [[ -d "$HOME/.config/fish" ]]; then
        local fish_completions_dir="$HOME/.config/fish/completions"
        mkdir -p "$fish_completions_dir"
        ln -sf "$COMPLETIONS_DIR/track.fish" "$fish_completions_dir/track.fish"
        log "  fish: completions symlinked to ${fish_completions_dir}/track.fish"
    fi
}

need_command curl
need_command tar
need_command awk
need_command sed
need_command tr

if [[ -n "${TRACK_VERSION:-}" ]]; then
    VERSION="$(normalize_version "$TRACK_VERSION")"
else
    VERSION="$(latest_version)"
fi

TAG="v${VERSION}"
TARGET="$(detect_target)"
ARCHIVE_NAME="track-${VERSION}-${TARGET}.tar.gz"
PACKAGE_NAME="track-${VERSION}-${TARGET}"

INSTALL_DIR="${TRACK_INSTALL_DIR:-$HOME/.tracker-cli}"
INSTALL_DIR="${INSTALL_DIR/#\~/$HOME}"
mkdir -p "$INSTALL_DIR"
INSTALL_DIR="$(cd "$INSTALL_DIR" && pwd)"
COMPLETIONS_DIR="$INSTALL_DIR/completions"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

ARCHIVE_PATH="$TMP_DIR/$ARCHIVE_NAME"
CHECKSUMS_PATH="$TMP_DIR/checksums-sha256.txt"
ARCHIVE_URL="${GITHUB_RELEASE_URL}/${TAG}/${ARCHIVE_NAME}"
CHECKSUMS_URL="${GITHUB_RELEASE_URL}/${TAG}/checksums-sha256.txt"

log "Installing track ${VERSION} for ${TARGET}"
log "Download: ${ARCHIVE_URL}"

curl -fL --retry 3 -o "$ARCHIVE_PATH" "$ARCHIVE_URL"
curl -fL --retry 3 -o "$CHECKSUMS_PATH" "$CHECKSUMS_URL"

verify_checksum "$CHECKSUMS_PATH" "$ARCHIVE_PATH" "$ARCHIVE_NAME"
log "Checksum verified."

tar -xzf "$ARCHIVE_PATH" -C "$TMP_DIR"

EXTRACTED_DIR="$TMP_DIR/$PACKAGE_NAME"
EXTRACTED_BINARY="$EXTRACTED_DIR/$BINARY_NAME"
[[ -x "$EXTRACTED_BINARY" ]] || fail "release archive did not contain an executable ${BINARY_NAME}"

cp "$EXTRACTED_BINARY" "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

mkdir -p "$COMPLETIONS_DIR"
"$INSTALL_DIR/$BINARY_NAME" completions bash > "$COMPLETIONS_DIR/track.bash"
"$INSTALL_DIR/$BINARY_NAME" completions zsh > "$COMPLETIONS_DIR/_track"
"$INSTALL_DIR/$BINARY_NAME" completions fish > "$COMPLETIONS_DIR/track.fish"

if [[ "${TRACK_SKIP_PATH:-}" == "1" ]]; then
    log ""
    log "Skipping shell startup file changes because TRACK_SKIP_PATH=1."
else
    configure_shell
fi

log ""
log "Installation complete."
log "  Binary:      $INSTALL_DIR/$BINARY_NAME"
log "  Completions: $COMPLETIONS_DIR"
log ""
log "Verify installation:"
log "  track --version"
log ""
log "Optional agent skills:"
log "  track init --skills"
