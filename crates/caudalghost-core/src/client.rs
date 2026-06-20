use crate::proto::{Request, Response};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

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

/// Traduce el fallo de conexión a un aviso accionable en voz de marca.
fn connect_error(socket: &Path, e: &std::io::Error) -> anyhow::Error {
    use std::io::ErrorKind;
    let msg = match e.kind() {
        ErrorKind::NotFound => format!(
            "no encuentro el daemon en {} — arranca el servicio con `systemctl enable --now caudalghostd` (o lanza caudalghostd a mano)",
            socket.display()
        ),
        ErrorKind::PermissionDenied => format!(
            "sin permiso para el socket {} — añádete al grupo `caudalghost` y vuelve a iniciar sesión",
            socket.display()
        ),
        _ => format!(
            "no pude conectar con el daemon en {} ({e}) — ¿está corriendo caudalghostd?",
            socket.display()
        ),
    };
    anyhow::anyhow!(msg)
}

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
