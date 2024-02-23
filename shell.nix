let
  pkgs = import (fetchTarball https://nixos.org/channels/nixos-unstable/nixexprs.tar.xz) { };
  prepareVenvIfNotExists = ''
    # create .venv if not exists
    if [ ! -d .venv ]; then
      python3 -m venv .venv
    fi
    source .venv/bin/activate
  '';
in
pkgs.mkShell {
  nativeBuildInputs = with pkgs; [
    ### build(bin/examples) SDL2-sys
    SDL2.dev

    ### dev tool
    rustup
    python310
    socat
    minicom
    quickjs
    tigervnc
    file

    ### build(lib)
    # libudev-sys: find libudev
    pkg-config
    # libudev-sys: contains libudev
    systemd.dev
    # openssl-sys: ssh2
    openssl.dev
    # build python binding
    maturin

    ### ci
    act
  ];
  buildInputs = with pkgs;[
  ];
  shellHook = ''
    export LD_LIBRARY_PATH="$LD_LIBRARY_PATH:${
      pkgs.lib.makeLibraryPath  [
      ]
    }"
    ${prepareVenvIfNotExists}
  '';
}
