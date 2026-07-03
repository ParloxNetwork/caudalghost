use crate::proto::Bucket;
use crate::retention::{HOUR_RETENTION_SECS, MINUTE_RETENTION_SECS};
use chrono::{DateTime, Datelike, Local, NaiveDate, TimeZone};

/// Ventana temporal [start, end) en epoch UTC.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub start: i64,
    pub end: i64,
}

// Nota: "ayer"/"dia:" asumen días de 86 400 s; en cambios DST el corte se desvía ±1 h. Aceptado en F1.
// Si la medianoche local no existe (gap de spring-forward), local_midnight propaga Err en lugar de fabricar un valor.

/// Lista canónica de tokens de rango válidos, en español, para reutilizar en
/// el `--help` de la CLI y en el mensaje de error de `resolve`.
pub fn tokens_ayuda() -> &'static str {
    "5m, 1h, 24h, 7d, 30d, 90d, 365d, hoy, ayer, este-mes, este-anio, \
     dia:YYYY-MM-DD, rango:YYYY-MM-DD:YYYY-MM-DD"
}

/// Tokens: 5m, 1h, 24h, 7d, 30d, 90d, 365d, hoy, ayer, este-mes, este-anio, dia:YYYY-MM-DD, rango:YYYY-MM-DD:YYYY-MM-DD
pub fn resolve(token: &str, now: DateTime<Local>) -> Result<Window, String> {
    let end = now.timestamp();
    let local_midnight = |d: NaiveDate| -> Result<i64, String> {
        Local
            .from_local_datetime(&d.and_hms_opt(0, 0, 0).unwrap())
            .earliest()
            .map(|t| t.timestamp())
            .ok_or_else(|| format!("medianoche local inexistente para {d} (cambio de horario)"))
    };
    let hoy0 = local_midnight(now.date_naive())?;
    let rel = |secs: i64| Window {
        start: end - secs,
        end,
    };
    let w = match token {
        "5m" => rel(300),
        "1h" => rel(3_600),
        "24h" => rel(86_400),
        "7d" => rel(7 * 86_400),
        "30d" => rel(30 * 86_400),
        "90d" => rel(90 * 86_400),
        "365d" => rel(365 * 86_400),
        "hoy" => Window { start: hoy0, end },
        "ayer" => Window {
            start: hoy0 - 86_400,
            end: hoy0,
        },
        "este-mes" => Window {
            start: local_midnight(now.date_naive().with_day(1).unwrap())?,
            end,
        },
        "este-anio" => Window {
            start: local_midnight(NaiveDate::from_ymd_opt(now.year(), 1, 1).unwrap())?,
            end,
        },
        t if t.starts_with("dia:") => {
            let d = NaiveDate::parse_from_str(&t[4..], "%Y-%m-%d")
                .map_err(|_| format!("fecha inválida en '{t}', formato dia:YYYY-MM-DD"))?;
            let s = local_midnight(d)?;
            Window {
                start: s,
                end: s + 86_400,
            }
        }
        t if t.starts_with("rango:") => {
            let rest = &t["rango:".len()..];
            let (a, b) = rest.split_once(':').ok_or_else(|| {
                format!("rango inválido '{t}', formato rango:YYYY-MM-DD:YYYY-MM-DD")
            })?;
            let d1 = NaiveDate::parse_from_str(a, "%Y-%m-%d")
                .map_err(|_| format!("fecha inicial inválida en '{t}'"))?;
            let d2 = NaiveDate::parse_from_str(b, "%Y-%m-%d")
                .map_err(|_| format!("fecha final inválida en '{t}'"))?;
            if d1 > d2 {
                return Err(format!("rango invertido: {d1} posterior a {d2}"));
            }
            Window {
                start: local_midnight(d1)?,
                end: local_midnight(d2)? + 86_400,
            }
        }
        _ => {
            return Err(format!(
                "rango desconocido: '{token}'. Válidos: {}",
                tokens_ayuda()
            ));
        }
    };
    Ok(w)
}

/// Elige el cubo temporal para graficar [start, end): respeta la granularidad
/// mínima de la capa fuente (por antigüedad de `start`, mismo umbral que
/// `storage::query_series`) y apunta a un número manejable de puntos según la
/// duración de la ventana.
pub fn pick_bucket(start: i64, end: i64, now: i64) -> Bucket {
    let min = if start >= now - MINUTE_RETENTION_SECS {
        Bucket::Minute
    } else if start >= now - HOUR_RETENTION_SECS {
        Bucket::Hour
    } else {
        Bucket::Day
    };
    let dur = (end - start).max(0);
    let desired = if dur <= 3 * 3_600 {
        Bucket::Minute
    } else if dur <= 14 * 86_400 {
        Bucket::Hour
    } else {
        Bucket::Day
    };
    desired.max(min)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    // Mié 2026-06-10 15:30:00 hora local como "ahora" fijo.
    fn now() -> chrono::DateTime<Local> {
        Local.with_ymd_and_hms(2026, 6, 10, 15, 30, 0).unwrap()
    }

    #[test]
    fn rangos_relativos() {
        let n = now().timestamp();
        assert_eq!(
            resolve("5m", now()).unwrap(),
            Window {
                start: n - 300,
                end: n
            }
        );
        assert_eq!(
            resolve("1h", now()).unwrap(),
            Window {
                start: n - 3_600,
                end: n
            }
        );
        assert_eq!(
            resolve("24h", now()).unwrap(),
            Window {
                start: n - 86_400,
                end: n
            }
        );
        assert_eq!(
            resolve("7d", now()).unwrap(),
            Window {
                start: n - 7 * 86_400,
                end: n
            }
        );
        assert_eq!(
            resolve("30d", now()).unwrap(),
            Window {
                start: n - 30 * 86_400,
                end: n
            }
        );
        assert_eq!(
            resolve("90d", now()).unwrap(),
            Window {
                start: n - 90 * 86_400,
                end: n
            }
        );
        assert_eq!(
            resolve("365d", now()).unwrap(),
            Window {
                start: n - 365 * 86_400,
                end: n
            }
        );
    }

    #[test]
    fn hoy_y_ayer_parten_de_medianoche_local() {
        let hoy = resolve("hoy", now()).unwrap();
        let medianoche = Local
            .with_ymd_and_hms(2026, 6, 10, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            hoy,
            Window {
                start: medianoche,
                end: now().timestamp()
            }
        );
        let ayer = resolve("ayer", now()).unwrap();
        assert_eq!(
            ayer,
            Window {
                start: medianoche - 86_400,
                end: medianoche
            }
        );
    }

    #[test]
    fn este_mes_y_este_anio() {
        let mes = resolve("este-mes", now()).unwrap();
        let primero = Local
            .with_ymd_and_hms(2026, 6, 1, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(mes.start, primero);
        let anio = resolve("este-anio", now()).unwrap();
        let enero = Local
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(anio.start, enero);
    }

    #[test]
    fn dia_especifico() {
        let w = resolve("dia:2026-06-01", now()).unwrap();
        let d = Local
            .with_ymd_and_hms(2026, 6, 1, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            w,
            Window {
                start: d,
                end: d + 86_400
            }
        );
        assert_eq!(w.end - w.start, 86_400);
    }

    #[test]
    fn token_invalido() {
        assert!(resolve("banana", now()).is_err());
        assert!(resolve("dia:junio", now()).is_err());
    }

    #[test]
    fn rango_de_fechas_hasta_inclusivo() {
        let w = resolve("rango:2026-06-01:2026-06-05", now()).unwrap();
        let d1 = Local
            .with_ymd_and_hms(2026, 6, 1, 0, 0, 0)
            .unwrap()
            .timestamp();
        let d5 = Local
            .with_ymd_and_hms(2026, 6, 5, 0, 0, 0)
            .unwrap()
            .timestamp();
        assert_eq!(
            w,
            Window {
                start: d1,
                end: d5 + 86_400
            }
        ); // incluye todo el 5
    }

    #[test]
    fn rango_de_un_solo_dia_equivale_a_dia() {
        let r = resolve("rango:2026-06-01:2026-06-01", now()).unwrap();
        let d = resolve("dia:2026-06-01", now()).unwrap();
        assert_eq!(r, d);
    }

    #[test]
    fn rango_invertido_o_invalido_es_error() {
        assert!(resolve("rango:2026-06-05:2026-06-01", now()).is_err()); // A > B
        assert!(resolve("rango:2026-06-01", now()).is_err()); // falta B
        assert!(resolve("rango:junio:julio", now()).is_err()); // no ISO
    }

    use crate::proto::Bucket;

    #[test]
    fn pick_bucket_rango_corto_reciente_minuto() {
        let now = 1_000_000_000;
        assert_eq!(
            crate::range::pick_bucket(now - 3_600, now, now),
            Bucket::Minute
        );
    }
    #[test]
    fn pick_bucket_dias_recientes_hora() {
        let now = 1_000_000_000;
        assert_eq!(
            crate::range::pick_bucket(now - 7 * 86_400, now, now),
            Bucket::Hour
        );
    }
    #[test]
    fn pick_bucket_rango_largo_dia() {
        let now = 1_000_000_000;
        assert_eq!(
            crate::range::pick_bucket(now - 30 * 86_400, now, now),
            Bucket::Day
        );
    }
    #[test]
    fn pick_bucket_respeta_granularidad_de_fuente_hora() {
        let now = 1_000_000_000;
        let start = now - 100 * 86_400;
        assert_eq!(
            crate::range::pick_bucket(start, start + 3_600, now),
            Bucket::Hour
        );
    }
    #[test]
    fn pick_bucket_muy_viejo_dia() {
        let now = 1_000_000_000;
        let start = now - 800 * 86_400;
        assert_eq!(
            crate::range::pick_bucket(start, start + 3_600, now),
            Bucket::Day
        );
    }

    #[test]
    fn tokens_ayuda_lista_los_validos() {
        let a = super::tokens_ayuda();
        assert!(a.contains("hoy") && a.contains("dia:") && a.contains("rango:"));
    }

    #[test]
    fn resolve_incluye_tokens_ayuda_en_el_mensaje_de_error() {
        let err = resolve("banana", now()).unwrap_err();
        assert!(err.contains("hoy") && err.contains("dia:"));
    }
}
