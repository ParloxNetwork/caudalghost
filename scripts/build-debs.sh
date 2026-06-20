#!/usr/bin/env bash
# Construye los .deb de CaudalGhost. Por defecto AMBOS: escritorio + servidor.
#
#   ./scripts/build-debs.sh            # ambos (recompila también la GUI)
#   ./scripts/build-debs.sh server     # solo el de servidor (no toca la GUI)
#   ./scripts/build-debs.sh desktop    # solo el de escritorio
#
# Salida:
#   target/debian/caudalghost_<ver>_amd64.deb         -> escritorio (daemon+CLI+GUI)
#   target/debian/caudalghost-server_<ver>_amd64.deb  -> headless   (daemon+CLI)
#
# Requisitos en el PATH: cargo, cargo-deb, cargo-tauri, npm.
#
# IMPORTANTE: para que apt/dpkg lo trate como ACTUALIZACIÓN, sube la versión en
# Cargo.toml -> [workspace.package] version ANTES de cada release. La DB en
# /var/lib/caudalghost/ se conserva entre actualizaciones; el servicio se reinicia.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
export PATH="$HOME/.cargo/bin:$PATH"

target="${1:-both}"

build_binaries() {
    echo "==> Compilando daemon + CLI (release)"
    cargo build --release -p caudalghostd -p caudalghost
}

build_gui() {
    echo "==> Compilando GUI (Tauri, --no-bundle)"
    ( cd crates/caudalghost-gui && cargo tauri build --no-bundle )
}

build_desktop_deb() {
    echo "==> Empaquetando caudalghost (escritorio: daemon + CLI + GUI)"
    cargo deb -p caudalghostd
}

build_server_deb() {
    echo "==> Empaquetando caudalghost-server (headless: daemon + CLI)"
    cargo deb -p caudalghostd --variant server
}

case "$target" in
    both)    build_binaries; build_gui; build_desktop_deb; build_server_deb ;;
    desktop) build_binaries; build_gui; build_desktop_deb ;;
    server)  build_binaries; build_server_deb ;;
    *)       echo "uso: $0 [both|desktop|server]" >&2; exit 2 ;;
esac

echo ""
echo "==> Listo. Artefactos en target/debian/:"
ls -lh target/debian/*.deb | awk '{print "   " $5, $9}'
