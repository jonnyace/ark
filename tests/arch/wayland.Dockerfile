# Arch + headless Wayland compositors for testing Lastbook Ark as a pure Wayland client (no X11 at all):
# sway (wlroots, headless backend, pixman renderer) and Hyprland (Omarchy's compositor) nested inside it.
FROM lastbook-ark-arch-test:dev
RUN pacman -Syu --noconfirm --needed sway hyprland weston foot && pacman -Scc --noconfirm
