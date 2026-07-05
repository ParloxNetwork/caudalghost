# Política de seguridad · Security Policy

> **Español** · [English below](#english)

CaudalGhost es un monitor de red **observacional**: mide, nunca interfiere. La seguridad y la privacidad
son parte del diseño, no un añadido. Este documento explica cómo reportar problemas y qué garantías (y
límites) tiene el proyecto.

## Reportar una vulnerabilidad

- **Escribe a:** **jonathan@parlox.net** (asunto con el prefijo `[security]`). Si lo prefieres, puedes cifrar
  el correo con GPG hacia esa dirección.
- **No abras un issue público** para fallos de seguridad; usa el correo para divulgación coordinada.
- Incluye: versión afectada, edición (escritorio/servidor), pasos para reproducir e impacto estimado.
- **Qué esperar:** acuse de recibo lo antes posible; trabajamos el arreglo y coordinamos la publicación
  contigo. El proyecto es **pre-1.0** y sin programa de recompensas, pero tu reporte se agradece y se acredita
  (si lo deseas) en las notas de versión.

## Versiones soportadas

Al ser pre-1.0, se da soporte de seguridad a la **última versión publicada**. Actualiza a la última `.deb`
para recibir arreglos (la base de datos se conserva entre actualizaciones).

## Modelo de seguridad (garantías de diseño)

- **Daemon sin root.** `caudalghostd` corre como un usuario de sistema dedicado con **solo** `CAP_BPF` +
  `CAP_PERFMON` (`NoNewPrivileges`), no como root. La instalación pide privilegios (sudo) una vez; el uso
  diario no.
- **Cero egress, estructural.** La unit de systemd restringe el daemon a `AF_UNIX`/`AF_NETLINK`: no puede
  abrir un socket de red aunque quisiera. Sin telemetría, sin llamar a casa.
- **Sin superficie de red.** Los clientes (CLI/TUI/GUI) hablan por un **socket Unix local** (permisos `0660`
  + ACL POSIX por usuario). No hay puerto TCP ni servidor web.
- **Solo observa.** Cuenta bytes en el kernel (eBPF); no inspecciona contenido, no bloquea ni modela tráfico.
- **Datos locales.** El histórico vive en SQLite en tu equipo. Nada sale de la máquina.
- **Superficie mínima.** Sandbox de systemd endurecido; la GUI expone solo comandos de **solo lectura** con
  una CSP restrictiva; dependencias con procedencia de crates.io.
- **Libre y auditable.** AGPL-3.0 (la pieza eBPF, GPL-2.0). Puedes leer, compilar y verificar todo.

**Frontera de confianza:** el límite de seguridad es el **daemon**; los clientes corren sin privilegios. Una
petición malformada de un cliente jamás debe tumbar el daemon (a lo sumo cierra esa conexión).

## Riesgos conocidos y aceptados

Por honestidad, los documentamos en vez de esconderlos:

- **Pila transitiva de la edición GUI (gtk-rs/wry) — advisories aguas arriba.** Afecta **solo a la edición GUI**;
  el daemon, la CLI y la edición servidor **no** enlazan esa pila. Incluye: `RUSTSEC-2024-0429` (*unsoundness* en
  `glib::VariantStrIter`, que nuestro código **no** usa), `RUSTSEC-2026-0194`/`-0195` (DoS por complejidad
  cuadrática y por memoria en `quick-xml` al parsear **XML no confiable**) y crates gtk-rs (`atk`, `gdk`, …)
  marcados **sin mantenimiento**. En nuestro uso **no hay ruta de explotación práctica**: la GUI no parsea XML de
  red y no tiene entrada de red (habla con el daemon por socket Unix en JSON; la WebView solo carga activos
  empaquetados). Los arreglos están **bloqueados aguas arriba** (wry fija webkit2gtk=2.0.2 y gtk=0.18): forzarlos
  rompería la GUI. **Trackeado**: el CI ahora **audita el `Cargo.lock` de la GUI** (informativo) para vigilar esta
  superficie, y se endurecerá a bloqueante cuando Tauri/wry migren su pila gtk-rs.
- **Reutilización de PID en la atribución por app.** La atribución cgroup→app es *best-effort*: entre que el
  kernel entrega un PID y se lee `/proc`, ese PID pudo reciclarse. Puede atribuir mal tráfico en casos raros;
  es una limitación conocida, no un fallo de seguridad.
- **Colisión de etiqueta de app muy larga.** Los nombres de app se acotan (128 caracteres); dos apps que
  compartan un prefijo larguísimo podrían agregarse bajo la misma etiqueta. Baja probabilidad, afecta a la
  precisión, no a la seguridad.
- **CSP de la GUI con `'unsafe-inline'`.** La WebView carga **solo** activos empaquetados —sin orígenes remotos
  (`connect-src` a `'self'`/`ipc:`, `object-src 'none'`, `base-uri 'self'`, `frame-ancestors 'none'`)— pero la
  CSP mantiene `'unsafe-inline'` en `style-src`/`script-src` porque los estilos dinámicos de las barras y la
  hidratación de SvelteKit lo requieren hoy. Mitigaciones: las devtools **no** se compilan en release
  (`features=[]`) y se **niegan** explícitamente en las *capabilities*; estas son una allowlist mínima de
  comandos de **solo lectura**. Endurecer la CSP a hashes/nonces está **planeado** (requiere validación manual
  en WebKitGTK antes de shippear).

---

<a id="english"></a>

## 🇬🇧 English

CaudalGhost is an **observational** network monitor: it measures, it never interferes. Security and privacy
are part of the design, not an afterthought.

### Reporting a vulnerability

- **Email:** **jonathan@parlox.net** (prefix the subject with `[security]`). You may encrypt the email with GPG.
- **Do not open a public issue** for security bugs; use email for coordinated disclosure.
- Include: affected version, edition (desktop/server), reproduction steps, and estimated impact.
- **What to expect:** acknowledgment as soon as possible; we work the fix and coordinate disclosure with you.
  The project is **pre-1.0** with no bounty program, but reports are appreciated and credited (if you wish) in
  the release notes.

### Supported versions

Being pre-1.0, security support targets the **latest released version**. Update to the latest `.deb` for fixes
(your database is preserved across updates).

### Security model (design guarantees)

- **Root-less daemon** — runs as a dedicated system user with only `CAP_BPF` + `CAP_PERFMON` (`NoNewPrivileges`),
  not root. Install needs sudo once; daily use doesn't.
- **Zero egress, by construction** — the systemd unit restricts the daemon to `AF_UNIX`/`AF_NETLINK`; it can't
  open a network socket. No telemetry, nothing phones home.
- **No network surface** — clients talk over a **local Unix socket** (`0660` + POSIX ACL). No TCP port, no web server.
- **Watches only** — counts bytes in the kernel (eBPF); no content inspection, no blocking or shaping.
- **Local data** — history lives in SQLite on your machine. Nothing leaves the box.
- **Minimal surface** — hardened systemd sandbox; the GUI exposes only **read-only** commands under a strict CSP;
  dependencies sourced from crates.io.
- **Free & auditable** — AGPL-3.0 (the eBPF piece, GPL-2.0). You can read, compile and verify everything.

**Trust boundary:** the security boundary is the **daemon**; clients are unprivileged. A malformed client
request must never crash the daemon (at most it closes that connection).

### Known & accepted risks

- **GUI edition transitive stack (gtk-rs/wry) — upstream advisories.** Affects **only the GUI edition**; the
  daemon, CLI and server edition don't link this stack. Includes: `RUSTSEC-2024-0429` (unsoundness in
  `glib::VariantStrIter`, which our code doesn't use), `RUSTSEC-2026-0194`/`-0195` (quadratic-time and
  memory-exhaustion DoS in `quick-xml` when parsing **untrusted XML**), and gtk-rs crates (`atk`, `gdk`, …) flagged
  **unmaintained**. In our usage there is **no practical exploitation path**: the GUI parses no network XML and has
  no network input (it talks to the daemon over a Unix socket in JSON; the WebView only loads bundled assets). The
  fixes are **gated upstream** (wry pins webkit2gtk=2.0.2, gtk=0.18) — forcing them would break the GUI.
  **Tracked**: CI now **audits the GUI `Cargo.lock`** (informational) to watch this surface, and will be tightened
  to blocking once Tauri/wry move their gtk-rs stack.
- **PID reuse in app attribution** — cgroup→app attribution is best-effort; a recycled PID can occasionally
  mis-attribute traffic. A known limitation, not a security flaw.
- **Very long app-label collision** — app names are capped (128 chars); two apps sharing a very long prefix
  could aggregate under one label. Low probability; affects precision, not security.
- **GUI CSP with `'unsafe-inline'`** — the WebView loads **only** bundled assets (no remote origins:
  `connect-src` to `'self'`/`ipc:`, `object-src 'none'`, `base-uri 'self'`, `frame-ancestors 'none'`), but the
  CSP keeps `'unsafe-inline'` on `style-src`/`script-src` because the dynamic bar styles and SvelteKit hydration
  currently need it. Mitigations: devtools are **not** compiled in release (`features=[]`) and explicitly
  **denied** in the capabilities, which are a minimal allowlist of **read-only** commands. Tightening the CSP to
  hashes/nonces is **planned** (needs manual WebKitGTK validation before shipping).
