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

# Remapeo de rutas embebidas: los binarios PÚBLICOS de las .deb no deben filtrar
# /home/<dev>/.cargo/... ni el path del proyecto (fuga del usuario del dev) — y es
# un paso hacia builds reproducibles. Mapea el HOME del dev a un prefijo neutro en
# el código de HOST de release (daemon, CLI y GUI). Preserva RUSTFLAGS previo.
#
# LIMITACIÓN CONOCIDA: este remap NO alcanza al objeto eBPF embebido. aya-build
# compila crates/caudalghost-ebpf en un cargo anidado fijando su propio
# CARGO_ENCODED_RUSTFLAGS (con prioridad sobre RUSTFLAGS), así que el remap se
# ignora para ese sub-build. Resultado verificado: CLI y GUI quedan 100% limpias;
# el daemon shippeado solo retiene rutas del dev DENTRO del blob BPF (aya-ebpf,
# rust-src nightly, path del proyecto). Cerrarlo exigiría tocar el toolchain del
# eBPF (riesgo de romper el kprobe), así que se documenta en vez de forzarlo.
export RUSTFLAGS="--remap-path-prefix=$HOME=/home/build ${RUSTFLAGS:-}"

target="${1:-both}"

build_binaries() {
    echo "==> Compilando daemon + CLI (release)"
    cargo build --release -p caudalghostd -p caudalghost
}

build_completions_man() {
    echo "==> Generando completions de shell + man page"
    local bin="target/release/caudalghost"
    mkdir -p target/completions target/man
    "$bin" completions bash > target/completions/caudalghost.bash
    "$bin" completions zsh   > target/completions/_caudalghost
    "$bin" completions fish  > target/completions/caudalghost.fish
    "$bin" man | gzip -9 -n > target/man/caudalghost.1.gz
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
    both)    build_binaries; build_completions_man; build_gui; build_desktop_deb; build_server_deb ;;
    desktop) build_binaries; build_completions_man; build_gui; build_desktop_deb ;;
    server)  build_binaries; build_completions_man; build_server_deb ;;
    *)       echo "uso: $0 [both|desktop|server]" >&2; exit 2 ;;
esac

echo ""
echo "==> Listo. Artefactos en target/debian/:"
ls -lh target/debian/*.deb | awk '{print "   " $5, $9}'
