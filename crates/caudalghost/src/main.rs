mod client;
mod live;
mod render;

use caudalghost_core::proto::{Request, Response};
use caudalghost_core::range;
use clap::{CommandFactory, Parser, Subcommand};
use render::Formato;
use std::io::IsTerminal;
use std::path::PathBuf;

/// CaudalGhost — monitor de uso de red. Cliente de consulta.
#[derive(Parser)]
#[command(
    name = "caudalghost",
    version = long_version(),
    after_help = "Ejemplos:\n  \
                  caudalghost total hoy -o csv\n  \
                  caudalghost top 7d -n 10\n  \
                  caudalghost hosts hoy --app firefox -u bits"
)]
struct Cli {
    /// Socket del daemon (default: XDG runtime)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

/// Flags de salida (`-o/-u/--json/-q/--no-color`), aplanados **solo** en los
/// subcommandos de datos (`ping/now/total/top/hosts`). `live` no los lleva:
/// la TUI los ignora y no tiene sentido listarlos en su `--help`.
#[derive(clap::Args, Clone)]
struct OpcionesSalida {
    /// Formato de salida
    #[arg(short = 'o', long, value_enum, default_value_t = Formato::Table)]
    output: Formato,
    /// Unidades para las tasas/tamaños en las tablas
    #[arg(short = 'u', long, value_enum, default_value_t = render::UnitMode::Binary)]
    units: render::UnitMode,
    /// Salida JSON cruda (para scripts y widgets). Alias de -o json.
    #[arg(long)]
    json: bool,
    /// Omite título/decoración: solo datos
    #[arg(short = 'q', long)]
    quiet: bool,
    /// Desactiva color (también respeta NO_COLOR)
    #[arg(long)]
    no_color: bool,
}

impl OpcionesSalida {
    /// Formato efectivo: `--json` fuerza `-o json` (alias retro-compatible).
    fn formato(&self) -> Formato {
        if self.json {
            Formato::Json
        } else {
            self.output
        }
    }

    /// Color habilitado: ni `--no-color`, ni `NO_COLOR` en el entorno, y stdout es TTY.
    fn color(&self) -> bool {
        if self.no_color || std::env::var_os("NO_COLOR").is_some() {
            return false;
        }
        std::io::stdout().is_terminal()
    }
}

impl Cli {
    /// Opciones de salida del subcomando activo (`None` para `Live`, que no
    /// las lleva).
    fn salida(&self) -> Option<&OpcionesSalida> {
        match &self.cmd {
            Cmd::Ping { salida } => Some(salida),
            Cmd::Now { salida, .. } => Some(salida),
            Cmd::Total { salida, .. } => Some(salida),
            Cmd::Top { salida, .. } => Some(salida),
            Cmd::Hosts { salida, .. } => Some(salida),
            Cmd::Live { .. } | Cmd::Completions { .. } | Cmd::Man => None,
        }
    }

    /// Formato efectivo del subcomando activo; `Table` por defecto si no
    /// lleva `OpcionesSalida` (p.ej. `live`, que no lo usa para render de datos).
    fn formato(&self) -> Formato {
        self.salida().map(|s| s.formato()).unwrap_or(Formato::Table)
    }

    /// Unidades del subcomando activo (default `Binary` si no aplica).
    fn units(&self) -> render::UnitMode {
        self.salida()
            .map(|s| s.units)
            .unwrap_or(render::UnitMode::Binary)
    }

    /// `--quiet` del subcomando activo (default `false` si no aplica).
    fn quiet(&self) -> bool {
        self.salida().map(|s| s.quiet).unwrap_or(false)
    }

    /// Color habilitado del subcomando activo (default: color de terminal
    /// si no aplica `OpcionesSalida`).
    fn color(&self) -> bool {
        self.salida().map(|s| s.color()).unwrap_or_else(|| {
            std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal()
        })
    }
}

/// Error de salida clasificado: determina el exit code y, si la salida
/// pedida es JSON, se re-emite como `{"error":{"kind":"...","message":"..."}}`
/// en vez de un `panic`/mensaje crudo de Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SalidaError {
    /// Uso incorrecto de flags/argumentos (normalmente ya lo ataja clap, que
    /// sale con su propio exit 2 antes de llegar aquí; se conserva en el
    /// enum para que el contrato de exit codes esté completo y disponible a
    /// futuras validaciones manuales de uso).
    #[allow(dead_code)]
    Uso,
    /// El daemon no responde: socket no encontrado, sin permiso, etc.
    /// Lleva el mensaje accionable ya construido por `caudalghost_core::client`.
    DaemonInalcanzable(String),
    /// El daemon respondió `Response::Error { message }`.
    ErrorNegocio(String),
    /// Rango/argumento temporal inválido (`range::resolve` falló).
    RangoInvalido(String),
    /// Cualquier otro fallo no clasificado.
    Otro(String),
}

impl SalidaError {
    /// Exit code por tipo: 2 uso, 3 daemon inalcanzable, 4 error de negocio,
    /// 5 rango inválido, 1 cualquier otro.
    fn exit_code(&self) -> i32 {
        match self {
            SalidaError::Uso => 2,
            SalidaError::DaemonInalcanzable(_) => 3,
            SalidaError::ErrorNegocio(_) => 4,
            SalidaError::RangoInvalido(_) => 5,
            SalidaError::Otro(_) => 1,
        }
    }

    /// Etiqueta corta estable para el campo `"kind"` del JSON de error.
    fn kind(&self) -> &'static str {
        match self {
            SalidaError::Uso => "uso",
            SalidaError::DaemonInalcanzable(_) => "daemon_inalcanzable",
            SalidaError::ErrorNegocio(_) => "error_negocio",
            SalidaError::RangoInvalido(_) => "rango_invalido",
            SalidaError::Otro(_) => "otro",
        }
    }

    /// Mensaje humano (sin comillas JSON) para la salida por defecto (no-JSON).
    fn mensaje_humano(&self) -> String {
        match self {
            SalidaError::Uso => "uso inválido".to_string(),
            SalidaError::DaemonInalcanzable(m)
            | SalidaError::ErrorNegocio(m)
            | SalidaError::RangoInvalido(m)
            | SalidaError::Otro(m) => m.clone(),
        }
    }

    #[cfg(test)]
    fn daemon_inalcanzable_de_prueba() -> Self {
        SalidaError::DaemonInalcanzable("daemon inalcanzable".to_string())
    }

    /// Representación JSON de una línea: `{"error":{"kind":"...","message":"..."}}`.
    fn a_json(&self) -> String {
        serde_json::to_string(&serde_json::json!({
            "error": {
                "kind": self.kind(),
                "message": self.mensaje_humano(),
            }
        }))
        .unwrap_or_else(|_| "{\"error\":{\"kind\":\"otro\",\"message\":\"error\"}}".to_string())
    }
}

impl std::fmt::Display for SalidaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.mensaje_humano())
    }
}

/// Clasifica el fallo de `client::call` (fallo de conexión al socket) usando
/// `caudalghost_core::client::ConexionError` cuando está disponible en la
/// cadena de causas; cualquier otro fallo (I/O de la petición, etc.) cae en
/// `SalidaError::Otro`.
fn clasifica_error_conexion(e: anyhow::Error) -> SalidaError {
    use caudalghost_core::client::ConexionError;
    if e.downcast_ref::<ConexionError>().is_some() {
        SalidaError::DaemonInalcanzable(e.to_string())
    } else {
        SalidaError::Otro(e.to_string())
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Verifica conexión con el daemon
    Ping {
        #[command(flatten)]
        salida: OpcionesSalida,
    },
    /// Actividad de los últimos segundos
    Now {
        #[arg(long, default_value_t = 60)]
        seconds: u32,
        /// Incluir loopback
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        salida: OpcionesSalida,
    },
    /// Totales por rango
    Total {
        /// Rango histórico. Tokens válidos (ver más abajo).
        #[arg(value_name = "RANGO", long_help = ayuda_rango())]
        rango: String,
        /// Incluir loopback
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        salida: OpcionesSalida,
    },
    /// Top de aplicaciones: por rango histórico o ventana en vivo
    #[command(group(
        clap::ArgGroup::new("rango_top")
            .args(["rango_pos", "rango_flag"])
            .multiple(false)
    ))]
    Top {
        /// Rango histórico. Excluyente con --rango y --seconds. Tokens válidos (ver más abajo).
        #[arg(value_name = "RANGO", conflicts_with = "seconds", long_help = ayuda_rango())]
        rango_pos: Option<String>,
        /// Rango histórico, forma equivalente al posicional. Excluyente con --seconds.
        #[arg(long = "rango", value_name = "RANGO", conflicts_with = "seconds", long_help = ayuda_rango())]
        rango_flag: Option<String>,
        /// Ventana en vivo en segundos (excluyente con el rango histórico) [default: 60]
        #[arg(long)]
        seconds: Option<u32>,
        /// Limita a las primeras N filas (tras ordenar por total desc)
        #[arg(short = 'n', long = "limit", value_name = "N")]
        limit: Option<usize>,
        #[command(flatten)]
        salida: OpcionesSalida,
    },
    /// Top de hosts remotos por rango; opcionalmente filtrado por app
    Hosts {
        /// Rango histórico. Tokens válidos (ver más abajo).
        #[arg(value_name = "RANGO", long_help = ayuda_rango())]
        rango: String,
        /// Filtrar por app (p. ej. firefox)
        #[arg(long)]
        app: Option<String>,
        /// Limita a las primeras N filas (tras ordenar por total desc)
        #[arg(short = 'n', long = "limit", value_name = "N")]
        limit: Option<usize>,
        #[command(flatten)]
        salida: OpcionesSalida,
    },
    /// Dashboard interactivo en vivo (TUI pantalla completa)
    Live {
        /// Ventana de historia del gráfico en segundos
        #[arg(long, default_value_t = 600)]
        window: u32,
        /// Incluir loopback (lo)
        #[arg(long)]
        all: bool,
        /// Auto-prueba headless: renderiza un frame y sale (sin TTY)
        #[arg(long, hide = true)]
        self_test: bool,
    },
    /// Genera el script de completado de shell (uso interno de empaquetado)
    #[command(hide = true)]
    Completions {
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
    /// Genera la página de manual en roff (uso interno de empaquetado)
    #[command(hide = true)]
    Man,
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(&cli) {
        if cli.formato() == Formato::Json {
            eprintln!("{}", e.a_json());
        } else {
            eprintln!("error: {}", e.mensaje_humano());
        }
        std::process::exit(e.exit_code());
    }
}

/// Llama al socket y clasifica un fallo de conexión como `DaemonInalcanzable`
/// (vía `caudalghost_core::client::ConexionError`) en vez de dejarlo como un
/// error genérico.
fn llamar(socket: &std::path::Path, req: &Request) -> Result<Response, SalidaError> {
    client::call(socket, req).map_err(clasifica_error_conexion)
}

/// Resuelve un token de rango o lo reporta como `RangoInvalido`.
fn resolver_rango(rango: &str) -> Result<range::Window, SalidaError> {
    range::resolve(rango, chrono::Local::now()).map_err(SalidaError::RangoInvalido)
}

/// Resuelve el socket a usar con precedencia `--socket` > `CAUDALGHOST_SOCKET`
/// (env) > default XDG (`resolve_client_socket`).
fn resolver_socket(flag: Option<PathBuf>, env: Option<String>) -> PathBuf {
    flag.or_else(|| env.map(PathBuf::from))
        .unwrap_or_else(caudalghost_core::paths::resolve_client_socket)
}

/// Texto de ayuda extendida para el argumento `<RANGO>`: lista los tokens
/// válidos desde la fuente única `range::tokens_ayuda()` (help + error de
/// `resolve` comparten la misma lista).
fn ayuda_rango() -> String {
    format!("Tokens válidos: {}", range::tokens_ayuda())
}

/// `--version` "rico": versión de Cargo.toml + hash corto de git + fecha de
/// build, inyectados en tiempo de compilación por `build.rs` vía
/// `CG_GIT_HASH`/`CG_BUILD_DATE`. `build.rs` obtiene la fecha con
/// `date -u +%Y-%m-%d` (o la deriva de `SOURCE_DATE_EPOCH` si está presente,
/// para builds reproducibles); si `git`/`date` no están disponibles al
/// compilar, degrada esos campos a `"desconocido"` sin fallar el build; en
/// tiempo de ejecución esta función nunca invoca `git`/`date`, solo lee las
/// env compiladas.
fn long_version() -> &'static str {
    const VERSION: &str = env!("CARGO_PKG_VERSION");
    const HASH: &str = env!("CG_GIT_HASH");
    const FECHA: &str = env!("CG_BUILD_DATE");
    Box::leak(format!("{VERSION} ({HASH} {FECHA})").into_boxed_str())
}

/// Rango efectivo de `top`: posicional o `--rango` (mutuamente excluyentes vía
/// `ArgGroup`, así que a lo sumo uno de los dos está presente).
fn rango_de_top(cli: &Cli) -> Option<String> {
    match &cli.cmd {
        Cmd::Top {
            rango_pos,
            rango_flag,
            ..
        } => rango_pos.clone().or_else(|| rango_flag.clone()),
        _ => None,
    }
}

/// Si la respuesta es `Response::Error`, la convierte en `SalidaError::ErrorNegocio`.
fn rechaza_si_error(resp: &Response) -> Result<(), SalidaError> {
    if let Response::Error { message } = resp {
        Err(SalidaError::ErrorNegocio(message.clone()))
    } else {
        Ok(())
    }
}

/// Imprime la salida de `render::emitir` seguida de su salto de línea final
/// — pero solo si `s` no está vacío. `emitir` devuelve `""` en `-o csv`/`-o
/// ndjson` cuando no hay filas (ndjson: `join("\n")` de un iterador vacío);
/// sin este guard, un `print!("{s}"); println!();` a ciegas deja un `\n`
/// suelto (1 byte) en vez de una salida de 0 bytes. Con `s` no vacío el
/// comportamiento es idéntico al `print!`/`println!` de siempre.
fn imprimir(s: &str) {
    print!("{s}");
    if !s.is_empty() {
        println!();
    }
}

/// Cuerpo real de la CLI, ya sin `main()`/`process::exit` de por medio: fácil
/// de probar y de envolver en la clasificación de `SalidaError`.
fn run(cli: &Cli) -> Result<(), SalidaError> {
    let socket = resolver_socket(cli.socket.clone(), std::env::var("CAUDALGHOST_SOCKET").ok());
    match &cli.cmd {
        Cmd::Ping { .. } => {
            let resp = llamar(&socket, &Request::Ping)?;
            if matches!(cli.formato(), Formato::Csv | Formato::Ndjson) {
                return Err(SalidaError::Otro(
                    "este comando no admite -o csv/ndjson (usa table o json); csv/ndjson aplican a total/top/hosts"
                        .into(),
                ));
            }
            rechaza_si_error(&resp)?;
            if let Response::Pong { version, ebpf } = &resp {
                if cli.formato() == Formato::Json {
                    println!("{}", ping_json(version, *ebpf));
                } else {
                    let estado = if *ebpf {
                        "activo"
                    } else {
                        "inactivo (solo interfaces)"
                    };
                    println!("caudalghostd v{version} responde ✓ · eBPF: {estado}");
                }
            }
            Ok(())
        }
        Cmd::Now { seconds, all, .. } => {
            let resp = llamar(&socket, &Request::Snapshot { seconds: *seconds })?;
            rechaza_si_error(&resp)?;
            if let Response::Snapshot { samples } = &resp {
                if cli.formato() == Formato::Table {
                    let secs = samples.len().max(1) as u64;
                    let filas: Vec<_> = agrega_series(samples)
                        .into_iter()
                        .filter(|(i, _, _, _)| *all || i != "lo")
                        .collect();
                    if !cli.quiet() {
                        let titulo_txt = format!("⏱ últimos {secs} s");
                        if cli.color() {
                            println!("{}", render::titulo(&titulo_txt));
                        } else {
                            println!("{titulo_txt}");
                        }
                    }
                    println!("{}", render::tabla_now(&filas, secs, cli.units()));
                } else {
                    imprimir(&render::emitir(
                        &filas_now_maquina(samples, *all),
                        cli.formato(),
                        cli.units(),
                        cli.quiet(),
                        cli.color(),
                        "⏱ now",
                        None,
                    ));
                }
            }
            Ok(())
        }
        Cmd::Total { rango, all, .. } => {
            let w = resolver_rango(rango)?;
            let resp = llamar(
                &socket,
                &Request::Total {
                    start: w.start,
                    end: w.end,
                },
            )?;
            rechaza_si_error(&resp)?;
            if let Response::Total { rows } = &resp {
                let filtradas: Vec<_> = rows
                    .iter()
                    .filter(|r| *all || r.iface != "lo")
                    .cloned()
                    .collect();
                imprimir(&render::emitir(
                    &filtradas,
                    cli.formato(),
                    cli.units(),
                    cli.quiet(),
                    cli.color(),
                    &format!("Σ rango: {rango}"),
                    None,
                ));
            }
            Ok(())
        }
        Cmd::Top { seconds, limit, .. } => {
            let rango = rango_de_top(cli);
            match (rango, seconds) {
                (Some(rango), None) => {
                    let w = resolver_rango(&rango)?;
                    let resp = llamar(
                        &socket,
                        &Request::TotalApps {
                            start: w.start,
                            end: w.end,
                        },
                    )?;
                    rechaza_si_error(&resp)?;
                    if let Response::TotalApps { rows } = &resp {
                        let mut ordenadas = rows.clone();
                        ordenadas.sort_by_key(|r| std::cmp::Reverse(r.rx + r.tx));
                        imprimir(&render::emitir(
                            &ordenadas,
                            cli.formato(),
                            cli.units(),
                            cli.quiet(),
                            cli.color(),
                            &format!("⛰ top apps · rango: {rango}"),
                            *limit,
                        ));
                    }
                    Ok(())
                }
                (None, secs) => {
                    let seconds = secs.unwrap_or(60);
                    let resp = llamar(&socket, &Request::Snapshot { seconds })?;
                    rechaza_si_error(&resp)?;
                    if let Response::Snapshot { samples } = &resp {
                        let filas = agrega_apps(samples);
                        let rows: Vec<caudalghost_core::proto::AppRow> = filas
                            .into_iter()
                            .map(|(app, rx, tx)| caudalghost_core::proto::AppRow { app, rx, tx })
                            .collect();
                        let etiqueta = format!("últimos {seconds} s");
                        imprimir(&render::emitir(
                            &rows,
                            cli.formato(),
                            cli.units(),
                            cli.quiet(),
                            cli.color(),
                            &format!("⛰ top apps · {etiqueta}"),
                            *limit,
                        ));
                    }
                    Ok(())
                }
                (Some(_), Some(_)) => unreachable!("clap conflicts_with lo impide"),
            }
        }
        Cmd::Hosts {
            rango, app, limit, ..
        } => {
            let w = resolver_rango(rango)?;
            let resp = llamar(
                &socket,
                &Request::TopRemotes {
                    start: w.start,
                    end: w.end,
                    app: app.clone(),
                },
            )?;
            rechaza_si_error(&resp)?;
            if let Response::TopRemotes { rows } = &resp {
                if rows.is_empty() && cli.formato() == Formato::Table {
                    if let Some(a) = app {
                        eprintln!("{}", aviso_sin_datos_app(a, rango));
                        return Ok(());
                    }
                }
                let mut ordenados = rows.clone();
                ordenados.sort_by_key(|r| std::cmp::Reverse(r.rx + r.tx));
                let etiqueta = match app {
                    Some(a) => format!("🌐 hosts · {a} · rango: {rango}"),
                    None => format!("🌐 hosts · rango: {rango}"),
                };
                imprimir(&render::emitir(
                    &ordenados,
                    cli.formato(),
                    cli.units(),
                    cli.quiet(),
                    cli.color(),
                    &etiqueta,
                    *limit,
                ));
            }
            Ok(())
        }
        Cmd::Live {
            window,
            all,
            self_test,
        } => {
            let r = if *self_test {
                live::self_test()
            } else {
                live::run(&socket, *window, *all)
            };
            r.map_err(|e| SalidaError::Otro(e.to_string()))
        }
        Cmd::Completions { shell } => {
            print!("{}", generar_completions(*shell));
            Ok(())
        }
        Cmd::Man => {
            print!("{}", generar_man());
            Ok(())
        }
    }
}

/// Genera el script de completado para `shell` a partir de la definición de
/// `Cli` (fuente única: no hay que mantener el listado de subcomandos/flags
/// a mano). Usado por el subcommand oculto `completions` y por `build-debs.sh`.
fn generar_completions(shell: clap_complete::Shell) -> String {
    let mut cmd = Cli::command();
    let nombre = cmd.get_name().to_string();
    let mut buf = Vec::new();
    clap_complete::generate(shell, &mut cmd, nombre, &mut buf);
    String::from_utf8_lossy(&buf).into_owned()
}

/// Genera la página de manual (roff) a partir de la misma definición de `Cli`.
/// Usado por el subcommand oculto `man` y por `build-debs.sh`.
fn generar_man() -> String {
    let cmd = Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut buf = Vec::new();
    man.render(&mut buf)
        .expect("render de man no debe fallar sobre un buffer en memoria");
    String::from_utf8_lossy(&buf).into_owned()
}

/// Filas "máquina" de `now`: totales agregados por interfaz (sin la serie del
/// sparkline, que solo tiene sentido en la tabla humana), listas para pasar
/// por `render::emitir` en `-o json/csv/ndjson`. `all` conserva `lo` si `true`.
fn filas_now_maquina(
    samples: &[caudalghost_core::types::Sample],
    all: bool,
) -> Vec<caudalghost_core::proto::TotalRow> {
    agrega_series(samples)
        .into_iter()
        .filter(|(i, _, _, _)| all || i != "lo")
        .map(|(iface, rx, tx, _serie)| caudalghost_core::proto::TotalRow { iface, rx, tx })
        .collect()
}

/// Envoltorio JSON consistente para `ping -o json`: mismo contrato `"schema":1`
/// que el resto de la CLI, en vez del `Response` crudo del daemon.
fn ping_json(version: &str, ebpf: bool) -> String {
    serde_json::json!({"schema": 1, "version": version, "ebpf": ebpf}).to_string()
}

/// Mensaje de aviso cuando `hosts <rango> --app X` no devuelve filas: evita
/// que el usuario vea una tabla vacía sin contexto de qué se filtró.
fn aviso_sin_datos_app(app: &str, rango: &str) -> String {
    format!("sin datos para la app '{app}' en el rango {rango}")
}

/// Colapsa los AppDelta de una ventana de samples, ordenados por total desc.
fn agrega_apps(samples: &[caudalghost_core::types::Sample]) -> Vec<(String, u64, u64)> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, (u64, u64)> = HashMap::new();
    for s in samples {
        for a in &s.apps {
            let e = acc.entry(a.app.clone()).or_default();
            e.0 += a.rx;
            e.1 += a.tx;
        }
    }
    let mut v: Vec<(String, u64, u64)> = acc.into_iter().map(|(k, (rx, tx))| (k, rx, tx)).collect();
    v.sort_by_key(|(_, rx, tx)| std::cmp::Reverse(rx + tx));
    v
}

/// Agrega samples a (iface, rx_total, tx_total, serie rx+tx por segundo).
/// Las series quedan alineadas: toda iface tiene un valor por sample (0 si no apareció).
fn agrega_series(samples: &[caudalghost_core::types::Sample]) -> Vec<(String, u64, u64, Vec<u64>)> {
    use std::collections::BTreeMap;
    let mut tot: BTreeMap<String, (u64, u64, Vec<u64>)> = BTreeMap::new();
    for (i, s) in samples.iter().enumerate() {
        for d in &s.deltas {
            let e = tot.entry(d.iface.clone()).or_default();
            e.0 += d.rx;
            e.1 += d.tx;
        }
        for (iface, e) in tot.iter_mut() {
            // alinear: rellena con ceros los samples en que la iface no existía
            e.2.resize(i, 0);
            let v = s
                .deltas
                .iter()
                .find(|d| &d.iface == iface)
                .map(|d| d.rx + d.tx)
                .unwrap_or(0);
            e.2.push(v);
        }
    }
    tot.into_iter()
        .map(|(i, (rx, tx, serie))| (i, rx, tx, serie))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use caudalghost_core::types::{IfaceDelta, Sample};

    fn d(iface: &str, rx: u64, tx: u64) -> IfaceDelta {
        IfaceDelta {
            iface: iface.into(),
            rx,
            tx,
        }
    }

    #[test]
    fn agrega_apps_de_snapshot() {
        use caudalghost_core::types::{AppDelta, Sample};
        let samples = vec![
            Sample {
                ts: 0,
                deltas: vec![],
                apps: vec![AppDelta {
                    app: "firefox".into(),
                    rx: 10,
                    tx: 1,
                }],
                flows: vec![],
            },
            Sample {
                ts: 1,
                deltas: vec![],
                apps: vec![],
                flows: vec![],
            }, // tick sin drenado
            Sample {
                ts: 2,
                deltas: vec![],
                apps: vec![
                    AppDelta {
                        app: "firefox".into(),
                        rx: 20,
                        tx: 2,
                    },
                    AppDelta {
                        app: "curl".into(),
                        rx: 5,
                        tx: 5,
                    },
                ],
                flows: vec![],
            },
        ];
        let filas = agrega_apps(&samples);
        assert_eq!(filas.len(), 2);
        // ordenadas por total desc
        assert_eq!(filas[0].0, "firefox");
        assert_eq!((filas[0].1, filas[0].2), (30, 3));
        assert_eq!((filas[1].1, filas[1].2), (5, 5));
    }

    #[test]
    fn exit_code_por_tipo_de_error() {
        use super::SalidaError;
        assert_eq!(SalidaError::Uso.exit_code(), 2);
        assert_eq!(SalidaError::daemon_inalcanzable_de_prueba().exit_code(), 3);
        assert_eq!(SalidaError::ErrorNegocio("x".into()).exit_code(), 4);
        assert_eq!(SalidaError::RangoInvalido("mal".into()).exit_code(), 5);
        assert_eq!(SalidaError::Otro("x".into()).exit_code(), 1);
    }

    #[test]
    fn long_version_incluye_version() {
        let v = super::long_version();
        assert!(v.starts_with(env!("CARGO_PKG_VERSION")));
        // no entra en pánico y trae los tres campos (hash/fecha pueden ser "desconocido")
        assert!(v.contains('(') && v.contains(')'));
    }

    #[test]
    fn error_a_json_estructurado() {
        use super::SalidaError;
        let j = SalidaError::RangoInvalido("rango-malo".into()).a_json();
        assert!(j.contains("\"error\"") && j.contains("rango-malo"));
    }

    /// `Response::Error` del daemon se clasifica como `ErrorNegocio` (exit 4),
    /// listo para re-emitirse como JSON de error si `-o json` — nunca un
    /// `bail!`/panic de Rust sin estructurar.
    #[test]
    fn response_error_se_clasifica_como_error_negocio() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;

        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("fake.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut s = stream;
            s.write_all(b"{\"type\":\"error\",\"message\":\"algo salio mal\"}\n")
                .unwrap();
        });

        let cli = Cli::try_parse_from(["caudalghost", "--socket", sock.to_str().unwrap(), "ping"])
            .unwrap();
        let err = run(&cli).expect_err("Response::Error debe devolver Err");
        assert_eq!(err, SalidaError::ErrorNegocio("algo salio mal".to_string()));
        assert_eq!(err.exit_code(), 4);
        let j = err.a_json();
        assert!(j.contains("\"error_negocio\"") && j.contains("algo salio mal"));
    }

    /// Socket inexistente (NotFound) → `DaemonInalcanzable` (exit 3), usando
    /// la clasificación de `caudalghost_core::client::ConexionError`.
    #[test]
    fn socket_inexistente_se_clasifica_como_daemon_inalcanzable() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("no-existe.sock");
        let cli = Cli::try_parse_from(["caudalghost", "--socket", sock.to_str().unwrap(), "ping"])
            .unwrap();
        let err = run(&cli).expect_err("socket inexistente debe devolver Err");
        assert_eq!(err.exit_code(), 3);
        assert!(matches!(err, SalidaError::DaemonInalcanzable(_)));
    }

    /// `ping` con `-o csv`/`-o ndjson` se rechaza con un mensaje claro (no es
    /// tabular, no pasa por `render::emitir`).
    #[test]
    fn ping_rechaza_csv_y_ndjson() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;

        fn responde_pong(sock: &std::path::Path) {
            let listener = UnixListener::bind(sock).unwrap();
            std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut s = stream;
                s.write_all(b"{\"type\":\"pong\",\"version\":\"0.0.0\",\"ebpf\":false}\n")
                    .unwrap();
            });
        }

        for formato in ["csv", "ndjson"] {
            let dir = tempfile::tempdir().unwrap();
            let sock = dir.path().join("fake.sock");
            responde_pong(&sock);
            let cli = Cli::try_parse_from([
                "caudalghost",
                "--socket",
                sock.to_str().unwrap(),
                "ping",
                "-o",
                formato,
            ])
            .unwrap();
            let err = run(&cli).expect_err(&format!("ping -o {formato} debe devolver Err"));
            assert!(
                matches!(err, SalidaError::Otro(_)),
                "formato {formato}: {err:?}"
            );
            assert_eq!(err.exit_code(), 1);
            assert!(err.mensaje_humano().contains("csv/ndjson"));
        }
    }

    /// `now` con `-o csv`/`-o ndjson` ya no se rechaza: pasa por `render::emitir`
    /// (vía `filas_now_maquina`) igual que `total/top/hosts`.
    #[test]
    fn now_acepta_csv_y_ndjson() {
        use std::io::{BufRead, BufReader, Write};
        use std::os::unix::net::UnixListener;

        fn responde_snapshot(sock: &std::path::Path) {
            let listener = UnixListener::bind(sock).unwrap();
            std::thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let mut s = stream;
                s.write_all(
                    b"{\"type\":\"snapshot\",\"samples\":[{\"ts\":0,\"deltas\":[{\"iface\":\"eth0\",\"rx\":100,\"tx\":10}],\"apps\":[],\"flows\":[]}]}\n",
                )
                .unwrap();
            });
        }

        for formato in ["csv", "ndjson"] {
            let dir = tempfile::tempdir().unwrap();
            let sock = dir.path().join("fake.sock");
            responde_snapshot(&sock);
            let cli = Cli::try_parse_from([
                "caudalghost",
                "--socket",
                sock.to_str().unwrap(),
                "now",
                "-o",
                formato,
            ])
            .unwrap();
            run(&cli).unwrap_or_else(|e| panic!("now -o {formato} no debe fallar: {e:?}"));
        }
    }

    /// Rango inválido en `total` → `RangoInvalido` (exit 5), sin llegar a tocar el socket.
    #[test]
    fn rango_invalido_en_total_es_exit_5() {
        let cli = Cli::try_parse_from(["caudalghost", "total", "no-es-un-rango"]).unwrap();
        let err = run(&cli).expect_err("rango inválido debe devolver Err");
        assert_eq!(err.exit_code(), 5);
        assert!(matches!(err, SalidaError::RangoInvalido(_)));
    }

    #[test]
    fn series_alineadas_con_iface_tardia() {
        let samples = vec![
            Sample {
                ts: 0,
                deltas: vec![d("eth0", 10, 5)],
                apps: vec![],
                flows: vec![],
            },
            Sample {
                ts: 1,
                deltas: vec![d("eth0", 20, 5), d("wlan0", 100, 50)],
                apps: vec![],
                flows: vec![],
            },
            Sample {
                ts: 2,
                deltas: vec![d("eth0", 30, 5), d("wlan0", 200, 50)],
                apps: vec![],
                flows: vec![],
            },
        ];
        let filas = agrega_series(&samples);
        assert_eq!(filas.len(), 2);
        let eth = &filas[0];
        let wlan = &filas[1];
        assert_eq!(eth.3, vec![15, 25, 35]);
        assert_eq!(
            wlan.3,
            vec![0, 150, 250],
            "iface tardía debe llevar 0 inicial"
        );
        assert_eq!((wlan.1, wlan.2), (300, 100));
    }

    #[test]
    fn top_acepta_posicional_y_flag_equivalentes() {
        let a = Cli::try_parse_from(["caudalghost", "top", "hoy"]).unwrap();
        let b = Cli::try_parse_from(["caudalghost", "top", "--rango", "hoy"]).unwrap();
        assert_eq!(rango_de_top(&a), Some("hoy".to_string()));
        assert_eq!(rango_de_top(&b), Some("hoy".to_string()));
        // posicional + --rango a la vez => error (ArgGroup)
        assert!(Cli::try_parse_from(["caudalghost", "top", "hoy", "--rango", "ayer"]).is_err());
    }

    #[test]
    fn top_y_hosts_aceptan_limit_pero_no_all() {
        assert!(Cli::try_parse_from(["caudalghost", "top", "hoy", "-n", "5"]).is_ok());
        assert!(Cli::try_parse_from(["caudalghost", "hosts", "hoy", "--limit", "5"]).is_ok());
        // `--all` no aplica: AppRow/RemoteRow no tienen interfaz que filtrar.
        assert!(Cli::try_parse_from(["caudalghost", "top", "hoy", "--all"]).is_err());
        assert!(Cli::try_parse_from(["caudalghost", "hosts", "hoy", "--all"]).is_err());
    }

    #[test]
    fn genera_completions_bash() {
        let s = super::generar_completions(clap_complete::Shell::Bash);
        assert!(s.contains("caudalghost"));
    }

    #[test]
    fn now_maquina_por_emitir_con_schema() {
        let samples = vec![Sample {
            ts: 0,
            deltas: vec![IfaceDelta {
                iface: "eth0".into(),
                rx: 100,
                tx: 10,
            }],
            apps: vec![],
            flows: vec![],
        }];
        let filas = super::filas_now_maquina(&samples, /*all=*/ false);
        assert_eq!(filas.len(), 1);
        assert_eq!(filas[0].iface, "eth0");
        // json de emitir sobre esas filas lleva "schema":1
        let j = crate::render::emitir(
            &filas,
            crate::render::Formato::Json,
            crate::render::UnitMode::Binary,
            true,
            false,
            "now",
            None,
        );
        assert!(j.contains("\"schema\""));
    }

    #[test]
    fn ping_json_lleva_schema() {
        let j = super::ping_json("1.2.3", true);
        assert!(j.contains("\"schema\"") && j.contains("1.2.3") && j.contains("ebpf"));
    }

    #[test]
    fn socket_precedencia_flag_env_default() {
        use std::path::PathBuf;
        assert_eq!(
            resolver_socket(Some(PathBuf::from("/f")), Some("/e".into())),
            PathBuf::from("/f")
        );
        assert_eq!(
            resolver_socket(None, Some("/e".into())),
            PathBuf::from("/e")
        );
        // None/None => default XDG (no vacío)
        assert!(!resolver_socket(None, None).as_os_str().is_empty());
    }

    #[test]
    fn hosts_app_vacio_da_aviso_en_texto() {
        let m = super::aviso_sin_datos_app("noexiste", "hoy");
        assert!(m.contains("noexiste") && m.contains("hoy"));
    }

    #[test]
    fn live_help_no_lista_flags_de_salida() {
        use clap::CommandFactory;
        let mut cmd = super::Cli::command();
        let mut live = cmd.find_subcommand_mut("live").unwrap().clone();
        let help = live.render_help().to_string();
        assert!(
            !help.contains("--output"),
            "live no debe listar --output: {help}"
        );
        assert!(!help.contains("--units"));
    }

    #[test]
    fn total_help_si_lista_flags_de_salida() {
        use clap::CommandFactory;
        let mut cmd = super::Cli::command();
        let mut total = cmd.find_subcommand_mut("total").unwrap().clone();
        let help = total.render_help().to_string();
        assert!(help.contains("--output") && help.contains("--units"));
    }
}
