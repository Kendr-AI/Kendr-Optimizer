#!/bin/sh
set -eu

KENDR_DEFAULT_VERSION="v0.1.4"
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
    grep -Eq '^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z][0-9A-Za-z-]*(\.[0-9A-Za-z][0-9A-Za-z-]*)*)?$'; then
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
staged_receipt=""
binary_backup=""
cleanup() {
    if [ -n "$staged_path" ]; then
        rm -f "$staged_path"
    fi
    if [ -n "$staged_receipt" ]; then
        rm -f "$staged_receipt"
    fi
    if [ -n "$binary_backup" ]; then
        rm -f "$binary_backup"
    fi
    rm -rf "$temp_root"
}
trap cleanup EXIT HUP INT TERM

installer_test_download=0
if [ -n "${KENDR_DOWNLOAD_BASE_URL:-}" ]; then
    if [ "${KENDR_INSTALLER_TEST_MODE:-}" != "1" ] ||
        [ "${KENDR_ALLOW_INSECURE:-}" != "1" ]; then
        fail "KENDR_DOWNLOAD_BASE_URL is restricted to numeric loopback installer tests"
    fi
    if ! printf '%s\n' "$KENDR_DOWNLOAD_BASE_URL" |
        grep -Eq '^http://127\.0\.0\.1:[0-9]+/?$'; then
        fail "KENDR_DOWNLOAD_BASE_URL is restricted to numeric loopback installer tests"
    fi
    installer_test_download=1
fi

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
            --repo "github.com/$KENDR_REPOSITORY" \
            --pattern "$name" \
            --output "$destination" >/dev/null ||
            fail "could not download $name with authenticated GitHub CLI"
        return
    fi

    if [ "$installer_test_download" -eq 1 ]; then
        base_url=$KENDR_DOWNLOAD_BASE_URL
    else
        base_url="https://github.com/$KENDR_REPOSITORY/releases/download/$version"
    fi
    url="${base_url%/}/$name"
    if command -v curl >/dev/null 2>&1; then
        if [ "$installer_test_download" -eq 1 ]; then
            curl -LsSf --retry 3 --connect-timeout 20 "$url" -o "$destination" ||
                fail "could not download $name"
        else
            curl --proto '=https' --tlsv1.2 -LsSf --retry 3 \
                --connect-timeout 20 "$url" -o "$destination" ||
                fail "could not download public release asset $name"
        fi
    elif command -v wget >/dev/null 2>&1; then
        if [ "$installer_test_download" -eq 1 ]; then
            wget -q -O "$destination" "$url" || fail "could not download $name"
        else
            wget --https-only --secure-protocol=TLSv1_2 -q -O "$destination" "$url" ||
                fail "could not download public release asset $name"
        fi
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
receipt="$install_dir/.kendr-opt-install.json"
if [ -L "$destination" ] || { [ -e "$destination" ] && [ ! -f "$destination" ]; }; then
    fail "existing destination is not a regular file: $destination"
fi
if [ -L "$receipt" ] || { [ -e "$receipt" ] && [ ! -f "$receipt" ]; }; then
    fail "existing install receipt is not a regular file: $receipt"
fi
staged_path=$(mktemp "$install_dir/.kendr-opt.XXXXXX") ||
    fail "cannot create a staging file in $install_dir"
cp "$candidate" "$staged_path" || fail "cannot stage kendr-opt in $install_dir"
chmod 0755 "$staged_path"

staged_receipt=$(mktemp "$install_dir/.kendr-opt-install.XXXXXX") ||
    fail "cannot create an install receipt staging file in $install_dir"
printf '{"schema_version":"kendr.install/v1","repository":"%s","install_method":"github-release","target":"%s","version":"%s","channel":"preview"}\n' \
    "$KENDR_REPOSITORY" "$target" "${version#v}" > "$staged_receipt" ||
    fail "cannot stage the install receipt in $install_dir"
chmod 0644 "$staged_receipt"

destination_existed=0
if [ -e "$destination" ]; then
    destination_existed=1
    binary_backup=$(mktemp "$install_dir/.kendr-opt.backup.XXXXXX") ||
        fail "cannot create a rollback file in $install_dir"
    cp -p "$destination" "$binary_backup" ||
        fail "cannot preserve the existing kendr-opt for rollback"
fi

mv -f "$staged_path" "$destination" || fail "cannot install kendr-opt in $install_dir"
staged_path=""
if ! mv -f "$staged_receipt" "$receipt"; then
    rollback_failed=0
    if [ "$destination_existed" -eq 1 ]; then
        if mv -f "$binary_backup" "$destination"; then
            binary_backup=""
        else
            rollback_failed=1
        fi
    elif ! rm -f "$destination"; then
        rollback_failed=1
    fi
    if [ "$rollback_failed" -eq 1 ]; then
        fail "cannot install the receipt and could not restore the previous kendr-opt"
    fi
    fail "cannot install the receipt; the binary installation was rolled back"
fi
staged_receipt=""
if [ -n "$binary_backup" ]; then
    rm -f "$binary_backup"
    binary_backup=""
fi
if [ ! -f "$destination" ] || [ -L "$destination" ]; then
    fail "installed destination is not a regular file: $destination"
fi
if [ ! -f "$receipt" ] || [ -L "$receipt" ]; then
    fail "installed receipt is not a regular file: $receipt"
fi

say "Installed kendr-opt ${version#v} to $destination"
say "Next: kendr-opt setup --list"
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
