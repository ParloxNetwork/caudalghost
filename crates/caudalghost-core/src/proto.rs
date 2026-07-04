use crate::types::Sample;
use serde::{Deserialize, Serialize};

/// Una línea JSON por petición; el servidor responde una línea JSON.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Request {
    Ping,
    /// Últimos `seconds` de la vista en vivo (máx. 3600).
    Snapshot {
        seconds: u32,
    },
    /// Totales por interfaz en [start, end), epoch UTC.
    Total {
        start: i64,
        end: i64,
    },
    /// Totales por aplicación en [start, end), epoch UTC.
    TotalApps {
        start: i64,
        end: i64,
    },
    /// Serie temporal del caudal en [start, end) por cubos de `bucket`, epoch UTC.
    Series {
        start: i64,
        end: i64,
        bucket: Bucket,
    },
    /// Top de hosts remotos en [start, end). `app=None` → global (todas las apps).
    TopRemotes {
        start: i64,
        end: i64,
        #[serde(default)]
        app: Option<String>,
    },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong {
        version: String,
        #[serde(default)]
        ebpf: bool,
    },
    Snapshot {
        samples: Vec<Sample>,
    },
    Total {
        rows: Vec<TotalRow>,
    },
    TotalApps {
        rows: Vec<AppRow>,
    },
    Series {
        points: Vec<SeriePoint>,
    },
    TopRemotes {
        rows: Vec<RemoteRow>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TotalRow {
    pub iface: String,
    pub rx: u64,
    pub tx: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppRow {
    pub app: String,
    pub rx: u64,
    pub tx: u64,
}

/// Una fila del top de hosts remotos: dominio (eTLD+1) o IP y su tráfico.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemoteRow {
    pub host: String,
    pub rx: u64,
    pub tx: u64,
}

/// Granularidad de cubo temporal para la serie histórica.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Minute,
    Hour,
    Day,
}

impl Bucket {
    /// Segundos que abarca un cubo de esta granularidad.
    pub fn secs(self) -> i64 {
        match self {
            Bucket::Minute => 60,
            Bucket::Hour => 3_600,
            Bucket::Day => 86_400,
        }
    }
}

/// Un punto de la serie temporal: caudal agregado en un cubo (todas las
/// interfaces sumadas).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriePoint {
    pub ts: i64,
    pub rx: u64,
    pub tx: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_roundtrip() {
        let r: Request = serde_json::from_str(r#"{"cmd":"total","start":10,"end":20}"#).unwrap();
        assert!(matches!(r, Request::Total { start: 10, end: 20 }));
        let s = serde_json::to_string(&Request::Snapshot { seconds: 60 }).unwrap();
        assert_eq!(s, r#"{"cmd":"snapshot","seconds":60}"#);
    }

    #[test]
    fn response_roundtrip() {
        let resp = Response::Total {
            rows: vec![TotalRow {
                iface: "eth0".into(),
                rx: 1,
                tx: 2,
            }],
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Response::Total { ref rows } if rows.len() == 1));
    }

    #[test]
    fn sample_sin_apps_es_compatible() {
        // Wire viejo (F1) sin campo apps debe deserializar con apps = [].
        let s: crate::types::Sample = serde_json::from_str(r#"{"ts":1,"deltas":[]}"#).unwrap();
        assert!(s.apps.is_empty());
    }

    #[test]
    fn sample_sin_flows_es_compatible() {
        // Wire viejo (F1/F2) sin campo flows debe deserializar con flows = [].
        let s: crate::types::Sample =
            serde_json::from_str(r#"{"ts":1,"deltas":[],"apps":[]}"#).unwrap();
        assert!(s.flows.is_empty());
    }

    #[test]
    fn app_remote_delta_roundtrip() {
        let d = crate::types::AppRemoteDelta {
            app: "firefox".into(),
            host: "github.com".into(),
            rx: 4,
            tx: 1,
        };
        let s = serde_json::to_string(&d).unwrap();
        let back: crate::types::AppRemoteDelta = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn pong_sin_ebpf_es_compatible() {
        let r: Response = serde_json::from_str(r#"{"type":"pong","version":"0.1.0"}"#).unwrap();
        assert!(matches!(r, Response::Pong { ebpf: false, .. }));
    }

    #[test]
    fn total_apps_roundtrip() {
        let req: Request =
            serde_json::from_str(r#"{"cmd":"total_apps","start":1,"end":2}"#).unwrap();
        assert!(matches!(req, Request::TotalApps { start: 1, end: 2 }));
        let resp = Response::TotalApps {
            rows: vec![AppRow {
                app: "firefox".into(),
                rx: 1,
                tx: 2,
            }],
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Response::TotalApps { ref rows } if rows.len() == 1));
    }

    #[test]
    fn top_remotes_roundtrip() {
        // app ausente → None (global)
        let req: Request =
            serde_json::from_str(r#"{"cmd":"top_remotes","start":1,"end":2}"#).unwrap();
        assert!(matches!(
            req,
            Request::TopRemotes {
                start: 1,
                end: 2,
                app: None
            }
        ));
        // app presente → Some
        let req2: Request =
            serde_json::from_str(r#"{"cmd":"top_remotes","start":1,"end":2,"app":"firefox"}"#)
                .unwrap();
        assert!(matches!(req2, Request::TopRemotes { app: Some(ref a), .. } if a == "firefox"));
        let resp = Response::TopRemotes {
            rows: vec![RemoteRow {
                host: "github.com".into(),
                rx: 5,
                tx: 1,
            }],
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Response::TopRemotes { ref rows } if rows.len() == 1));
    }

    #[test]
    fn series_request_roundtrip() {
        let r: Request =
            serde_json::from_str(r#"{"cmd":"series","start":0,"end":600,"bucket":"hour"}"#)
                .unwrap();
        assert!(matches!(
            r,
            Request::Series {
                start: 0,
                end: 600,
                bucket: Bucket::Hour
            }
        ));
        let s = serde_json::to_string(&Request::Series {
            start: 1,
            end: 2,
            bucket: Bucket::Minute,
        })
        .unwrap();
        assert_eq!(s, r#"{"cmd":"series","start":1,"end":2,"bucket":"minute"}"#);
    }

    #[test]
    fn series_response_roundtrip() {
        let resp = Response::Series {
            points: vec![SeriePoint {
                ts: 60,
                rx: 10,
                tx: 1,
            }],
        };
        let s = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&s).unwrap();
        assert!(matches!(back, Response::Series { ref points } if points.len() == 1));
    }

    #[test]
    fn bucket_secs() {
        assert_eq!(Bucket::Minute.secs(), 60);
        assert_eq!(Bucket::Hour.secs(), 3_600);
        assert_eq!(Bucket::Day.secs(), 86_400);
    }

    /// El parser del protocolo es el límite de input NO confiable (una línea del
    /// socket). Ante entradas adversariales NUNCA debe paniquear (a lo sumo
    /// devuelve `Err`). Red de regresión determinista; el fuzzing profundo vive
    /// en `fuzz/` (cargo-fuzz).
    #[test]
    fn parser_no_paniquea_con_entradas_adversariales() {
        let anidado = "[".repeat(600); // recursión profunda → serde corta con Err, sin desbordar la pila
        let casos: Vec<String> = vec![
            "".into(),
            "{}".into(),
            "null".into(),
            "[]".into(),
            "not json".into(),
            "\u{feff}{}".into(), // BOM
            "{\"cmd\":\"desconocido\"}".into(),
            "{\"cmd\":\"total\",\"start\":99999999999999999999999,\"end\":-99999999999999}".into(),
            "{\"cmd\":\"series\",\"start\":0,\"end\":0,\"bucket\":\"siglo\"}".into(),
            "{\"cmd\":\"top_remotes\",\"start\":0,\"end\":0,\"app\":\"\\u0000\\u0000\"}".into(),
            "{\"cmd\":\"snapshot\",\"seconds\":null}".into(),
            "{\"cmd\":".into(), // truncado
            "{\"type\":\"snapshot\",\"samples\":[]} basura extra".into(),
            anidado,
        ];
        for c in &casos {
            // El Ok/Err da igual: lo que se prueba es que NO paniquea.
            let _ = serde_json::from_str::<Request>(c);
            let _ = serde_json::from_str::<Response>(c);
        }
    }
}
