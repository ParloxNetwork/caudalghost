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
pub fn call(socket: &Path, req: &Request) -> anyhow::Result<Response> {
    let mut stream = UnixStream::connect(socket).map_err(|e| connect_error(socket, &e))?;
    let mut line = serde_json::to_string(req)?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    let mut reader = BufReader::new(stream);
    let mut resp = String::new();
    reader.read_line(&mut resp)?;
    Ok(serde_json::from_str(&resp)?)
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
}
