use std::path::{Path, PathBuf};

/// Socket bajo XDG_RUNTIME_DIR; sin él, /tmp/caudalghost.sock.
pub fn socket_path_from(xdg_runtime: Option<&str>) -> PathBuf {
    match xdg_runtime {
        Some(dir) => PathBuf::from(dir).join("caudalghost.sock"),
        None => PathBuf::from("/tmp/caudalghost.sock"),
    }
}

/// Base bajo XDG_DATA_HOME; sin él, ~/.local/share, siempre en caudalghost/caudal.db.
pub fn db_path_from(xdg_data: Option<&str>, home: &str) -> PathBuf {
    let base = match xdg_data {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(home).join(".local/share"),
    };
    base.join("caudalghost").join("caudal.db")
}

/// Socket del daemon cuando corre como servicio de sistema (F5).
pub const SYSTEM_SOCKET: &str = "/run/caudalghost/caudalghost.sock";

/// Candidatos del socket para CLIENTES (CLI/GUI), de mayor a menor prioridad:
/// override de entorno → socket de sistema (servicio) → XDG runtime (dev) →
/// /tmp (último recurso). Función pura: no toca el filesystem.
pub fn client_socket_candidates(
    env_socket: Option<&str>,
    xdg_runtime: Option<&str>,
) -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(s) = env_socket {
        v.push(PathBuf::from(s));
    }
    v.push(PathBuf::from(SYSTEM_SOCKET));
    if let Some(dir) = xdg_runtime {
        v.push(PathBuf::from(dir).join("caudalghost.sock"));
    }
    v.push(PathBuf::from("/tmp/caudalghost.sock"));
    v
}

/// Primer candidato que satisface `exists` (predicado inyectable → testeable).
pub fn first_existing(candidates: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    candidates.iter().find(|p| exists(p.as_path())).cloned()
}

/// Resuelve el socket del cliente leyendo el entorno real: primer candidato
/// existente, o el de sistema si ninguno existe (para que el error lo nombre).
pub fn resolve_client_socket() -> PathBuf {
    let env_socket = std::env::var("CAUDALGHOST_SOCKET").ok();
    let xdg = std::env::var("XDG_RUNTIME_DIR").ok();
    let cands = client_socket_candidates(env_socket.as_deref(), xdg.as_deref());
    first_existing(&cands, |p| p.exists()).unwrap_or_else(|| PathBuf::from(SYSTEM_SOCKET))
}

/// Ruta del socket por defecto leyendo el entorno real.
pub fn default_socket_path() -> PathBuf {
    let xdg = std::env::var("XDG_RUNTIME_DIR").ok();
    socket_path_from(xdg.as_deref())
}

/// Ruta de la base por defecto leyendo el entorno real.
pub fn default_db_path() -> PathBuf {
    let xdg = std::env::var("XDG_DATA_HOME").ok();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    db_path_from(xdg.as_deref(), &home)
}

/// Base del daemon cuando corre como servicio de sistema (F5).
pub const SYSTEM_DB: &str = "/var/lib/caudalghost/caudal.db";

/// Base resuelta: override explícito (CAUDALGHOST_DB) o la lógica XDG/HOME.
/// Función pura (entorno inyectado) → testeable.
pub fn db_path_resolved(env_db: Option<&str>, xdg_data: Option<&str>, home: &str) -> PathBuf {
    match env_db {
        Some(p) => PathBuf::from(p),
        None => db_path_from(xdg_data, home),
    }
}

/// Base por defecto leyendo el entorno real (honra CAUDALGHOST_DB).
pub fn resolve_db_path() -> PathBuf {
    let env_db = std::env::var("CAUDALGHOST_DB").ok();
    let xdg = std::env::var("XDG_DATA_HOME").ok();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    db_path_resolved(env_db.as_deref(), xdg.as_deref(), &home)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn socket_usa_xdg_runtime_si_existe() {
        let p = socket_path_from(Some("/run/user/1000"));
        assert_eq!(p.to_str().unwrap(), "/run/user/1000/caudalghost.sock");
    }

    #[test]
    fn socket_cae_a_tmp_sin_xdg() {
        let p = socket_path_from(None);
        assert_eq!(p.to_str().unwrap(), "/tmp/caudalghost.sock");
    }

    #[test]
    fn db_usa_xdg_data_o_home() {
        assert_eq!(
            db_path_from(Some("/x/data"), "/home/u").to_str().unwrap(),
            "/x/data/caudalghost/caudal.db"
        );
        assert_eq!(
            db_path_from(None, "/home/u").to_str().unwrap(),
            "/home/u/.local/share/caudalghost/caudal.db"
        );
    }

    #[test]
    fn candidatos_socket_priorizan_env_luego_sistema_luego_xdg() {
        let v = client_socket_candidates(Some("/tmp/x.sock"), Some("/run/user/1000"));
        let got: Vec<&str> = v.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(
            got,
            vec![
                "/tmp/x.sock",
                "/run/caudalghost/caudalghost.sock",
                "/run/user/1000/caudalghost.sock",
                "/tmp/caudalghost.sock",
            ]
        );
    }

    #[test]
    fn candidatos_socket_sin_env_ni_xdg() {
        let v = client_socket_candidates(None, None);
        let got: Vec<&str> = v.iter().map(|p| p.to_str().unwrap()).collect();
        assert_eq!(
            got,
            vec!["/run/caudalghost/caudalghost.sock", "/tmp/caudalghost.sock"]
        );
    }

    #[test]
    fn primer_existente_elige_el_que_existe() {
        let cands = vec![
            PathBuf::from("/a/no.sock"),
            PathBuf::from("/b/si.sock"),
            PathBuf::from("/c/no.sock"),
        ];
        let elegido = first_existing(&cands, |p| p.to_str() == Some("/b/si.sock"));
        assert_eq!(elegido.unwrap().to_str().unwrap(), "/b/si.sock");
        assert!(first_existing(&cands, |_| false).is_none());
    }

    #[test]
    fn db_resuelve_override_luego_default() {
        assert_eq!(
            db_path_resolved(Some("/data/x.db"), Some("/x/data"), "/home/u")
                .to_str()
                .unwrap(),
            "/data/x.db"
        );
        assert_eq!(
            db_path_resolved(None, None, "/home/u").to_str().unwrap(),
            "/home/u/.local/share/caudalghost/caudal.db"
        );
    }
}
