mod ipc;

use caudalghost_core::proto::{Request, Response};
use caudalghost_core::types::Sample;
use serde::Serialize;
use tauri::Emitter;

#[derive(Clone, Serialize)]
struct LiveUpdate {
    connected: bool,
    ebpf: bool,
    version: String,
    samples: Vec<Sample>,
    iface_ips: std::collections::HashMap<String, String>,
    /// Por qué no conecta cuando `connected == false` (caído / sin permiso / otro).
    /// `Ok` cuando sí conecta. Permite a la GUI ofrecer la acción correcta.
    reason: ipc::ConnReason,
}

/// Una pasada de sondeo del daemon: ping + snapshot. Fuente única de verdad para
/// el loop en vivo y para el botón "Reintentar" (comando `reconnect`).
fn poll() -> LiveUpdate {
    let pong = ipc::call(&Request::Ping);
    let snap = ipc::call(&Request::Snapshot { seconds: 60 });
    match (pong, snap) {
        (Ok(Response::Pong { version, ebpf }), Ok(Response::Snapshot { samples })) => LiveUpdate {
            connected: true,
            ebpf,
            version,
            samples,
            iface_ips: iface_ips(),
            reason: ipc::ConnReason::Ok,
        },
        _ => {
            // No respondió: clasifica el porqué. Si el socket conecta pero no
            // contesta (daemon a medio arrancar), es "otro", no "caído".
            let reason = match ipc::probe_reason() {
                ipc::ConnReason::Ok => ipc::ConnReason::Other,
                r => r,
            };
            LiveUpdate {
                connected: false,
                ebpf: false,
                version: String::new(),
                samples: vec![],
                iface_ips: Default::default(),
                reason,
            }
        }
    }
}

/// Reintento inmediato disparado por el usuario desde la GUI (botón "Reintentar").
#[tauri::command]
fn reconnect() -> LiveUpdate {
    poll()
}

fn iface_ips() -> std::collections::HashMap<String, String> {
    let mut m = std::collections::HashMap::new();
    if let Ok(addrs) = if_addrs::get_if_addrs() {
        for a in addrs {
            if let std::net::IpAddr::V4(v4) = a.addr.ip() {
                if !v4.is_loopback() {
                    m.entry(a.name).or_insert_with(|| v4.to_string());
                }
            }
        }
    }
    m
}

#[tauri::command]
fn ping() -> Result<Response, String> {
    ipc::call(&Request::Ping)
}

#[tauri::command]
fn snapshot(seconds: u32) -> Result<Response, String> {
    ipc::call(&Request::Snapshot { seconds })
}

#[derive(serde::Serialize)]
struct HistoryData {
    series: Vec<caudalghost_core::proto::SeriePoint>,
    total: Vec<caudalghost_core::proto::TotalRow>,
    apps: Vec<caudalghost_core::proto::AppRow>,
    hosts: Vec<caudalghost_core::proto::RemoteRow>,
}

#[tauri::command]
fn history(rango: String) -> Result<HistoryData, String> {
    let (start, end, bucket) =
        ipc::history_window(&rango, chrono::Local::now()).map_err(|e| e.to_string())?;
    let series = match ipc::call(&Request::Series { start, end, bucket })? {
        Response::Series { points } => points,
        Response::Error { message } => return Err(message),
        _ => return Err("respuesta inesperada (series)".into()),
    };
    let total = match ipc::call(&Request::Total { start, end })? {
        Response::Total { rows } => rows,
        Response::Error { message } => return Err(message),
        _ => return Err("respuesta inesperada (total)".into()),
    };
    let apps = match ipc::call(&Request::TotalApps { start, end })? {
        Response::TotalApps { rows } => rows,
        Response::Error { message } => return Err(message),
        _ => return Err("respuesta inesperada (apps)".into()),
    };
    let hosts = match ipc::call(&Request::TopRemotes { start, end, app: None })? {
        Response::TopRemotes { rows } => rows,
        Response::Error { message } => return Err(message),
        _ => return Err("respuesta inesperada (hosts)".into()),
    };
    Ok(HistoryData {
        series,
        total,
        apps,
        hosts,
    })
}

/// Top de hosts de un rango, filtrable por app (drilldown). app vacío → global.
#[tauri::command]
fn top_remotes(
    rango: String,
    app: Option<String>,
) -> Result<Vec<caudalghost_core::proto::RemoteRow>, String> {
    let (start, end, _bucket) =
        ipc::history_window(&rango, chrono::Local::now()).map_err(|e| e.to_string())?;
    match ipc::call(&Request::TopRemotes { start, end, app })? {
        Response::TopRemotes { rows } => Ok(rows),
        Response::Error { message } => Err(message),
        _ => Err("respuesta inesperada (top_remotes)".into()),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            ping,
            snapshot,
            history,
            top_remotes,
            reconnect
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                let _ = handle.emit("live-update", poll());
                std::thread::sleep(std::time::Duration::from_secs(1));
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error al arrancar CaudalGhost");
}
