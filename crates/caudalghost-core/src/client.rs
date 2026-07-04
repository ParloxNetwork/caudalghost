use crate::proto::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

/// Clasificación de un fallo de conexión al socket, para que los llamantes
/// (CLI, TUI, GUI) decidan cómo reaccionar (p. ej. exit code) sin tener que
/// volver a inspeccionar `std::io::ErrorKind` por su cuenta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConexionError {
    /// El socket no existe: el daemon no está corriendo.
    NoEncontrado,
    /// El socket existe pero no tenemos permiso para usarlo.
    SinPermiso,
    /// Cualquier otro fallo de conexión (rechazada, etc.).
    Otro,
}

impl ConexionError {
    /// Clasifica un `std::io::Error` de conexión según su `ErrorKind`.
    pub fn clasificar(e: &std::io::Error) -> Self {
        use std::io::ErrorKind;
        match e.kind() {
            ErrorKind::NotFound => ConexionError::NoEncontrado,
            ErrorKind::PermissionDenied => ConexionError::SinPermiso,
            _ => ConexionError::Otro,
        }
    }
}

/// Abre el socket Unix, envía una petición (una línea JSON) y devuelve la
/// respuesta (una línea JSON). Cliente IPC compartido por la CLI/TUI y la GUI.
/// Techo defensivo del tamaño de la respuesta leída del socket. Está muy por
/// encima de cualquier respuesta legítima del daemon; su único fin es que un
/// peer NO confiable en el socket (o algo que no sea el daemon) no pueda hacer
/// crecer el `String` del cliente sin límite → DoS por memoria en CLI/TUI/GUI.
/// Es la contraparte del `MAX_LINE_BYTES` del servidor (que acota las peticiones).
const MAX_RESPONSE_BYTES: u64 = 64 * 1024 * 1024;

pub fn call(socket: &Path, req: &Request) -> anyhow::Result<Response> {
    let mut stream = UnixStream::connect(socket).map_err(|e| connect_error(socket, &e))?;
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let resp = read_capped_line(BufReader::new(stream), MAX_RESPONSE_BYTES)?;
    Ok(serde_json::from_str(&resp)?)
}

/// Lee una línea acotada a `max` bytes. Si se alcanza el tope sin `\n`, corta y
/// devuelve error en vez de crecer sin límite. Extraído para poder probar el
/// tope sin escribir megabytes reales por el socket.
fn read_capped_line<R: BufRead>(reader: R, max: u64) -> anyhow::Result<String> {
    let mut resp = String::new();
    let n = reader.take(max).read_line(&mut resp)?;
    if n as u64 >= max && !resp.ends_with('\n') {
        anyhow::bail!(
            "respuesta del daemon demasiado grande (> {max} bytes) — posible peer no confiable en el socket"
        );
    }
    Ok(resp)
}

/// Traduce el fallo de conexión a un aviso accionable en voz de marca. La
/// clasificación (`ConexionError`) queda adjunta como `context` para que los
/// llamantes puedan recuperarla con `error.downcast_ref::<ConexionError>()`.
fn connect_error(socket: &Path, e: &std::io::Error) -> anyhow::Error {
    let clase = ConexionError::clasificar(e);
    let msg = match clase {
        ConexionError::NoEncontrado => format!(
            "no encuentro el daemon en {} — arranca el servicio con `systemctl enable --now caudalghostd` (o lanza caudalghostd a mano)",
            socket.display()
        ),
        ConexionError::SinPermiso => format!(
            "sin permiso para el socket {} — añádete al grupo `caudalghost` y vuelve a iniciar sesión",
            socket.display()
        ),
        ConexionError::Otro => format!(
            "no pude conectar con el daemon en {} ({e}) — ¿está corriendo caudalghostd?",
            socket.display()
        ),
    };
    anyhow::Error::new(clase).context(msg)
}

impl std::fmt::Display for ConexionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConexionError::NoEncontrado => write!(f, "socket no encontrado"),
            ConexionError::SinPermiso => write!(f, "sin permiso para el socket"),
            ConexionError::Otro => write!(f, "fallo de conexión"),
        }
    }
}
impl std::error::Error for ConexionError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;

    #[test]
    fn connect_error_distingue_notfound_y_permiso() {
        let p = Path::new("/run/caudalghost/caudalghost.sock");
        let nf = connect_error(p, &std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(nf.to_string().contains("systemctl enable"), "{nf}");
        let pd = connect_error(
            p,
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        );
        assert!(pd.to_string().contains("grupo `caudalghost`"), "{pd}");
        let otro = connect_error(
            p,
            &std::io::Error::from(std::io::ErrorKind::ConnectionRefused),
        );
        assert!(otro.to_string().contains("no pude conectar"), "{otro}");
    }

    #[test]
    fn conexion_error_clasifica_por_errorkind() {
        assert_eq!(
            ConexionError::clasificar(&std::io::Error::from(std::io::ErrorKind::NotFound)),
            ConexionError::NoEncontrado
        );
        assert_eq!(
            ConexionError::clasificar(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
            ConexionError::SinPermiso
        );
        assert_eq!(
            ConexionError::clasificar(&std::io::Error::from(std::io::ErrorKind::ConnectionRefused)),
            ConexionError::Otro
        );
    }

    #[test]
    fn connect_error_lleva_la_clasificacion_en_el_contexto() {
        let p = Path::new("/run/caudalghost/caudalghost.sock");
        let nf = connect_error(p, &std::io::Error::from(std::io::ErrorKind::NotFound));
        assert_eq!(
            nf.downcast_ref::<ConexionError>().copied(),
            Some(ConexionError::NoEncontrado)
        );
    }

    /// Servidor falso de una respuesta para probar el roundtrip del cliente.
    #[test]
    fn call_roundtrip_contra_servidor_falso() {
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("fake.sock");
        let listener = UnixListener::bind(&sock).unwrap();
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert!(line.contains("ping"));
            let mut s = stream;
            s.write_all(b"{\"type\":\"pong\",\"version\":\"9.9.9\"}\n")
                .unwrap();
        });
        let resp = call(&sock, &Request::Ping).unwrap();
        assert!(matches!(resp, Response::Pong { version, .. } if version == "9.9.9"));
    }

    /// Una respuesta gigante SIN '\n' se corta al tope y devuelve error, en vez
    /// de crecer el String del cliente sin límite (DoS por memoria ante un peer
    /// no confiable en el socket). Simetría con el MAX_LINE_BYTES del servidor.
    #[test]
    fn read_capped_line_corta_respuesta_gigante_sin_newline() {
        use std::io::Cursor;
        let data = vec![b'x'; 500]; // 500 bytes, sin '\n'
        let err = read_capped_line(Cursor::new(data), 100).unwrap_err();
        assert!(
            err.to_string().contains("demasiado grande"),
            "debe cortar y avisar: {err}"
        );
    }

    #[test]
    fn read_capped_line_ok_bajo_el_tope() {
        use std::io::Cursor;
        let r = read_capped_line(Cursor::new(b"{\"type\":\"pong\"}\n".to_vec()), 1024).unwrap();
        assert!(r.contains("pong") && r.ends_with('\n'));
    }
}
