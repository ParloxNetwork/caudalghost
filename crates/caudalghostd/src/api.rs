use crate::live::LiveRing;
use crate::storage;
use caudalghost_core::proto::{RemoteRow, Request, Response};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// Enlaza el socket Unix con permisos de grupo (0660) y lo deja escuchando.
/// El grupo lo hereda del proceso (servicio: `Group=caudalghost`). Sustituye al
/// antiguo parche 0666: ahora solo el grupo del servicio puede conectar.
pub fn bind(socket_path: &Path) -> std::io::Result<UnixListener> {
    let _ = std::fs::remove_file(socket_path); // socket huérfano de una corrida previa
    let listener = UnixListener::bind(socket_path)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(socket_path, std::fs::Permissions::from_mode(0o660))?;
    Ok(listener)
}

/// Bucle de aceptación: un hilo por conexión. El listener ya viene enlazado por
/// `bind()`, de modo que el llamador puede señalar readiness antes de aceptar.
pub fn serve(
    listener: UnixListener,
    ring: Arc<Mutex<LiveRing>>,
    db_path: String,
    ebpf_active: bool,
) -> std::io::Result<()> {
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("caudalghostd: error aceptando conexión: {e}");
                continue;
            }
        };
        let ring = ring.clone();
        let db = db_path.clone();
        std::thread::spawn(move || {
            let _ = handle(stream, &ring, &db, ebpf_active);
        });
    }
    Ok(())
}

fn handle(
    stream: UnixStream,
    ring: &Arc<Mutex<LiveRing>>,
    db_path: &str,
    ebpf_active: bool,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut writer = BufWriter::new(stream);
    let mut line = String::new();
    while reader.read_line(&mut line)? > 0 {
        let resp = match serde_json::from_str::<Request>(line.trim()) {
            Ok(req) => dispatch(req, ring, db_path, ebpf_active),
            Err(e) => Response::Error {
                message: format!("petición inválida: {e}"),
            },
        };
        let mut out = serde_json::to_string(&resp)?;
        out.push('\n');
        writer.write_all(out.as_bytes())?;
        writer.flush()?;
        line.clear();
    }
    Ok(())
}

fn dispatch(
    req: Request,
    ring: &Arc<Mutex<LiveRing>>,
    db_path: &str,
    ebpf_active: bool,
) -> Response {
    match req {
        Request::Ping => Response::Pong {
            version: env!("CARGO_PKG_VERSION").into(),
            ebpf: ebpf_active,
        },
        Request::Snapshot { seconds } => {
            let samples = ring.lock().unwrap().last(seconds.min(3600) as usize);
            Response::Snapshot { samples }
        }
        Request::TotalApps { start, end } => match storage::open(Path::new(db_path)) {
            Ok(conn) => match storage::query_total_apps(&conn, start, end, epoch_now()) {
                Ok(rows) => Response::TotalApps { rows },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },
        Request::Total { start, end } => {
            // Cada consulta abre su propia conexión de lectura: WAL permite
            // lectores concurrentes con el escritor del daemon.
            match storage::open(Path::new(db_path)) {
                Ok(conn) => match storage::query_total(&conn, start, end, epoch_now()) {
                    Ok(rows) => Response::Total { rows },
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            }
        }
        Request::Series { start, end, bucket } => match storage::open(Path::new(db_path)) {
            Ok(conn) => match storage::query_series(&conn, start, end, bucket, epoch_now()) {
                Ok(points) => Response::Series { points },
                Err(e) => Response::Error {
                    message: e.to_string(),
                },
            },
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },
        Request::TopRemotes { start, end, app } => match storage::open(Path::new(db_path)) {
            Ok(conn) => {
                match storage::query_top_remotes(&conn, start, end, app.as_deref(), epoch_now()) {
                    Ok(rows) => Response::TopRemotes {
                        rows: rows
                            .into_iter()
                            .map(|(host, rx, tx)| RemoteRow { host, rx, tx })
                            .collect(),
                    },
                    Err(e) => Response::Error {
                        message: e.to_string(),
                    },
                }
            }
            Err(e) => Response::Error {
                message: e.to_string(),
            },
        },
    }
}

pub fn epoch_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::LiveRing;
    use crate::storage;
    use caudalghost_core::proto::{Bucket, Request, Response};
    use caudalghost_core::types::{IfaceDelta, Sample};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixStream;
    use std::sync::{Arc, Mutex};

    fn arranca_servidor() -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("api.sock");
        let db = dir.path().join("t.db");
        let conn = storage::open(&db).unwrap();
        // ts antiguo → vive en la capa día (query_total con now real lo enruta ahí)
        conn.execute("INSERT INTO day_iface VALUES (86400, 'eth0', 500, 50)", [])
            .unwrap();
        conn.execute("INSERT INTO day_app VALUES (86400, 'firefox', 300, 30)", [])
            .unwrap();
        drop(conn);
        let ring = Arc::new(Mutex::new(LiveRing::new(3600)));
        ring.lock().unwrap().push(Sample {
            ts: 1,
            deltas: vec![IfaceDelta {
                iface: "eth0".into(),
                rx: 9,
                tx: 1,
            }],
            apps: vec![],
            flows: vec![],
        });
        let db_s = db.to_string_lossy().into_owned();
        let listener = bind(&sock).unwrap();
        std::thread::spawn(move || {
            let _ = serve(listener, ring, db_s, false);
        });
        (dir, sock)
    }

    fn pide(sock: &std::path::Path, req: &Request) -> Response {
        let mut stream = UnixStream::connect(sock).unwrap();
        let mut line = serde_json::to_string(req).unwrap();
        line.push('\n');
        stream.write_all(line.as_bytes()).unwrap();
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        reader.read_line(&mut resp).unwrap();
        serde_json::from_str(&resp).unwrap()
    }

    #[test]
    fn bind_crea_socket_con_permisos_de_grupo_0660() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("api.sock");
        let _l = bind(&sock).unwrap();
        let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o660);
    }

    #[test]
    fn ping_snapshot_total_y_error() {
        let (_dir, sock) = arranca_servidor();
        assert!(matches!(
            pide(&sock, &Request::Ping),
            Response::Pong { ebpf: false, .. }
        ));
        match pide(&sock, &Request::Snapshot { seconds: 10 }) {
            Response::Snapshot { samples } => assert_eq!(samples.len(), 1),
            other => panic!("respuesta inesperada: {other:?}"),
        }
        match pide(
            &sock,
            &Request::Total {
                start: 0,
                end: i64::MAX,
            },
        ) {
            Response::Total { rows } => {
                assert_eq!(rows.len(), 1);
                assert_eq!((rows[0].rx, rows[0].tx), (500, 50));
            }
            other => panic!("respuesta inesperada: {other:?}"),
        }
        match pide(
            &sock,
            &Request::TotalApps {
                start: 0,
                end: i64::MAX,
            },
        ) {
            Response::TotalApps { rows } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(
                    (rows[0].app.as_str(), rows[0].rx, rows[0].tx),
                    ("firefox", 300, 30)
                );
            }
            other => panic!("respuesta inesperada: {other:?}"),
        }
        match pide(
            &sock,
            &Request::Series {
                start: 0,
                end: i64::MAX,
                bucket: Bucket::Day,
            },
        ) {
            Response::Series { points } => {
                assert_eq!(points.len(), 1);
                assert_eq!((points[0].rx, points[0].tx), (500, 50));
            }
            other => panic!("respuesta inesperada: {other:?}"),
        }
        // Línea inválida → Error, no caída del servidor.
        let mut stream = UnixStream::connect(&sock).unwrap();
        stream.write_all(b"esto no es json\n").unwrap();
        let mut reader = BufReader::new(stream);
        let mut resp = String::new();
        reader.read_line(&mut resp).unwrap();
        let parsed: Response = serde_json::from_str(&resp).unwrap();
        assert!(matches!(parsed, Response::Error { .. }));
    }
}
