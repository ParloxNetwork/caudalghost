use crate::live::LiveRing;
use crate::storage;
use caudalghost_core::proto::{RemoteRow, Request, Response};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Tope de bytes por línea de petición. Cualquier petición legítima es < 1 KiB;
/// 64 KiB es un techo holgado que corta una línea sin `\n` antes de agotar el
/// heap (evita OOM por una sola conexión que nunca cierra la línea).
const MAX_LINE_BYTES: usize = 64 * 1024;
/// Conexiones concurrentes máximas. Quien abra de más se rechaza, en vez de
/// generar un hilo por conexión sin límite (evita agotamiento de hilos/FDs).
const MAX_CONNS: usize = 128;
/// Timeout de lectura/escritura por conexión: cierra a quien conecta y no habla
/// (libera el slot; corta un slow-loris local que ocuparía un hilo para siempre).
const IO_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Desenlace de leer una línea de petición acotada.
#[derive(Debug, PartialEq, Eq)]
enum LineOutcome {
    /// Fin de la conexión (lectura de 0 bytes).
    Eof,
    /// Línea completa (terminada en `\n`) leída en `line`.
    Line,
    /// Se alcanzó el tope sin `\n`: línea demasiado larga (o cierre a media línea).
    TooLong,
}

/// Lee una línea terminada en `\n` en `line`, sin superar `max` bytes. Sustituye
/// a `read_line` directo para impedir el crecimiento ilimitado del `String` ante
/// una conexión que nunca manda `\n`.
fn read_request_line(
    reader: &mut impl BufRead,
    line: &mut String,
    max: usize,
) -> std::io::Result<LineOutcome> {
    line.clear();
    // `take(max)` acota la lectura: si la línea no trae `\n` en `max` bytes, el
    // lector se detiene ahí en vez de crecer el `String` sin límite.
    let n = reader.take(max as u64).read_line(line)?;
    if n == 0 {
        return Ok(LineOutcome::Eof);
    }
    if !line.ends_with('\n') {
        return Ok(LineOutcome::TooLong);
    }
    Ok(LineOutcome::Line)
}

/// Limita conexiones concurrentes. `try_acquire` entrega un guard que libera el
/// slot al soltarse (`Drop`); si no quedan slots, devuelve `None`.
struct ConnLimiter {
    active: Arc<AtomicUsize>,
    max: usize,
}

/// Guard de una conexión admitida: decrementa el contador al soltarse.
struct ConnGuard(Arc<AtomicUsize>);

impl Drop for ConnGuard {
    fn drop(&mut self) {
        // Relaxed basta: el contador no publica/consume otros datos, solo cuenta.
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

impl ConnLimiter {
    fn new(max: usize) -> Self {
        Self {
            active: Arc::new(AtomicUsize::new(0)),
            max: max.max(1),
        }
    }

    /// Reserva un slot si hay sitio. `None` si ya hay `max` conexiones activas.
    fn try_acquire(&self) -> Option<ConnGuard> {
        // Incremento optimista; si supera el tope, se revierte. Evita una ventana
        // de carrera entre comprobar y reservar. Relaxed: contador autocontenido.
        let prev = self.active.fetch_add(1, Ordering::Relaxed);
        if prev >= self.max {
            self.active.fetch_sub(1, Ordering::Relaxed);
            None
        } else {
            Some(ConnGuard(self.active.clone()))
        }
    }

    #[cfg(test)]
    fn active(&self) -> usize {
        self.active.load(Ordering::Relaxed)
    }
}

/// Bucle de aceptación: un hilo por conexión. El listener ya viene enlazado por
/// `bind()`, de modo que el llamador puede señalar readiness antes de aceptar.
pub fn serve(
    listener: UnixListener,
    ring: Arc<Mutex<LiveRing>>,
    db_path: String,
    ebpf_active: bool,
) -> std::io::Result<()> {
    let limiter = ConnLimiter::new(MAX_CONNS);
    for stream in listener.incoming() {
        let stream = match stream {
            Ok(s) => s,
            Err(e) => {
                eprintln!("caudalghostd: error aceptando conexión: {e}");
                continue;
            }
        };
        // Tope de conexiones concurrentes: si no hay slot, cerramos sin gastar un
        // hilo (evita agotamiento de hilos/FDs por avalancha de conexiones).
        let Some(guard) = limiter.try_acquire() else {
            eprintln!("caudalghostd: conexión rechazada (límite de {MAX_CONNS} alcanzado)");
            drop(stream);
            continue;
        };
        let ring = ring.clone();
        let db = db_path.clone();
        std::thread::spawn(move || {
            let _guard = guard; // libera el slot al terminar el hilo
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
    // Timeouts: cierran a un cliente que conecta y no habla (o deja de leer),
    // liberando el slot de conexión en vez de bloquear el hilo para siempre.
    let read_half = stream.try_clone()?;
    read_half.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    let mut reader = BufReader::new(read_half);
    let mut writer = BufWriter::new(stream);
    let mut line = String::new();
    loop {
        match read_request_line(&mut reader, &mut line, MAX_LINE_BYTES)? {
            LineOutcome::Eof => break,
            LineOutcome::TooLong => {
                // Línea truncada: no se puede resincronizar el protocolo de líneas,
                // así que avisamos y cerramos la conexión.
                write_response(
                    &mut writer,
                    &Response::Error {
                        message: "petición demasiado larga".into(),
                    },
                )?;
                break;
            }
            LineOutcome::Line => {
                let resp = match serde_json::from_str::<Request>(line.trim()) {
                    Ok(req) => dispatch(req, ring, db_path, ebpf_active),
                    Err(e) => Response::Error {
                        message: format!("petición inválida: {e}"),
                    },
                };
                write_response(&mut writer, &resp)?;
            }
        }
    }
    Ok(())
}

/// Serializa una respuesta como línea JSON y la vuelca (con flush).
fn write_response(writer: &mut impl Write, resp: &Response) -> anyhow::Result<()> {
    let mut out = serde_json::to_string(resp)?;
    out.push('\n');
    writer.write_all(out.as_bytes())?;
    writer.flush()?;
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

    #[test]
    fn linea_normal_terminada_en_newline_es_line() {
        let data = b"{\"cmd\":\"ping\"}\n";
        let mut r = BufReader::new(&data[..]);
        let mut s = String::new();
        assert_eq!(
            read_request_line(&mut r, &mut s, 64 * 1024).unwrap(),
            LineOutcome::Line
        );
        assert!(s.contains("ping"));
    }

    #[test]
    fn entrada_vacia_es_eof() {
        let data: &[u8] = b"";
        let mut r = BufReader::new(data);
        let mut s = String::new();
        assert_eq!(
            read_request_line(&mut r, &mut s, 100).unwrap(),
            LineOutcome::Eof
        );
    }

    #[test]
    fn linea_que_supera_el_tope_sin_newline_es_toolong_sin_crecer() {
        // 10·max bytes sin un solo '\n': read_line ingenuo crecería el String a
        // 10·max; el lector acotado se detiene en max y reporta TooLong.
        let big = vec![b'x'; 1000];
        let mut r = BufReader::new(&big[..]);
        let mut s = String::new();
        assert_eq!(
            read_request_line(&mut r, &mut s, 100).unwrap(),
            LineOutcome::TooLong
        );
        assert!(
            s.len() <= 100,
            "el buffer no debe exceder el tope: {}",
            s.len()
        );
    }

    #[test]
    fn dos_lineas_seguidas_se_leen_en_dos_pasadas() {
        // El lector acotado no debe perder datos del buffer entre llamadas.
        let data = b"{\"cmd\":\"ping\"}\n{\"cmd\":\"ping\"}\n";
        let mut r = BufReader::new(&data[..]);
        let mut s = String::new();
        assert_eq!(
            read_request_line(&mut r, &mut s, 64 * 1024).unwrap(),
            LineOutcome::Line
        );
        assert_eq!(
            read_request_line(&mut r, &mut s, 64 * 1024).unwrap(),
            LineOutcome::Line
        );
        assert_eq!(
            read_request_line(&mut r, &mut s, 64 * 1024).unwrap(),
            LineOutcome::Eof
        );
    }

    #[test]
    fn conn_limiter_concede_hasta_el_tope_y_libera_al_soltar() {
        let lim = ConnLimiter::new(2);
        let g1 = lim.try_acquire();
        let g2 = lim.try_acquire();
        assert!(g1.is_some() && g2.is_some());
        assert_eq!(lim.active(), 2);
        assert!(
            lim.try_acquire().is_none(),
            "el 3.º debe rechazarse en tope 2"
        );
        drop(g1);
        assert_eq!(lim.active(), 1);
        assert!(lim.try_acquire().is_some(), "tras liberar uno hay sitio");
    }

    #[test]
    fn linea_gigante_no_tumba_el_daemon_y_sigue_sirviendo() {
        let (_dir, sock) = arranca_servidor();
        {
            let mut stream = UnixStream::connect(&sock).unwrap();
            // Por encima del tope: el server corta y responde Error (o cierra).
            let huge = vec![b'x'; MAX_LINE_BYTES + 4096];
            let _ = stream.write_all(&huge);
            let _ = stream.write_all(b"\n");
            let mut reader = BufReader::new(stream);
            let mut resp = String::new();
            let _ = reader.read_line(&mut resp);
            if !resp.trim().is_empty() {
                let parsed: Response = serde_json::from_str(&resp).unwrap();
                assert!(matches!(parsed, Response::Error { .. }));
            }
        }
        // Propiedad central: el daemon sigue vivo y atiende una conexión nueva.
        assert!(matches!(pide(&sock, &Request::Ping), Response::Pong { .. }));
    }
}
