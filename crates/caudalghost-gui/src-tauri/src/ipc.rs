use caudalghost_core::proto::{Bucket, Request, Response};
use caudalghost_core::{client, paths, range};
use std::path::PathBuf;

/// Ruta del socket del daemon. Delega en el resolutor compartido del core:
/// override de entorno → socket de sistema (servicio) → XDG runtime → /tmp.
pub fn socket() -> PathBuf {
    paths::resolve_client_socket()
}

/// Resuelve un token de rango a (start, end, bucket) para la consulta histórica.
pub fn history_window(
    rango: &str,
    now: chrono::DateTime<chrono::Local>,
) -> anyhow::Result<(i64, i64, Bucket)> {
    let w = range::resolve(rango, now).map_err(anyhow::Error::msg)?;
    let bucket = range::pick_bucket(w.start, w.end, w.end);
    Ok((w.start, w.end, bucket))
}

/// Llama al daemon por el socket compartido; mapea el error a String para Tauri.
pub fn call(req: &Request) -> Result<Response, String> {
    client::call(&socket(), req).map_err(|e| e.to_string())
}

/// Por qué (no) responde el daemon. Dato estructurado para que la GUI muestre el
/// mensaje y la acción correctos, en vez de un "reintentando" eterno e indistinto.
/// Espeja las ramas de `caudalghost_core::client::connect_error`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnReason {
    /// El socket responde.
    Ok,
    /// Socket inexistente → el daemon no está corriendo (o cayó).
    Down,
    /// Sin permiso sobre el socket → falta el grupo `caudalghost` / ACL.
    Denied,
    /// Cualquier otro fallo (rechazo, daemon a medio arrancar, etc.).
    Other,
}

/// Clasifica un `ErrorKind` de `connect` en una razón de conexión. Función pura:
/// se mira el `ErrorKind` (robusto), nunca el texto del mensaje.
pub fn classify(kind: std::io::ErrorKind) -> ConnReason {
    use std::io::ErrorKind;
    match kind {
        ErrorKind::NotFound => ConnReason::Down,
        ErrorKind::PermissionDenied => ConnReason::Denied,
        _ => ConnReason::Other,
    }
}

/// Sondea el socket del daemon (sin enviar petición) y devuelve por qué (no)
/// conecta. Lo usa la GUI para distinguir "daemon caído" de "sin permiso".
pub fn probe_reason() -> ConnReason {
    match std::os::unix::net::UnixStream::connect(socket()) {
        Ok(_) => ConnReason::Ok,
        Err(e) => classify(e.kind()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_window_mapea_rango_a_cubo() {
        let now = chrono::Local::now();
        let (s, e, b) = history_window("7d", now).unwrap();
        assert!(e > s);
        assert_eq!(b, Bucket::Hour); // 7 días → cubo hora
    }

    #[test]
    fn history_window_rango_invalido_es_error() {
        assert!(history_window("xxx", chrono::Local::now()).is_err());
    }

    #[test]
    fn classify_distingue_caido_de_sin_permiso() {
        use std::io::ErrorKind;
        // Espeja las ramas de core::client::connect_error, pero como dato
        // estructurado para que la GUI muestre el mensaje y la acción correctos.
        assert_eq!(classify(ErrorKind::NotFound), ConnReason::Down);
        assert_eq!(classify(ErrorKind::PermissionDenied), ConnReason::Denied);
        assert_eq!(classify(ErrorKind::ConnectionRefused), ConnReason::Other);
    }
}
