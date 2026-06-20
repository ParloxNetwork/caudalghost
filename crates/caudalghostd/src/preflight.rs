//! Validación de requisitos para la atribución por app (eBPF). Comprobaciones
//! baratas con avisos honestos; el daemon NO aborta — degrada a solo-interfaz.

use std::path::Path;

/// Parsea la *release* del kernel ("7.0.0-22-generic", "6.8.0-91") → (major, minor).
pub fn parse_kernel_version(uname_release: &str) -> Option<(u32, u32)> {
    let mut it = uname_release.split(['.', '-']);
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    Some((major, minor))
}

/// ¿El kernel separa CAP_BPF/CAP_PERFMON de CAP_SYS_ADMIN? (≥ 5.8)
pub fn kernel_has_split_caps(major: u32, minor: u32) -> bool {
    (major, minor) >= (5, 8)
}

/// ¿Existe la ruta? (para BTF `/sys/kernel/btf/vmlinux` y systemd `/run/systemd/system`).
pub fn path_present(p: &Path) -> bool {
    p.exists()
}

/// Reúne los avisos de requisitos para loguear al arrancar (no aborta).
pub fn preflight_warnings(kernel: Option<(u32, u32)>, btf: bool, systemd: bool) -> Vec<String> {
    let mut w = Vec::new();
    match kernel {
        Some((maj, min)) if !kernel_has_split_caps(maj, min) => w.push(format!(
            "kernel {maj}.{min} < 5.8: sin CAP_BPF/CAP_PERFMON separadas; \
             la atribución por app puede exigir más privilegio."
        )),
        None => w.push("no pude leer la versión del kernel.".to_string()),
        _ => {}
    }
    if !btf {
        w.push(
            "falta el BTF del kernel (/sys/kernel/btf/vmlinux): \
             la atribución por app (eBPF) no estará disponible."
                .to_string(),
        );
    }
    if !systemd {
        w.push(
            "systemd no detectado: la instalación como servicio no aplica; \
             usa el arranque manual."
                .to_string(),
        );
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsea_version_de_kernel() {
        assert_eq!(parse_kernel_version("7.0.0-22-generic"), Some((7, 0)));
        assert_eq!(parse_kernel_version("6.8.0-91"), Some((6, 8)));
        assert_eq!(parse_kernel_version("5.15.0"), Some((5, 15)));
        assert_eq!(parse_kernel_version("basura"), None);
    }

    #[test]
    fn split_caps_desde_5_8() {
        assert!(kernel_has_split_caps(7, 0));
        assert!(kernel_has_split_caps(5, 8));
        assert!(!kernel_has_split_caps(5, 7));
        assert!(!kernel_has_split_caps(4, 19));
    }

    #[test]
    fn detecta_presencia_de_fichero() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("vmlinux");
        assert!(!path_present(&f));
        std::fs::write(&f, b"x").unwrap();
        assert!(path_present(&f));
    }

    #[test]
    fn warnings_kernel_viejo_y_sin_btf() {
        let w = preflight_warnings(Some((5, 7)), false, true);
        assert_eq!(w.len(), 2);
        assert!(w[0].contains("5.7"));
        assert!(w[1].contains("BTF"));
        assert!(preflight_warnings(Some((7, 0)), true, true).is_empty());
        assert!(preflight_warnings(None, true, false).len() == 2);
    }
}
