# Changelog — CaudalGhost

Formato basado en [Keep a Changelog](https://keepachangelog.com/es/). Versionado semántico.

## [0.6.2] — 2026-07-02

### Corregido

- **La GUI mostraba una versión desincronizada** (`v0.5.3`) aunque el paquete ya era 0.6.x: el footer
  leía la versión de `package.json`, que vive **fuera del workspace** y quedaba atrás en los releases que
  solo tocaban el daemon/CLI. Ahora `vite.config` deriva la versión del `Cargo.toml` del workspace
  (fuente de verdad única), de modo que el footer **siempre** coincide con la versión del release.
  Sincronizados también `package.json`, `src-tauri/tauri.conf.json` y `src-tauri/Cargo.toml`.

## [0.6.1] — 2026-07-02

### TUI (`caudalghost live`)

Ejes del gráfico de caudal y pico localizable (conserva el `Sparkline`; cliente-only).

- **Eje X de tiempo:** fila inferior con marcas relativas alineadas a las columnas (borde derecho =
  `ahora`, hacia la izquierda `12s`, `24s`, … según el ancho y `--window`).
- **Eje Y mínimo:** canalón izquierdo con el **máximo realmente dibujado** arriba y `0 B` abajo, para
  leer la magnitud de una barra de un vistazo.
- **Pico localizable:** el título pasa a `(pico X/s · hace Nmin)` — el pico de la ventana (10 min) ahora
  dice **cuándo** ocurrió; si cae dentro de la franja visible, se marca su columna con un `▲`.
- **Responsivo:** el canalón Y se omite en terminales estrechas y el eje X en terminales muy bajas,
  degradando al gráfico sin ejes sin romper el layout.

## [0.6.0] — 2026-07-02

### TUI (`caudalghost live`)

Arranca el arco **0.6.x = TUI**: el dashboard de terminal gana consistencia y funciones que solo tienen
sentido con la app viva a pantalla completa. Todo **cliente-only** (sin cambios en el daemon ni el protocolo).

- **Máquina de modos + teclas coherentes:** `Esc` retrocede un nivel según el modo; `q` solo sale en Normal.
  El pie muestra el **modo activo** (`[NORMAL]`/`[ZOOM]`/`[FILTRO]`/`[PAUSA+SCRUB]`/`[AYUDA]`) y las teclas
  del contexto.
- **Overlay de ayuda `?`** con todas las teclas agrupadas.
- **Foco de panel `Tab`/`Shift+Tab` + zoom `Enter`** a pantalla completa (`Esc` vuelve).
- **Selección de fila `j`/`k` · `↑`/`↓`** en el panel enfocado.
- **Pausa + scrub (`p`, luego `←`/`→`):** recorre el gráfico segundo a segundo con un marcador sobre el
  sparkline y lee el valor exacto de ese instante ("hace N s").
- **Filtro `/`** incremental por panel (se limpia al cambiar de foco).
- **Frescura del dato** en la cabecera (`dato: hace N s`, verde/ámbar/rojo).
- **Reconexión resiliente** con backoff, sin perder el histórico ya pintado.
- **Resaltado por umbral** (percentil 90 de la propia serie reciente).
- **Fecha con año** en la cabecera (`jue 02 jul 2026 · 16:38:57`).
- **Consistencia visual:** cabeceras y unidades uniformes; aviso de filas ocultas ("+N más — Enter para
  expandir") en vez de recorte silencioso.

Verificación: 113 tests verdes · `cargo clippy`/`fmt` limpios · `build --release` OK.

## [0.5.3] — 2026-07-02

### GUI

- **Top 10 + scroll:** apps/servidores en vivo y apps histórico acotadas al top 10; interfaces ilimitadas con
  scroll interno (mínimo 5 filas), sin agrandar el grid.
- **Fix del scroll de `main`** (`min-height:0` en el flex item) que afectaba a las vistas En vivo e Histórico.
- **Calendario propio** (`DateField.svelte` + `calendar.ts`) que reemplaza al `<input type=date>` nativo de
  WebKitGTK (que cambiaba el valor al navegar mes/año y se cerraba en cada clic): navegar no cierra, clic-día
  aplica+cierra, semana desde lunes (ISO 8601), etiquetas ES, sin futuro.

## [0.5.2] — 2026-07-01

### Corregido (GUI)

- **Histórico sin "corte" al final:** el eje del gráfico termina en el último dato disponible en vez de en el
  reloj "ahora", así en ventanas cortas (5 min, 1 h) ya no se ve un hueco vacío por el minuto en curso (que aún
  no está en el rollup por-minuto). Ayer / 24 h / etc. no cambian; el borde izquierdo sigue anclado a la ventana.

## [0.5.1] — 2026-07-01

### Corregido (GUI)

- **Eje Y sin recortes:** el ancho del eje Y se ajusta al rótulo más largo (p. ej. "1023.9 MiB"), así los números
  ya no se cortan contra el borde izquierdo, sin importar cuántos dígitos tengan (tanto en Histórico como En vivo).

## [0.5.0] — 2026-07-01

### Vista En vivo (GUI)

- **Gráfico fluido:** el caudal en vivo se desplaza suave (~60 FPS) en vez de saltar de segundo en segundo —
  el eje de tiempo se mueve continuo con `requestAnimationFrame`, con valores reales (sin interpolar). Se congela
  al pausar; respeta `prefers-reduced-motion`.
- **Rótulos del eje relativos al ahora:** en vez de segundos sueltos (`:05 :10…`), el borde derecho es "ahora" y
  hacia la izquierda "15s", "30s", "45s", "1 min".
- **Versión en el footer:** se muestra `vX.Y.Z |` antes de "Hecho con ❤ por Parlox Network".

## [0.4.0] — 2026-07-01

### Vista Histórico (GUI)

- **Rótulos del eje X con contexto de fecha:** eje de dos niveles (hora/día arriba, fecha/mes/año agrupado
  abajo), adaptativo al rango y con una sola etiqueta por día (sin números repetidos). Se acabaron los muros de
  horas sin saber de qué día son (7 días, 24 h, 5 min).
- **Eje anclado a la ventana pedida:** el gráfico se ancla a `[inicio, fin]` del rango en vez de auto-escalar a
  la extensión de los datos. Arregla la vista **"ayer"** (con un solo punto ya no inventa un eje de varios años)
  y **"24 h"** (cubre las 24 h completas hasta *ahora*). Además se limpia el estado al cambiar de rango.
- **Tooltip contenido:** ya no se sale de la ventana en el primer/último punto (clamp + flip); su texto de
  fecha/hora se alinea con el eje.
- **Selector de fechas con rango:** además del día suelto, un toggle "Rango" permite elegir Desde–Hasta (Hasta
  inclusivo; token `rango:YYYY-MM-DD:YYYY-MM-DD` en el backend). Ventana un poco más ancha para que el selector
  no salte de fila.

## [0.3.0] — 2026-06-20

### Seguridad / Hardening

Auditoría de ciberseguridad de extremo a extremo (2026-06-20) seguida de un hardening completo con **TDD**
(test rojo → verde por cada arreglo) y un **segundo barrido adversarial** que revisó el propio diff buscando
regresiones y vulnerabilidades nuevas. Modelo de amenazas: **local** (el daemon no expone red; la superficie es
el socket Unix, los datos no confiables que llegan al daemon —DNS, `/proc`—, el webview y la instalación).
Nada remotamente explotable. Detalle completo en `_local/plans/2026-06-20-security-audit-0.3.0.md`.

> La app ya partía de una postura sólida (usuario de sistema `nologin`, solo `CAP_BPF`/`CAP_PERFMON`,
> `systemd` muy endurecido, sin red, sin ejecución de subprocesos, SQL parametrizado). Estos cambios cierran
> robustez/DoS local, defensa en profundidad y endurecimiento de instalación.

#### Alta

- **H-1 · Exposición de tráfico entre usuarios en servidor multiusuario.**
  *Identificado:* la concesión automática de ACL del socket por UID (feature D11) daba acceso a **todo** usuario
  con sesión logind (UID≥1000). En un servidor, cada login SSH crea sesión → cualquier usuario podía leer los
  flujos de red de **toda la máquina** (`access.rs`, CWE-284/200).
  *Arreglado:* palanca de política `grant_active_users` (flag `--no-acl-active-users` o
  `CAUDALGHOST_ACL_ACTIVE_USERS=0`); **el escritorio preserva el comportamiento por defecto** («instalar→abrir→
  funciona») y **la edición SERVIDOR arranca restringida**: su `.deb` instala un drop-in
  (`…/caudalghostd.service.d/10-acl-off.conf`) con `CAUDALGHOST_ACL_ACTIVE_USERS=0`, de modo que el socket solo
  lo concede el grupo `caudalghost`. Permitir acceso a todos los usuarios con sesión es un paso **intencional**
  (`sudo systemctl edit caudalghostd` → `Environment=CAUDALGHOST_ACL_ACTIVE_USERS=1`), y el daemon **avisa en el
  log** cada vez que arranca con el acceso por-UID activado. (commits `a48d8a3`, `e58be07`, + drop-in del server)

#### Media

- **H-2 · DoS local del socket IPC.**
  *Identificado:* un hilo por conexión sin límite, sin timeout de lectura y sin tope de longitud de línea →
  agotamiento de hilos/heap por un cliente local con acceso al socket (`api.rs`, CWE-400/770).
  *Arreglado:* timeout de E/S (30 s), lectura de línea acotada a 64 KiB (línea enorme → `Error`, no OOM) y
  `ConnLimiter` (máx. 128 conexiones concurrentes). Mitigación complementaria en systemd (H-9). (commit `8323958`)

#### Baja

- **H-3/H-4 · Webview sin CSP + `innerHTML`.**
  *Identificado:* `csp: null` en Tauri (sin defensa en profundidad) y un `innerHTML` latente en el tooltip del
  chart (`tauri.conf.json`, `Chart.svelte`, CWE-79).
  *Arreglado:* CSP estricta (`default-src 'self'`, `object-src 'none'`, `frame-ancestors 'none'`, `connect-src`
  solo IPC) y tooltip con DOM seguro (`textContent`). (commit `cdd6ea0`) — *smoke-test visual de la build de
  producción: OK (fuentes, gráfico en vivo y paneles renderizan; la CSP no rompe nada).*
- **H-5 · `$SUDO_USER` sin sanear en `postinst` (root).**
  *Identificado:* `usermod -aG caudalghost $SUDO_USER` con valor influenciable → inyección de opciones (CWE-88).
  *Arreglado:* `usermod ... -- "$user"` + guard de charset (permite usuarios de empresa Kerberos/Samba, rechaza
  metacaracteres y pinta de opción). Test: `scripts/test-postinst.sh`. (commits `31c3251`, `4dade72`)
- **H-6 · Plugin `opener` sin uso.**
  *Identificado:* `tauri-plugin-opener` exponía `open_url`/`open_path` al webview sin que el frontend lo use
  (CWE-272). *Arreglado:* eliminado de Rust, capabilities y npm (`package.json`); de paso podó ~85 deps
  transitivas del lock. (commits `35b483e`, `ad29b13`)
- **H-7 · Strings no confiables sin acotar.**
  *Identificado:* etiquetas de app (`/proc/{cgroup,comm}`, fijables por un proceso) y hostnames DNS se
  almacenaban sin tope de longitud ni saneo de control chars (`appmap.rs`, `resolve.rs`, CWE-400).
  *Arreglado:* `sanitize_label` (cap 128 + sin control chars), `is_acceptable_hostname` (drop vacío/>253/control),
  tope defensivo de 1 MiB en el buffer del monitor. (commit `e7f293c`)

#### Info

- **H-8 · `query_top_remotes` sin `LIMIT`.** Top de remotos acotado a 500 filas (top-N legítimo que acota la
  respuesta IPC; cubre un escritorio realista sin truncar). (commits `5afdee4`, `e58be07`)
- **H-9 · `systemd` sin techo de recursos.** `TasksMax=256` + `MemoryMax=256M` como control compensatorio del DoS
  (H-2), sin afectar a eBPF. (commit `c33b4c6`)
- **H-9b · Símbolos en release.** `[profile.release] strip=true` (binarios más pequeños, sin tabla de símbolos).
  (commit `65b448a`)

#### Correcciones del review adversarial del propio diff

10 hallazgos (todos Low/Info): 8 corregidos en código (regresión del `LIMIT` global con muchas apps; parsing
case-insensitive del env; `postinst` que rechazaba usuarios de empresa; rustdoc desplazado; visibilidad y
ordenación atómica de `ConnLimiter`; remanente npm del opener) y 2 documentados (el `'unsafe-inline'` de la CSP es
necesario por la hidratación inline de SvelteKit). (commits `e58be07`, `4dade72`, `ad29b13`)

### Verificación

178 resultados de test verdes · `cargo clippy --all-targets` limpio · `cargo fmt --check` limpio ·
`cargo build --release` OK · `scripts/test-postinst.sh` 5/5 · `svelte-check` 0 errores + `vite build` OK.

### Estado del release

Publicado el 2026-06-20: versión `0.3.0` en `Cargo.toml`, tag firmado `v0.3.0`, **ambas** `.deb`
reempacadas (`scripts/build-debs.sh`) e instaladas en escritorio y servidor. Smoke-test visual de la GUI
(CSP) verificado; política del `.deb` servidor resuelta (arranca con el acceso por-UID OFF; ON es opt-in
avisado en el log).

Residual: **push** a un remoto cuando exista (hoy el repositorio es solo local).

### Rollback

`git checkout main` (o `git reset --hard v0.2.0-baseline`) devuelve el árbol 0.2.0 intacto.
