#!/bin/bash
# Install only Mesa's driver runtime in minimal Debian images. The full package
# also ships mesa-overlay-control.py, which would add an unused Python runtime.
set -euo pipefail

case "${1:?target architecture required}" in
    amd64|arm64) ;;
    arm) exit 0 ;;
    *) echo 'unsupported Vulkan runtime target architecture' >&2; exit 1 ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"
apt-get download mesa-vulkan-drivers
archives=(mesa-vulkan-drivers_*.deb)
test "${#archives[@]}" -eq 1
archive=${archives[0]}
test -z "$(dpkg-deb -f "$archive" Pre-Depends)"

# Retain every declared library dependency. Only the Python alternative for the
# omitted overlay-control utility is excluded; unfamiliar alternatives fail.
dependencies=()
while IFS= read -r dependency; do
    dependency=$(sed -E 's/\([^)]*\)//g; s/^[[:space:]]+//; s/[[:space:]]+$//' <<< "$dependency")
    if [[ "$dependency" =~ ^python3(:any)?[[:space:]]*\|[[:space:]]*python3-minimal(:any)?$ ]]; then
        continue
    fi
    if [[ ! "$dependency" =~ ^[a-z0-9][a-z0-9+.:_-]*$ ]]; then
        echo "unsupported Mesa runtime dependency: $dependency" >&2
        exit 1
    fi
    dependencies+=("$dependency")
done < <(dpkg-deb -f "$archive" Depends | tr ',' '\n')
apt-get install -y --no-install-recommends "${dependencies[@]}"

dpkg-deb -x "$archive" extracted
cp -a extracted/usr/lib/. /usr/lib/
mkdir -p /usr/share/vulkan /usr/share/drirc.d /usr/local/share/licenses/mesa-vulkan
cp -a extracted/usr/share/vulkan/. /usr/share/vulkan/
cp -a extracted/usr/share/drirc.d/. /usr/share/drirc.d/
cp extracted/usr/share/doc/mesa-vulkan-drivers/copyright /usr/local/share/licenses/mesa-vulkan/
dpkg-deb -f "$archive" Package Version Architecture > /usr/local/share/licenses/mesa-vulkan/package.txt
ldconfig
