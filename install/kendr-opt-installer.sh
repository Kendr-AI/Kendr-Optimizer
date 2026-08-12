#!/bin/sh
set -eu

KENDR_DEFAULT_VERSION="v0.1.1"
KENDR_REPOSITORY="Kendr-AI/Kendr-Optimizer"

say() {
    printf '%s\n' "$*"
}

fail() {
    printf 'kendr-opt installer: %s\n' "$*" >&2
    exit 1
}

version=${KENDR_VERSION:-$KENDR_DEFAULT_VERSION}
if ! printf '%s\n' "$version" |
    grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+([-+][0-9A-Za-z.-]+)?$'; then
    fail "invalid release version: $version"
fi

system=$(uname -s 2>/dev/null || true)
machine=$(uname -m 2>/dev/null || true)

if [ "$system" = "Darwin" ] && [ "$machine" = "x86_64" ]; then
    if [ "$(sysctl -in sysctl.proc_translated 2>/dev/null || true)" = "1" ]; then
        machine="arm64"
    fi
fi

case "$system:$machine" in
    Linux:x86_64|Linux:amd64) target="x86_64-unknown-linux-musl" ;;
    Linux:aarch64|Linux:arm64) target="aarch64-unknown-linux-musl" ;;
    Darwin:x86_64|Darwin:amd64) target="x86_64-apple-darwin" ;;
    Darwin:aarch64|Darwin:arm64) target="aarch64-apple-darwin" ;;
    *) fail "unsupported platform: ${system:-unknown}/${machine:-unknown}" ;;
esac

asset="kendr-opt-$target.tar.gz"
install_dir=${KENDR_INSTALL_DIR:-"$HOME/.local/bin"}
[ -n "$install_dir" ] || fail "KENDR_INSTALL_DIR cannot be empty"

temp_root=$(mktemp -d "${TMPDIR:-/tmp}/kendr-opt.XXXXXX") || fail "cannot create a temporary directory"
staged_path=""
cleanup() {
    if [ -n "$staged_path" ]; then
        rm -f "$staged_path"
    fi
    rm -rf "$temp_root"
}
trap cleanup EXIT HUP INT TERM

download_with_gh=0
if [ -z "${KENDR_DOWNLOAD_BASE_URL:-}" ] && command -v gh >/dev/null 2>&1; then
    if gh auth status --hostname github.com >/dev/null 2>&1; then
        download_with_gh=1
    fi
fi

download_asset() {
    name=$1
    destination=$2
    if [ "$download_with_gh" -eq 1 ]; then
        gh release download "$version" \
            --repo "$KENDR_REPOSITORY" \
            --pattern "$name" \
            --output "$destination" >/dev/null ||
            fail "could not download $name with authenticated GitHub CLI"
        return
    fi

    base_url=${KENDR_DOWNLOAD_BASE_URL:-"https://github.com/$KENDR_REPOSITORY/releases/download/$version"}
    case "$base_url" in
        https://*) ;;
        http://*)
            [ "${KENDR_ALLOW_INSECURE:-}" = "1" ] ||
                fail "non-HTTPS download URL requires KENDR_ALLOW_INSECURE=1"
            ;;
        *) fail "download URL must use HTTPS" ;;
    esac
    url="${base_url%/}/$name"
    if command -v curl >/dev/null 2>&1; then
        if [ "${KENDR_ALLOW_INSECURE:-}" = "1" ]; then
            curl -LsSf --retry 3 --connect-timeout 20 "$url" -o "$destination" ||
                fail "could not download $name"
        else
            curl --proto '=https' --tlsv1.2 -LsSf --retry 3 \
                --connect-timeout 20 "$url" -o "$destination" ||
                fail "could not download $name; authenticate gh for private releases"
        fi
    elif command -v wget >/dev/null 2>&1; then
        wget -q -O "$destination" "$url" ||
            fail "could not download $name; authenticate gh for private releases"
    else
        fail "curl, wget, or authenticated GitHub CLI is required"
    fi
}

archive="$temp_root/$asset"
checksums="$temp_root/SHA256SUMS"
download_asset "$asset" "$archive"
download_asset "SHA256SUMS" "$checksums"

checksum_lines=$(awk -v name="$asset" '$2 == name { print $1 }' "$checksums")
checksum_count=$(printf '%s\n' "$checksum_lines" | awk 'NF { count++ } END { print count + 0 }')
[ "$checksum_count" -eq 1 ] || fail "SHA256SUMS must contain exactly one entry for $asset"
expected=$(printf '%s\n' "$checksum_lines" | tr 'A-F' 'a-f')
[ "${#expected}" -eq 64 ] || fail "invalid SHA-256 value for $asset"
case "$expected" in
    *[!0-9a-f]*) fail "invalid SHA-256 value for $asset" ;;
esac

if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$archive" | awk '{ print $1 }')
elif command -v shasum >/dev/null 2>&1; then
    actual=$(shasum -a 256 "$archive" | awk '{ print $1 }')
elif command -v openssl >/dev/null 2>&1; then
    actual=$(openssl dgst -sha256 "$archive" | awk '{ print $NF }')
else
    fail "a SHA-256 tool (sha256sum, shasum, or openssl) is required"
fi
actual=$(printf '%s' "$actual" | tr 'A-F' 'a-f')
[ "$actual" = "$expected" ] || fail "SHA-256 mismatch for $asset"

extract_dir="$temp_root/extracted"
mkdir "$extract_dir"
archive_root="kendr-opt-${version#v}-$target"
members_file="$temp_root/archive-members"
tar -tzf "$archive" > "$members_file" || fail "could not inspect $asset"
member_count=$(awk 'NF { count++ } END { print count + 0 }' "$members_file")
[ "$member_count" -eq 7 ] || fail "$asset has an unexpected archive layout"
for member in \
    "$archive_root/kendr-opt" \
    "$archive_root/CHANGELOG.md" \
    "$archive_root/LICENSE" \
    "$archive_root/NOTICE" \
    "$archive_root/README.md" \
    "$archive_root/RUST_STDLIB_LICENSES.html" \
    "$archive_root/THIRD_PARTY_LICENSES.html"
do
    matches=$(grep -Fxc "$member" "$members_file" || true)
    [ "$matches" -eq 1 ] || fail "$asset has an unexpected archive layout"
done
tar -xzf "$archive" -C "$extract_dir" || fail "could not extract $asset"
candidate="$extract_dir/$archive_root/kendr-opt"
[ -f "$candidate" ] || fail "archive does not contain the expected kendr-opt binary"
[ ! -L "$candidate" ] || fail "archive binary cannot be a symbolic link"
chmod 0755 "$candidate"

actual_version=$("$candidate" --version 2>/dev/null || true)
[ "$actual_version" = "kendr-opt ${version#v}" ] ||
    fail "downloaded binary version mismatch: ${actual_version:-no output}"
"$candidate" engines --compact >/dev/null 2>&1 || fail "downloaded binary failed its engine smoke test"

mkdir -p "$install_dir" || fail "cannot create install directory: $install_dir"
destination="$install_dir/kendr-opt"
if [ -e "$destination" ] && { [ ! -f "$destination" ] || [ -L "$destination" ]; }; then
    fail "existing destination is not a regular file: $destination"
fi
staged_path=$(mktemp "$install_dir/.kendr-opt.XXXXXX") ||
    fail "cannot create a staging file in $install_dir"
cp "$candidate" "$staged_path" || fail "cannot stage kendr-opt in $install_dir"
chmod 0755 "$staged_path"
mv -f "$staged_path" "$destination" || fail "cannot install kendr-opt in $install_dir"
staged_path=""
if [ ! -f "$destination" ] || [ -L "$destination" ]; then
    fail "installed destination is not a regular file: $destination"
fi

say "Installed kendr-opt ${version#v} to $destination"
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *)
        say "Add Kendr to this shell's PATH with:"
        say "  export PATH=\"$install_dir:\$PATH\""
        ;;
esac

resolved=$(command -v kendr-opt 2>/dev/null || true)
if [ -n "$resolved" ] && [ "$resolved" != "$destination" ]; then
    say "Warning: another kendr-opt at $resolved currently appears earlier on PATH."
fi
