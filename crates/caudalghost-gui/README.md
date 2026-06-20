# CaudalGhost — GUI de escritorio

App Tauri v2 + SvelteKit (SPA, Svelte 5) que visualiza el caudal de red en vivo e histórico, sobre la misma API del socket del daemon `caudalghostd`. Aplica la identidad de marca CaudalGhost (ver `../../brand/`).

## Desarrollo

Requiere el daemon corriendo (crea el socket Unix):

```bash
# en la raíz del repo:
cargo build --release && ./target/release/caudalghostd &
# aquí:
cargo tauri dev
```

Workspace Cargo **standalone** (no es miembro del workspace principal): así `cargo test` del backend nunca construye webkit/tauri.

Parte del proyecto CaudalGhost — AGPL-3.0-only.
