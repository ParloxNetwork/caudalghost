# Changelog — CaudalGhost

Formato basado en [Keep a Changelog](https://keepachangelog.com/es/). Versionado semántico.

## [0.8.1] — 2026-07-06

### Interfaz — cliente-only (sin cambios en el daemon ni el protocolo)

- **Panel de destinos renombrado `Servidores` → `Hosts`** en la GUI y la TUI (`caudalghost live`). Ese panel se
  llena **solo** con hosts resueltos por DNS a los que tus apps salieron a conectarse — que a veces incluyen
  equipos de tu **red local** (un NAS, el router), así que "servidores remotos" resultaba impreciso. **"Hosts"**
  es correcto en todos los casos (remoto o local), coincide con el nombre interno y con la CLI
  (`caudalghost hosts`), y es idéntico en inglés. Solo cambia la etiqueta; sin cambios de comportamiento, ni en
  el daemon, el protocolo o la base de datos (instala como actualización conservando el histórico).

## [0.8.0] — 2026-07-04

### Seguridad — segundo endurecimiento (tras 0.3.0)

Pasada de hardening integral, motivada por una auditoría (gitleaks + semgrep + trivy). **Sin cambios en el
protocolo ni en el formato de datos**; la base de datos se conserva. No se pierde ningún control ya ganado: un
test de regresión fija el sandbox de la unit systemd de 0.3.0.

- **Frontera de confianza más robusta (daemon + IPC).** El cliente **acota la respuesta del daemon a 64 MiB**
  (evita consumo de memoria no acotado si una línea llega gigante). La atribución cgroup→app **descarta
  etiquetas vacías** (`app-.scope`, `snap.`, `.service` exactos) en vez de mezclar el tráfico de procesos
  distintos bajo la clave `""` → caen a `comm`/`(desconocido)`.
- **Test de regresión del sandbox systemd.** Fija los controles de seguridad de 0.3.0 (`User=caudalghost`,
  `CAP_BPF`/`CAP_PERFMON`, `NoNewPrivileges`, `RestrictAddressFamilies`, `ProtectSystem=strict`, filtro de
  syscalls) y **veta reintroducir directivas que rompan el eBPF** (`MemoryDenyWriteExecute`, `PrivateUsers`,
  `DynamicUser`, `ProtectProc=invisible`, denylist de syscalls).
- **Fuzzing.** Harness `cargo-fuzz` sobre los límites de input no confiable — el parser del protocolo IPC, la
  atribución cgroup→app y el parser de respuestas del monitor de systemd-resolved (`parse_monitor_answer`,
  influido por DNS) — más pruebas adversariales deterministas en el suite. Verificado: **1.58M ejecuciones sin
  una sola caída**.
- **Cadena de suministro / CI.** Workflows de **CI** (fmt, clippy, tests; job aparte para el eBPF) y de
  **seguridad** (`cargo-audit` sobre el workspace **y el `Cargo.lock` de la GUI** —informativo, cierra un punto
  ciego donde vivía glib—, `cargo-deny`, `gitleaks`, `semgrep`, `npm audit` de la GUI, y un guard de superficie de
  la GUI corregido a los namespaces reales de permisos de Tauri v2). MSRV declarado (`rust-version = 1.96`).
- **GUI.** Se **niegan explícitamente las devtools internas** (defensa en profundidad; en release ya no se
  compilan, `features=[]`); las *capabilities* quedan como allowlist mínima de comandos de **solo lectura**.
- **Builds sin fugas de rutas.** `--remap-path-prefix` en el pipeline de release: los binarios publicados dejan
  de filtrar el path del equipo de desarrollo (la CLI queda 100% limpia; el daemon solo retiene rutas del blob
  eBPF, cuyo build anidado no hereda el remap). Un paso hacia builds reproducibles.
- **Formalización.** `SECURITY.md` (reporte coordinado + modelo de seguridad + riesgos aceptados, **ES/EN**),
  **modelo de amenazas** (`docs/threat-model.md`) y **página de Seguridad** en el sitio (ES/EN). El riesgo
  `RUSTSEC-2024-0429` (glib, **solo edición GUI**, bloqueado aguas arriba) queda documentado y **trackeado**.

## [0.7.4] — 2026-07-04

### CLI — pulido (cliente-only, sin cambios en el daemon ni el protocolo)

- **Ordenar por columna `--sort <campo>` + `--reverse`/`-r`:** ordena las listas por `app`/`host`/`interfaz`,
  `rx`, `tx` o `total`. Por defecto **descendente** (mayor→menor / Z→A); `--reverse` invierte a ascendente
  (y solo, sin `--sort`, invierte el orden por defecto del comando). Aplica a `top`, `hosts`, `total` y `now`;
  `serie` (cronológica) y `ping` lo rechazan con un mensaje claro. Sin `--sort`, el orden por defecto (ranking
  por total) no cambia.
- **`--no-header`:** omite la línea de cabecera en `-o csv` (para pipelines que solo quieren datos). Fuera de
  csv → error claro. Sin la opción, el csv sale igual que antes.
- **Ejemplos en `--help`:** cada subcomando de datos (`ping/now/total/top/hosts/serie`) muestra ahora una
  invocación de ejemplo al pie de su `--help`.

## [0.7.3] — 2026-07-03

### Presentación y empaquetado (sin cambios en el daemon ni el protocolo)

- **README de portada** (`README.md`) bilingüe **español/inglés**: presenta la app (no es documentación —
  eso vive en el sitio) con banner de marca **adaptable al tema** de GitHub (variante clara del lockup),
  previews de terminal con datos de ejemplo, instalación de las dos ediciones `.deb`, nota de privacidad y
  la firma de Parlox Network. Enlaza a [caudalghost.parlox.net](https://caudalghost.parlox.net).
- **Metadatos de creador/maintainer** más completos para los instaladores gráficos (Discover, GNOME
  Software) y `apt`:
  - **Síntesis del paquete** real (antes era el marcador `[generated from Rust crate …]`).
  - La **página de manual** incluye ahora la sección `AUTHORS` (Parlox Network).
  - `authors` a nivel de workspace en los metadatos de Cargo.
  - **AppStream** enriquecido: categorías, palabras clave y bloque de **novedades** por versión.
  - `.desktop` sin la advertencia de doble categoría principal.

## [0.7.2] — 2026-07-03

### CLI — features (cliente-only, sin tocar el daemon ni el protocolo)

- **Subcomando nuevo `serie <RANGO>`:** timeline del **caudal total** (todas las interfaces sumadas) por
  cubos temporales. En `-o table` dibuja una **sparkline** del total + una tabla `tiempo │ rx │ tx │ total`
  con fila `Σ total`; en `-o json/csv/ndjson` sale por el mismo contrato que el resto (`"schema":1`, ndjson
  con bytes crudos por línea). El **cubo** (`minute`/`hour`/`day`) se elige automáticamente según el rango
  o se fija con `--bucket`. Usa la petición `Series` ya existente del protocolo.
- **Proyección de columnas `--fields <a,b,c>`:** limita la salida de **csv/json/ndjson** a las columnas
  pedidas (en el orden pedido). Nombres válidos = los de la cabecera del comando (`app`/`host`/`interfaz`/
  `tiempo`, `rx`, `tx`, `total`); en json/ndjson los valores salen **crudos** (números), aptos para `jq`.
  Con `-o table` o en `ping` → error claro. **Sin `--fields`, la salida no cambia** respecto a 0.7.1.
- **Refresco periódico `--watch` / `--interval <SECS>`:** reejecuta un comando de datos cada `SECS`
  segundos (default 2) limpiando pantalla, hasta `Ctrl-C` — un monitor ligero en la terminal sin entrar a
  la TUI. Solo `-o table`; con formatos máquina → error (para pipelines está el bucle de shell). El primer
  render no borra la pantalla, así un error inmediato no la limpia.

## [0.7.1] — 2026-07-03

### CLI — pulido

Pulido de las asperezas de 0.7.0 (cliente-only, sin tocar el daemon ni el protocolo).

- **Contrato JSON unificado en toda la CLI:** `now` y `ping` pasan por la misma capa de salida que
  `total/top/hosts`. Su `-o json` deja de ser el "sobre" crudo del `Response` y lleva `"schema": 1` como el
  resto. Además `now` ya soporta **`-o csv` y `-o ndjson`** (una fila por interfaz); `-o table` conserva el
  sparkline y la media por segundo. `ping` sigue en `table`/`json` (no es tabular → `csv`/`ndjson` dan error
  claro). Cierra la nota "queda para 0.7.1" del changelog de 0.7.0.
- **`--version` con contexto de build:** muestra versión + hash corto de git + fecha
  (p. ej. `caudalghost 0.7.1 (a1b2c3d 2026-07-03)`), vía un `build.rs` nuevo que degrada a `desconocido`
  sin git y no rompe el build.
- **Flags de salida acotados a los comandos donde aplican:** `-o/--output`, `-u/--units`, `--json`,
  `-q/--quiet` y `--no-color` se aplanan (`clap::Args`) en `ping/now/total/top/hosts` y **dejan de aparecer
  en `live --help`** (la TUI los ignoraba). `--socket` sigue siendo global.
- **Aviso en `hosts --app <X>` sin datos:** en tabla imprime `sin datos para la app '<X>' en el rango <R>`
  en vez de una tabla vacía; en `json/csv/ndjson` la salida sigue vacía (lista vacía válida para scripts).

#### Correcciones

- **`hosts --app <inexistente> -o ndjson`** emitía un `\n` colgado (1 byte) en vez de salida vacía;
  ahora son **0 bytes** exactos (NDJSON limpio para pipelines).
- **`-n/--limit <N>` ahora aplica a `json/csv/ndjson`,** no solo a la tabla: los formatos máquina emiten
  las N primeras filas. La fila `Σ total` de la tabla sigue sumando **todas** las filas del rango (total
  real), no solo las N mostradas.

## [0.7.0] — 2026-07-03

### CLI (comandos de un-disparo: `ping/now/total/top/hosts`)

Arranca el arco **0.7.x = CLI**: la CLI de un-disparo se vuelve **profesional y scriptable** (cliente-only,
sin cambios en el daemon ni el protocolo).

- **Formatos de salida `-o/--output <table|json|csv|ndjson>`** (default `table`): CSV para hojas de
  cálculo, NDJSON (una fila por línea, bytes crudos) para pipelines con `jq`. `--json` se conserva como
  **alias** de `-o json`. La salida JSON tabular lleva `"schema": 1` como contrato estable.
- **Códigos de salida diferenciados** (para cron/systemd/scripts):

  | Código | Significado |
  |---|---|
  | `0` | OK |
  | `2` | uso inválido de flags (lo emite clap) |
  | `3` | daemon inalcanzable (socket ausente / sin permiso) |
  | `4` | error de negocio (el daemon respondió `Error`) |
  | `5` | rango/argumento inválido |
  | `1` | fallback genérico |

  Con `-o json`, los errores salen estructurados: `{"error":{"kind":"…","message":"…"}}`.
- **Unidades `-u/--units <binary|decimal|bits>`** en las tablas (coherente con la tecla `u` de la TUI).
- **Rango unificado en `top`:** acepta `<RANGO>` posicional como `total`/`hosts`; `--rango` se mantiene
  como **forma equivalente** (retro-compatible con 0.6.x).
- **`-n/--limit <N>`** en `top`/`hosts` para acotar filas; la fila **`Σ total` refleja el total real** de
  todo el rango, no el de las N mostradas.
- **Ayuda de tokens de rango** en el `--help` de `total`/`top`/`hosts` y en el mensaje de error al parsear.
- **`--quiet`** (solo datos), **`--no-color`** (+ respeta `NO_COLOR`), **alineación numérica a la derecha**.
- **`CAUDALGHOST_SOCKET`** como fallback de `--socket` (precedencia flag > env > default XDG).
- **Empaquetado:** completions **bash/zsh/fish** (`clap_complete`) y **man page** (`clap_mangen`)
  generadas desde la misma definición de la CLI y empaquetadas en **ambas** ediciones `.deb`.

#### Comportamientos conocidos
- **`--all` se retiró de `top`/`hosts`** (era un no-op: esas filas no tienen interfaz que filtrar); sigue
  en `now`/`total`/`live`.
- **`ping`/`now` soportan `table` y `json`** (no `csv`/`ndjson`, que devuelven error claro); su `-o json`
  es el "sobre" crudo del `Response`, distinto del envoltorio `{"schema":1,…}` de `total/top/hosts`.
  Unificarlos del todo queda para 0.7.1.

## [0.6.5] — 2026-07-03

### TUI (`caudalghost live`)

Pulido del **resaltado por umbral** en los paneles de apps e interfaces (colorea la fila cuando una
entidad está más activa de lo normal *para ella misma*).

- **Suavizado (10 s):** el nivel se calcula sobre la media móvil de los últimos 10 s comparada con los
  percentiles de la propia serie **igualmente suavizada** (sostenido vs sostenido). El color deja de
  parpadear de segundo a segundo y refleja actividad sostenida, no un pico de un solo segundo.
- **Sin rojo alarmista:** el nivel "alto" pasa de rojo (poco legible como texto) a un **naranja cálido**
  (`#F28C28`); el "elevado" se queda en amarillo; una entidad tranquila no se colorea. El estado
  "desconectado" de la cabecera sigue en rojo (ese sí es un error real).

## [0.6.4] — 2026-07-03

### TUI (`caudalghost live`)

Rediseño del layout: los **servidores** vuelven a la vista inicial y el histórico deja de duplicar cada
panel (cliente-only).

- **Banda de servidores:** panel nuevo, siempre visible, con el **top de servidores de todas las apps**
  (`ahora` agrega el tráfico en vivo por host; `histórico` usa el registro del rango activo).
- **Toggle de vista por panel:** cada panel (**apps · interfaces · servidores**) alterna
  **`ahora` ↔ `histórico`** con **`v`** sobre el panel enfocado (`Tab`). El histórico arranca en `todo`
  (todo el registro) y `r` lo cicla a `hoy/ayer/7d/30d`.
- **Layout de 3 bandas:** gráficos · `apps | interfaces` · `servidores`. El foco pasa de 5 a 4 paneles
  (`Gráficos · Apps · Interfaces · Servidores`), en vez de bandas "ahora"/"histórico" separadas.
- Drill-down (`→`/`l`), scrub en pausa, ejes del gráfico y filtro (`/`) se conservan.

## [0.6.3] — 2026-07-03

### TUI (`caudalghost live`)

Drill-down de una app a sus servidores y selector de rango del histórico (cliente-only).

- **Drill-down app→hosts:** con una app seleccionada (`j`/`k`) en `apps · ahora` o `apps · histórico`,
  `→` / `l` entra a **los servidores a los que consume esa app** (en vivo desde `Sample.flows`; histórico
  desde `TopRemotes` del rango). `←` / `Esc` vuelve a la lista de apps. El pie muestra el modo `[HOSTS]`.
- **Selector de rango del histórico:** `r` cicla `todo → hoy → ayer → 7d → 30d`; el rango activo se
  muestra en los títulos de los paneles de histórico (p. ej. `apps · histórico · 7d`) y afecta a apps e
  interfaces históricos juntas.

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
