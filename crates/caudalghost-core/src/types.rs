use serde::{Deserialize, Serialize};

/// Contadores absolutos de una interfaz en un instante (lectura cruda de sysfs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfaceCounters {
    pub iface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

/// Bytes movidos por una interfaz en un intervalo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IfaceDelta {
    pub iface: String,
    pub rx: u64,
    pub tx: u64,
}

/// Bytes movidos por una aplicación en un intervalo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppDelta {
    pub app: String,
    pub rx: u64,
    pub tx: u64,
}

/// Bytes movidos por una app hacia un host remoto en un intervalo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRemoteDelta {
    pub app: String,
    pub host: String,
    pub rx: u64,
    pub tx: u64,
}

/// Muestra por-segundo de la vista en vivo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub ts: i64,
    pub deltas: Vec<IfaceDelta>,
    /// Por-app: solo se llena en ticks de drenado eBPF (~cada 2 s); vacío entre ellos.
    #[serde(default)]
    pub apps: Vec<AppDelta>,
    /// Por (app, host remoto): solo se llena en ticks de drenado eBPF; vacío entre ellos.
    #[serde(default)]
    pub flows: Vec<AppRemoteDelta>,
}
