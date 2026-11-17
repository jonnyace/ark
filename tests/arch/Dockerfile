# Arch Linux (Omarchy's base) test image for the AppImage: WebKitWebDriver (shipped in webkitgtk-6.0 on Arch),
# Xvfb, dbus and Node. Tests run as an X11 client under Xvfb; Hyprland/Wayland itself is not exercised here.
FROM archlinux:latest
RUN pacman -Syu --noconfirm --needed webkitgtk-6.0 webkit2gtk-4.1 xorg-server-xvfb xorg-xauth dbus nodejs curl fuse2 \
      gst-plugins-base gst-plugins-good mesa && pacman -Scc --noconfirm
