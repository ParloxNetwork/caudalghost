mod client;
mod live;
mod render;

use caudalghost_core::proto::{Request, Response};
use caudalghost_core::range;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

/// CaudalGhost — monitor de uso de red. Cliente de consulta.
#[derive(Parser)]
#[command(name = "caudalghost", version)]
struct Cli {
    /// Socket del daemon (default: XDG runtime)
    #[arg(long, global = true)]
    socket: Option<PathBuf>,
    /// Salida JSON cruda (para scripts y widgets)
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Verifica conexión con el daemon
    Ping,
    /// Actividad de los últimos segundos
    Now {
        #[arg(long, default_value_t = 60)]
        seconds: u32,
        /// Incluir loopback
        #[arg(long)]
        all: bool,
    },
    /// Totales por rango: 5m, 1h, 24h, 7d, 30d, 90d, 365d, hoy, ayer,
    /// este-mes, este-anio, dia:YYYY-MM-DD
    Total {
        rango: String,
        /// Incluir loopback
        #[arg(long)]
        all: bool,
    },
    /// Top de aplicaciones: por rango histórico o ventana en vivo
    Top {
        /// Rango histórico (5m, hoy, 7d, ...). Excluyente con --seconds.
        #[arg(long, conflicts_with = "seconds")]
        rango: Option<String>,
        /// Ventana en vivo en segundos
        #[arg(long)]
        seconds: Option<u32>,
    },
    /// Top de hosts remotos por rango; opcionalmente filtrado por app
    Hosts {
        /// Rango histórico (5m, hoy, 7d, ...)
        rango: String,
        /// Filtrar por app (p. ej. firefox)
        #[arg(long)]
        app: Option<String>,
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let socket = cli
        .socket
        .clone()
        .unwrap_or_else(caudalghost_core::paths::resolve_client_socket);
    match &cli.cmd {
        Cmd::Ping => {
            let resp = client::call(&socket, &Request::Ping)?;
            salida(&cli, &resp, |r| {
                if let Response::Pong { version, ebpf } = r {
                    let estado = if *ebpf {
                        "activo"
                    } else {
                        "inactivo (solo interfaces)"
                    };
                    println!("caudalghostd v{version} responde ✓ · eBPF: {estado}");
                }
            })
        }
        Cmd::Now { seconds, all } => {
            let resp = client::call(&socket, &Request::Snapshot { seconds: *seconds })?;
            salida(&cli, &resp, |r| {
                if let Response::Snapshot { samples } = r {
                    let secs = samples.len().max(1) as u64;
                    let filas: Vec<_> = agrega_series(samples)
                        .into_iter()
                        .filter(|(i, _, _, _)| *all || i != "lo")
                        .collect();
                    println!("{}", render::titulo(&format!("⏱ últimos {secs} s")));
                    println!("{}", render::tabla_now(&filas, secs));
                }
            })
        }
        Cmd::Total { rango, all } => {
            let w = range::resolve(rango, chrono::Local::now()).map_err(anyhow::Error::msg)?;
            let resp = client::call(
                &socket,
                &Request::Total {
                    start: w.start,
                    end: w.end,
                },
            )?;
            salida(&cli, &resp, |r| {
                if let Response::Total { rows } = r {
                    let filtradas: Vec<_> = rows
                        .iter()
                        .filter(|r| *all || r.iface != "lo")
                        .cloned()
                        .collect();
                    println!("{}", render::titulo(&format!("Σ rango: {rango}")));
                    println!("{}", render::tabla_total(&filtradas, rango));
                }
            })
        }
        Cmd::Top { rango, seconds } => match (rango, seconds) {
            (Some(rango), None) => {
                let w = range::resolve(rango, chrono::Local::now()).map_err(anyhow::Error::msg)?;
                let resp = client::call(
                    &socket,
                    &Request::TotalApps {
                        start: w.start,
                        end: w.end,
                    },
                )?;
                salida(&cli, &resp, |r| {
                    if let Response::TotalApps { rows } = r {
                        println!(
                            "{}",
                            render::titulo(&format!("⛰ top apps · rango: {rango}"))
                        );
                        println!("{}", render::tabla_top(rows, rango));
                    }
                })
            }
            (None, secs) => {
                let seconds = secs.unwrap_or(60);
                let resp = client::call(&socket, &Request::Snapshot { seconds })?;
                salida(&cli, &resp, |r| {
                    if let Response::Snapshot { samples } = r {
                        let filas = agrega_apps(samples);
                        let rows: Vec<caudalghost_core::proto::AppRow> = filas
                            .into_iter()
                            .map(|(app, rx, tx)| caudalghost_core::proto::AppRow { app, rx, tx })
                            .collect();
                        let etiqueta = format!("últimos {seconds} s");
                        println!("{}", render::titulo(&format!("⛰ top apps · {etiqueta}")));
                        println!("{}", render::tabla_top(&rows, &etiqueta));
                    }
                })
            }
            (Some(_), Some(_)) => unreachable!("clap conflicts_with lo impide"),
        },
        Cmd::Hosts { rango, app } => {
            let w = range::resolve(rango, chrono::Local::now()).map_err(anyhow::Error::msg)?;
            let resp = client::call(
                &socket,
                &Request::TopRemotes {
                    start: w.start,
                    end: w.end,
                    app: app.clone(),
                },
            )?;
            salida(&cli, &resp, |r| {
                if let Response::TopRemotes { rows } = r {
                    let etiqueta = match app {
                        Some(a) => format!("🌐 hosts · {a} · rango: {rango}"),
                        None => format!("🌐 hosts · rango: {rango}"),
                    };
                    println!("{}", render::titulo(&etiqueta));
                    println!("{}", render::tabla_hosts(rows, rango));
                }
            })
        }
        Cmd::Live {
            window,
            all,
            self_test,
        } => {
            if *self_test {
                live::self_test()
            } else {
                live::run(&socket, *window, *all)
            }
        }
    }
}

/// JSON crudo con --json; render humano si no. Error del daemon → exit 1.
fn salida(cli: &Cli, resp: &Response, humano: impl Fn(&Response)) -> anyhow::Result<()> {
    if let Response::Error { message } = resp {
        anyhow::bail!("el daemon respondió error: {message}");
    }
    if cli.json {
        println!("{}", serde_json::to_string(resp)?);
    } else {
        humano(resp);
    }
    Ok(())
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
}
