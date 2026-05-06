#!/usr/bin/env bash
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary_name="tool-study-session"
install_dir="${HOME}/.local/bin"
desktop_dir="${HOME}/.local/share/applications"
data_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/tool-study-session"

"$project_root/scripts/build-linux.sh"

mkdir -p "$install_dir" "$desktop_dir" "$data_dir"
install -Dm755 "$project_root/dist/linux/$binary_name" "$install_dir/$binary_name"

cat > "$desktop_dir/$binary_name.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Tool Study Session
Comment=Terminal study clock, calendar, todo, and pomodoro app
Exec=$install_dir/$binary_name
Terminal=true
Categories=Utility;Office;
DESKTOP

chmod 644 "$desktop_dir/$binary_name.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$desktop_dir" >/dev/null 2>&1 || true
fi

echo "Installed $binary_name to $install_dir"
echo "Desktop launcher installed to $desktop_dir/$binary_name.desktop"
echo "Todo data will be stored in $data_dir"
