{
  # Optional Nix entry points for development and for a reproducible build of the CLI. Nix is never needed to run
  # the apps. The desktop bundles are built in the pinned container (build/desktop.sh); see DECISIONS.md D-018.
  description = "Lastbook Next: offline knowledge library with optional local AI";
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/b349185bb79c7cf7ad8ea8c097931730bf473c71"; # nixos-26.05, 2026-11-15
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      forAll = f: nixpkgs.lib.genAttrs systems (s: f nixpkgs.legacyPackages.${s});
    in {
      # `nix develop`: everything build/ci.sh needs, including the WebKitGTK/GStreamer SDK for the desktop app.
      # rustup reads rust-toolchain.toml, so the Rust version stays the one the project pins.
      devShells = forAll (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [
            rustup nodejs_22 pnpm python3 git cmake ninja gcc pkg-config sqlite ffmpeg
            webkitgtk_4_1 gtk3 libsoup_3 openssl librsvg glib-networking xdotool
            gst_all_1.gstreamer gst_all_1.gst-plugins-base gst_all_1.gst-plugins-good
          ];
        };
      });
      # `nix build .#lastbook-cli`: the command-line tool from the locked Cargo graph, in the Nix sandbox.
      packages = forAll (pkgs: rec {
        lastbook-cli = pkgs.rustPlatform.buildRustPackage {
          pname = "lastbook-cli";
          version = "0.9.0-rc.1";
          src = pkgs.lib.cleanSource ./.;
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = [ "-p" "lbn-cli" ];
          cargoTestFlags = [ "-p" "lbn-core" ];
          meta = { description = "Build, inspect, verify, install and search Lastbook Next packs"; license = pkgs.lib.licenses.mit; mainProgram = "lastbook"; };
        };
        default = lastbook-cli;
      });
    };
}
