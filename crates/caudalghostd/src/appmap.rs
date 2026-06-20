use std::collections::HashMap;
use std::path::Path;

/// Tope de longitud (en chars) de una etiqueta de aplicación. Un proceso local
/// puede fijar su `comm` o disponer su cgroup con bytes arbitrarios; la etiqueta
/// se guarda en SQLite y se pinta en la TUI/GUI, así que la acotamos y saneamos.
const MAX_APP_LABEL: usize = 128;

/// Sanea una etiqueta de app: elimina caracteres de control y recorta a
/// `MAX_APP_LABEL` chars. Evita bloat de BD e inyección de control en la UI.
fn sanitize_label(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(MAX_APP_LABEL)
        .collect()
}

/// Extrae el identificador de aplicación de una línea de cgroup v2 (`0::/...`).
/// Reconoce scopes de escritorio (app-*.scope), Flatpak, Snap y services.
pub fn app_from_cgroup(line: &str) -> Option<String> {
    let path = line.strip_prefix("0::")?;
    let last = path.rsplit('/').next()?;
    if let Some(rest) = last.strip_prefix("app-flatpak-") {
        return Some(clean_app_unit(rest));
    }
    if let Some(rest) = last.strip_prefix("app-") {
        return Some(clean_app_unit(rest));
    }
    if let Some(rest) = last.strip_prefix("snap.") {
        return rest.split('.').next().map(str::to_string);
    }
    if let Some(name) = last.strip_suffix(".service") {
        return Some(name.to_string());
    }
    None
}

/// Limpia un nombre de unidad systemd ya sin el prefijo app-/app-flatpak-:
/// corta la instancia de templates (@...), quita .scope/.service, y para
/// scopes quita el sufijo aleatorio -<alfanum>. Decodifica escapes \x2d.
fn clean_app_unit(s: &str) -> String {
    if let Some(i) = s.find('@') {
        // template app-<id>@<instancia>.service → el id es lo anterior al @
        return unescape_systemd(&s[..i]);
    }
    if let Some(body) = s.strip_suffix(".scope") {
        return unescape_systemd(strip_random_suffix(body));
    }
    if let Some(body) = s.strip_suffix(".service") {
        // service sin instancia: el nombre es exacto, NO recortar guiones
        return unescape_systemd(body);
    }
    unescape_systemd(strip_random_suffix(s))
}

/// Sufijo -<alfanum> que systemd añade a los scopes de escritorio.
/// Limitación conocida: un id que termine legítimamente así se recorta de más.
fn strip_random_suffix(s: &str) -> &str {
    match s.rfind('-') {
        Some(i)
            if !s[i + 1..].is_empty() && s[i + 1..].chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            &s[..i]
        }
        _ => s,
    }
}

/// systemd escapa '-' como \x2d en nombres de unidad.
fn unescape_systemd(s: &str) -> String {
    s.replace("\\x2d", "-")
}

/// Resuelve un tgid leyendo /proc: cgroup → comm → "(desconocido)".
pub fn resolve_app(proc_root: &Path, tgid: u32) -> String {
    let base = proc_root.join(tgid.to_string());
    if let Ok(cg) = std::fs::read_to_string(base.join("cgroup")) {
        for line in cg.lines() {
            if let Some(app) = app_from_cgroup(line) {
                return sanitize_label(&app);
            }
        }
    }
    if let Ok(comm) = std::fs::read_to_string(base.join("comm")) {
        let c = comm.trim();
        if !c.is_empty() {
            return sanitize_label(c);
        }
    }
    "(desconocido)".to_string()
}

/// Caché tgid→app: resuelve una vez por proceso y olvida los que ya no aparecen.
#[derive(Default)]
pub struct AppCache {
    map: HashMap<u32, String>,
}

impl AppCache {
    pub fn get(&mut self, proc_root: &Path, tgid: u32) -> String {
        self.map
            .entry(tgid)
            .or_insert_with(|| resolve_app(proc_root, tgid))
            .clone()
    }

    /// Conserva solo los tgids vistos en el último drenado (evita crecer sin límite).
    pub fn retain(&mut self, live: &[u32]) {
        self.map.retain(|k, _| live.contains(k));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_label_quita_control_chars_y_recorta() {
        // Limpio se mantiene.
        assert_eq!(sanitize_label("firefox"), "firefox");
        // Control chars (ESC, newline, NUL) eliminados; el resto se conserva.
        assert_eq!(sanitize_label("fire\x1b[31mfox\n"), "fire[31mfox");
        assert_eq!(sanitize_label("ab\0cd"), "abcd");
        // Tope de longitud.
        let largo = "a".repeat(500);
        assert_eq!(sanitize_label(&largo).chars().count(), MAX_APP_LABEL);
    }

    #[test]
    fn resolve_app_recorta_etiqueta_larguisima_de_cgroup() {
        let largo = "a".repeat(300);
        let cg = format!("0::/user.slice/app.slice/app-{largo}.scope");
        let d = fake_proc(30, Some(&cg), None);
        let got = resolve_app(d.path(), 30);
        assert!(
            got.chars().count() <= MAX_APP_LABEL,
            "etiqueta sin recortar: {} chars",
            got.chars().count()
        );
    }

    #[test]
    fn resolve_app_sanea_comm_con_control_chars() {
        // comm con un newline incrustado (un proceso puede fijar su comm).
        let d = fake_proc(31, None, Some("evil\x1bhack"));
        let got = resolve_app(d.path(), 31);
        assert!(!got.chars().any(|c| c.is_control()), "quedó un control char: {got:?}");
    }

    #[test]
    fn parsea_scopes_de_escritorio() {
        assert_eq!(
            app_from_cgroup("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-org.mozilla.firefox-3422.scope"),
            Some("org.mozilla.firefox".into())
        );
        assert_eq!(
            app_from_cgroup("0::/user.slice/.../app.slice/app-konsole-91.scope"),
            Some("konsole".into())
        );
    }

    #[test]
    fn parsea_flatpak_snap_y_services() {
        assert_eq!(
            app_from_cgroup("0::/user.slice/.../app-flatpak-com.spotify.Client-22.scope"),
            Some("com.spotify.Client".into())
        );
        assert_eq!(
            app_from_cgroup("0::/system.slice/snap.firefox.firefox-a1b2.scope"),
            Some("firefox".into())
        );
        assert_eq!(
            app_from_cgroup("0::/system.slice/tailscaled.service"),
            Some("tailscaled".into())
        );
    }

    #[test]
    fn cgroup_irreconocible_da_none() {
        assert_eq!(
            app_from_cgroup("0::/user.slice/user-1000.slice/session-2.scope"),
            None
        );
        assert_eq!(app_from_cgroup(""), None);
    }

    /// /proc falso: <root>/<pid>/cgroup y <root>/<pid>/comm
    fn fake_proc(pid: u32, cgroup: Option<&str>, comm: Option<&str>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join(pid.to_string());
        std::fs::create_dir_all(&p).unwrap();
        if let Some(c) = cgroup {
            std::fs::write(p.join("cgroup"), format!("{c}\n")).unwrap();
        }
        if let Some(c) = comm {
            std::fs::write(p.join("comm"), format!("{c}\n")).unwrap();
        }
        dir
    }

    #[test]
    fn resolve_usa_cgroup_luego_comm_luego_desconocido() {
        let d = fake_proc(
            10,
            Some("0::/user.slice/app.slice/app-firefox-1.scope"),
            Some("firefox-bin"),
        );
        assert_eq!(resolve_app(d.path(), 10), "firefox");
        let d = fake_proc(11, Some("0::/user.slice/session-2.scope"), Some("curl"));
        assert_eq!(resolve_app(d.path(), 11), "curl");
        let d = fake_proc(12, None, None); // proceso ya muerto
        assert_eq!(resolve_app(d.path(), 12), "(desconocido)");
    }

    #[test]
    fn parsea_services_template_con_instancia() {
        assert_eq!(
            app_from_cgroup("0::/user.slice/.../app-code@e23928efc5ab41bf.service"),
            Some("code".into())
        );
        assert_eq!(
            app_from_cgroup("0::/user.slice/.../app-dropbox@autostart.service"),
            Some("dropbox".into())
        );
        assert_eq!(
            app_from_cgroup(r"0::/user.slice/.../app-google\x2dchrome@ce30d1.service"),
            Some("google-chrome".into())
        );
        assert_eq!(
            app_from_cgroup("0::/user.slice/.../app-flameshot.service"),
            Some("flameshot".into())
        );
    }

    #[test]
    fn cache_resuelve_una_vez_y_olvida_pids_ausentes() {
        let d = fake_proc(20, Some("0::/x/app-kate-5.scope"), None);
        let mut cache = AppCache::default();
        assert_eq!(cache.get(d.path(), 20), "kate");
        // aunque el proceso desaparezca, la caché responde
        std::fs::remove_dir_all(d.path().join("20")).unwrap();
        assert_eq!(cache.get(d.path(), 20), "kate");
        // retain solo conserva los tgids vivos del último drenado
        cache.retain(&[99]);
        assert_eq!(cache.get(d.path(), 20), "(desconocido)"); // ya sin /proc ni caché
    }
}
