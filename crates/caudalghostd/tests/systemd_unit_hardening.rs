//! Test de regresión del endurecimiento de la unit systemd (heredado de 0.3.0).
//! Fija como CONTRATO que (a) las directivas de seguridad críticas siguen
//! presentes y (b) las "landmines" que rompen el kprobe eBPF no se reintroducen
//! como directivas activas. NO arranca el daemon; solo lee `dist/systemd/`.
//!
//! Guardarraíl de la 0.8.0 "no perder lo ganado": si alguien degrada la unit en
//! un release futuro, este test lo caza en `cargo test` antes de empaquetar.

/// Líneas "activas" de la unit (sin comentarios ni vacías, ya trimmeadas). Las
/// landmines se documentan en COMENTARIOS, así que filtrarlos es imprescindible
/// para no dar falsos positivos.
fn active_lines() -> Vec<String> {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dist/systemd/caudalghostd.service"
    );
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("no pude leer la unit {path}: {e}"));
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .collect()
}

#[test]
fn unit_conserva_controles_de_seguridad_criticos() {
    let lines = active_lines();
    let requeridas = [
        "User=caudalghost",
        "Group=caudalghost",
        "AmbientCapabilities=CAP_BPF CAP_PERFMON",
        "CapabilityBoundingSet=CAP_BPF CAP_PERFMON",
        "NoNewPrivileges=yes",
        "RestrictAddressFamilies=AF_UNIX AF_NETLINK",
        "ProtectSystem=strict",
        "SystemCallFilter=@system-service bpf perf_event_open",
    ];
    for req in requeridas {
        assert!(
            lines.iter().any(|l| l == req),
            "REGRESIÓN de seguridad: falta la directiva `{req}` en la unit systemd"
        );
    }
}

#[test]
fn unit_no_reintroduce_landmines_que_rompen_ebpf() {
    let lines = active_lines();
    // Estas directivas ACTIVAS rompen el kprobe eBPF (documentadas en la unit como
    // "NO poner"); solo deben existir en comentarios, ya filtrados arriba.
    let prohibidas = [
        "MemoryDenyWriteExecute",
        "PrivateUsers",
        "DynamicUser",
        "ProtectProc=invisible",
    ];
    for l in &lines {
        for p in prohibidas {
            assert!(
                !l.contains(p),
                "REGRESIÓN: la directiva `{p}` está ACTIVA en la unit (rompe eBPF): `{l}`"
            );
        }
        assert!(
            !(l.starts_with("SystemCallFilter")
                && (l.contains("~@privileged") || l.contains("~@resources"))),
            "REGRESIÓN: SystemCallFilter en denylist ~@privileged/~@resources mata bpf(): `{l}`"
        );
    }
}

#[test]
fn edicion_server_mantiene_acl_restringida_por_defecto() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../dist/systemd/server-acl-off.conf"
    );
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("no pude leer {path}: {e}"));
    assert!(
        text.contains("CAUDALGHOST_ACL_ACTIVE_USERS=0"),
        "REGRESIÓN: la edición servidor debe fijar CAUDALGHOST_ACL_ACTIVE_USERS=0 por defecto"
    );
}
