//! Acceso sin re-login (D11): concede el socket por UID a los usuarios con
//! sesión activa (systemd-logind), además del grupo `caudalghost` (fallback).
//! Así `instalar → abrir → funciona` sin cerrar sesión ni reiniciar.

use std::collections::HashSet;
use std::path::Path;

use posix_acl::{PosixACL, Qualifier, ACL_READ, ACL_WRITE};

/// UIDs con sesión activa según logind: cada usuario con ≥1 sesión tiene un
/// fichero `/run/systemd/users/<uid>`. Filtra a usuarios reales (≥ `min_uid`).
/// Directorio inyectado → función testeable; sin logind devuelve vacío.
pub fn active_user_uids_in(users_dir: &Path, min_uid: u32) -> Vec<u32> {
    let mut uids = Vec::new();
    let Ok(rd) = std::fs::read_dir(users_dir) else {
        return uids;
    };
    for entry in rd.flatten() {
        if let Some(uid) = entry
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        {
            if uid >= min_uid {
                uids.push(uid);
            }
        }
    }
    uids.sort_unstable();
    uids.dedup();
    uids
}

/// UIDs con sesión activa leyendo el entorno real (`/run/systemd/users`, ≥ 1000).
pub fn active_user_uids() -> Vec<u32> {
    active_user_uids_in(Path::new("/run/systemd/users"), 1000)
}

/// Variable de entorno que controla la política de acceso al socket.
pub const ENV_ACL_ACTIVE: &str = "CAUDALGHOST_ACL_ACTIVE_USERS";

/// ¿Conceder ACL `rw` a los usuarios con sesión activa (UID≥1000)?
///
/// Default = `true`: preserva el flujo de **escritorio** D11 «instalar→abrir→
/// funciona» sin re-login. Se puede **desactivar** (ej.: edición **servidor**
/// multiusuario, donde cada login SSH crea sesión logind y vería los flujos de
/// todo el host) vía el flag `--no-acl-active-users` o `CAUDALGHOST_ACL_ACTIVE_USERS`
/// = `0`/`false`/`no`/`off`. Desactivado, el acceso queda **solo por el grupo
/// `caudalghost`**, que el administrador concede explícitamente.
pub fn grant_active_users(cli_no_grant: bool, env_val: Option<&str>) -> bool {
    if cli_no_grant {
        return false; // el flag CLI manda sobre el entorno
    }
    match env_val {
        Some(v) => !matches!(v.trim(), "0" | "false" | "no" | "off"),
        None => true, // default: conceder (preserva escritorio)
    }
}

/// Concede `u:<uid>:rw` a cada uid dado y retira las entradas de usuario que ya
/// no estén; deja intacto el resto del ACL (owner/grupo/otros). Idempotente.
pub fn sync_socket_acl(socket: &Path, uids: &[u32]) -> anyhow::Result<()> {
    let want: HashSet<u32> = uids.iter().copied().collect();
    let mut acl = PosixACL::read_acl(socket)?;
    let stale: Vec<u32> = acl
        .entries()
        .into_iter()
        .filter_map(|e| match e.qual {
            Qualifier::User(u) if !want.contains(&u) => Some(u),
            _ => None,
        })
        .collect();
    for u in stale {
        acl.remove(Qualifier::User(u));
    }
    for &u in uids {
        acl.set(Qualifier::User(u), ACL_READ | ACL_WRITE);
    }
    acl.write_acl(socket)?;
    Ok(())
}

/// ¿El ACL del fichero concede al uid? (helper de test)
#[cfg(test)]
fn uid_tiene_acl(path: &Path, uid: u32) -> bool {
    PosixACL::read_acl(path)
        .map(|acl| {
            acl.entries()
                .into_iter()
                .any(|e| matches!(e.qual, Qualifier::User(u) if u == uid))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lee_uids_de_sesiones_activas_filtrando_sistema() {
        let dir = tempfile::tempdir().unwrap();
        for n in ["1000", "1001", "0", "999", "no-uid"] {
            std::fs::write(dir.path().join(n), b"").unwrap();
        }
        assert_eq!(active_user_uids_in(dir.path(), 1000), vec![1000, 1001]);
        // Directorio inexistente → vacío (sin logind), no panic.
        assert!(active_user_uids_in(std::path::Path::new("/no/existe/x"), 1000).is_empty());
    }

    #[test]
    fn politica_default_concede_y_el_flag_o_env_la_desactivan() {
        // Default (sin flag, sin env): conceder → preserva escritorio.
        assert!(grant_active_users(false, None));
        // Flag CLI --no-acl-active-users: desactiva (prioridad máxima).
        assert!(!grant_active_users(true, None));
        assert!(!grant_active_users(true, Some("1"))); // el flag manda sobre env
                                                       // Env con valores "falsy" desactiva.
        for v in ["0", "false", "no", "off", " off "] {
            assert!(
                !grant_active_users(false, Some(v)),
                "env={v:?} debe desactivar"
            );
        }
        // Cualquier otro valor de env mantiene el default (conceder).
        for v in ["1", "true", "yes", "", "loquesea"] {
            assert!(
                grant_active_users(false, Some(v)),
                "env={v:?} debe conceder"
            );
        }
    }

    #[test]
    fn concede_y_retira_acl_de_usuario() {
        // Nota: requiere un FS con soporte ACL. tempdir() usa $TMPDIR (ext4/tmpfs
        // modernos lo soportan). Si el entorno no lo soporta, este test se omite.
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("sock");
        std::fs::write(&f, b"").unwrap();
        if sync_socket_acl(&f, &[424242]).is_err() {
            eprintln!("ACL no soportado en TMPDIR; test omitido");
            return;
        }
        assert!(uid_tiene_acl(&f, 424242));
        sync_socket_acl(&f, &[]).unwrap();
        assert!(!uid_tiene_acl(&f, 424242));
    }
}
