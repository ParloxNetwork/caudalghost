//! Daemon de CaudalGhost: registra tráfico por interfaz. Solo observa.
//!
//! La lógica vive en la librería (módulos `pub`) para que los helpers puros
//! sean API pública testeable y reutilizable; `main.rs` es solo el arranque.

pub mod access;
pub mod api;
pub mod appmap;
pub mod collector;
pub mod daemon;
pub mod ebpf;
pub mod live;
pub mod notify;
pub mod preflight;
pub mod resolve;
pub mod rollup;
pub mod storage;

use clap::Parser;
use live::LiveRing;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Daemon de CaudalGhost: registra tráfico por interfaz. Solo observa.
#[derive(Parser)]
#[command(name = "caudalghostd", version)]
pub struct Opts {
    /// Ruta de la base SQLite (default: XDG data)
    #[arg(long)]
    pub db: Option<PathBuf>,
    /// Ruta del socket Unix (default: XDG runtime)
    #[arg(long)]
    pub socket: Option<PathBuf>,
    /// Raíz de sysfs para tests (no usar en producción)
    #[arg(long, default_value = "/sys/class/net", hide = true)]
    pub sysfs: PathBuf,
    /// Segundos entre muestras
    #[arg(long, default_value_t = 1)]
    pub interval_secs: u64,
    /// Desactiva eBPF (modo solo-interfaz)
    #[arg(long)]
    pub no_ebpf: bool,
}

pub fn run(opts: Opts) -> anyhow::Result<()> {
    let db_path = opts
        .db
        .unwrap_or_else(caudalghost_core::paths::resolve_db_path);
    let socket_path = opts
        .socket
        .unwrap_or_else(caudalghost_core::paths::resolve_client_socket);
    if let Some(p) = db_path.parent() {
        std::fs::create_dir_all(p)?;
    }

    // Validación de requisitos: avisa honesto, NO aborta (el daemon degrada).
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .and_then(|s| preflight::parse_kernel_version(s.trim()));
    for w in preflight::preflight_warnings(
        kernel,
        preflight::path_present(std::path::Path::new("/sys/kernel/btf/vmlinux")),
        preflight::path_present(std::path::Path::new("/run/systemd/system")),
    ) {
        eprintln!("caudalghostd: aviso de requisitos — {w}");
    }

    let conn = storage::open_or_recover(&db_path)?;
    let ring = Arc::new(Mutex::new(LiveRing::new(3600)));

    let mut engine = if opts.no_ebpf {
        eprintln!("caudalghostd: eBPF desactivado por --no-ebpf (modo solo-interfaz)");
        None
    } else {
        match ebpf::EbpfEngine::load() {
            Ok(e) => {
                eprintln!("caudalghostd: eBPF activo (atribución por app)");
                Some(e)
            }
            Err(e) => {
                eprintln!(
                    "caudalghostd: eBPF no disponible ({e}); modo solo-interfaz. \
                     Ejecuta con privilegios (CAP_BPF+CAP_PERFMON o root) para atribución por app."
                );
                None
            }
        }
    };
    let ebpf_active = engine.is_some();

    // Enlaza el socket ANTES de señalar readiness, para que ningún cliente
    // corra una carrera contra un socket que aún no escucha.
    let listener = api::bind(&socket_path)?;
    // Acceso sin re-login (D11): concede el socket a los usuarios logueados por
    // UID. Si no hay logind o falla el ACL, se degrada al grupo (no aborta).
    let mut acl_failing = false;
    if let Err(e) = access::sync_socket_acl(&socket_path, &access::active_user_uids()) {
        eprintln!("caudalghostd: ACL del socket no aplicado ({e}); se usa el grupo caudalghost.");
        acl_failing = true;
    }
    {
        let ring = ring.clone();
        let db = db_path.to_string_lossy().into_owned();
        std::thread::spawn(move || {
            if let Err(e) = api::serve(listener, ring, db, ebpf_active) {
                eprintln!("caudalghostd: servidor API caído: {e}");
            }
        });
    }

    let stop = Arc::new(AtomicBool::new(false));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, stop.clone())?;
    signal_hook::flag::register(signal_hook::consts::SIGINT, stop.clone())?;

    eprintln!(
        "caudalghostd v{}: db={} socket={} ebpf={}",
        env!("CARGO_PKG_VERSION"),
        db_path.display(),
        socket_path.display(),
        ebpf_active,
    );

    // systemd: base abierta + eBPF intentado + socket escuchando → READY=1.
    notify::ready();

    let mut d = daemon::Daemon::new(conn, ring);
    let mut cache = appmap::AppCache::default();
    // Resolver DNS: solo útil con eBPF (sin flujos no hay nada que resolver).
    // Inerte si systemd-resolved no está o falta privilegio → se usan IPs crudas.
    let resolver = if ebpf_active {
        Some(resolve::ResolvedResolver::start())
    } else {
        None
    };

    // Watchdog opcional: latido a la mitad de WatchdogSec si systemd lo activó.
    let wd_interval = notify::watchdog_ping_interval();
    let mut last_wd = std::time::Instant::now();
    // Refresco del ACL cada ~5 s (capta logins/logouts nuevos).
    let acl_every = Duration::from_secs(5);
    let mut last_acl = std::time::Instant::now();

    let mut tick_n: u64 = 0;
    while !stop.load(Ordering::Relaxed) {
        match collector::read_counters(&opts.sysfs) {
            Ok(counters) => d.tick(counters, api::epoch_now())?,
            Err(e) => eprintln!("caudalghostd: error leyendo sysfs: {e}"),
        }
        tick_n += 1;
        // drenado eBPF cada 2 ticks (= 2·interval_secs segundos)
        if tick_n.is_multiple_of(2) {
            if let Some(eng) = engine.as_mut() {
                match eng.drain() {
                    Ok(drained) => {
                        let live: Vec<u32> = drained.iter().map(|(k, _)| k.tgid).collect();
                        let apps = ebpf::aggregate_by_app(&drained, &mut |tgid| {
                            cache.get(std::path::Path::new("/proc"), tgid)
                        });
                        let flows = ebpf::aggregate_by_app_remote(
                            &drained,
                            &mut |tgid| cache.get(std::path::Path::new("/proc"), tgid),
                            &mut |k| match resolver.as_ref() {
                                Some(r) => ebpf::host_label_with(k, r),
                                None => ebpf::flow_host_label(k),
                            },
                        );
                        cache.retain(&live);
                        d.ingest_apps(apps);
                        d.ingest_flows(flows);
                    }
                    Err(e) => eprintln!("caudalghostd: error drenando eBPF: {e}"),
                }
            }
        }
        // Latido del watchdog si toca.
        if let Some(iv) = wd_interval {
            if last_wd.elapsed() >= iv {
                notify::watchdog_ping();
                last_wd = std::time::Instant::now();
            }
        }
        // Refresco del ACL: capta logins/logouts sin reiniciar el daemon.
        if last_acl.elapsed() >= acl_every {
            match access::sync_socket_acl(&socket_path, &access::active_user_uids()) {
                Ok(()) => acl_failing = false,
                Err(e) => {
                    if !acl_failing {
                        eprintln!(
                            "caudalghostd: refresco de ACL falló ({e}); se usa el grupo caudalghost \
                             (no se repetirá hasta que cambie)."
                        );
                        acl_failing = true;
                    }
                }
            }
            last_acl = std::time::Instant::now();
        }
        std::thread::sleep(Duration::from_secs(opts.interval_secs));
    }
    notify::stopping(); // STOPPING=1 antes del flush final
    d.flush_pending()?; // no perder el minuto en curso al apagar
    let _ = std::fs::remove_file(&socket_path);
    Ok(())
}
