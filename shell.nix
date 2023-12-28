let
  pkgs = import (fetchTarball https://nixos.org/channels/nixos-unstable/nixexprs.tar.xz) { };
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    ### build SDL2-sys
    SDL2.dev

    ### serialport-rs
    # virt serial dev
    socat
    # serial client
    minicom
    # build libudev-sys: find libudev
    pkg-config
    # build libudev-sys: contains libudev
    systemd.dev
  ];
  buildInputs = with pkgs;[
  ];
  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
      pkgs.lib.makeLibraryPath  [
        pkgs.systemd.dev
      ]
    }"'';
}
