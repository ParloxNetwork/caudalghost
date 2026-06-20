use caudalghost_core::proto::{AppRow, SeriePoint, TotalRow};
use rusqlite::{params, Connection};
use std::collections::HashMap;
use std::net::IpAddr;
use std::path::Path;

pub use caudalghost_core::retention::{HOUR_RETENTION_SECS, MINUTE_RETENTION_SECS};

/// Una familia de tablas escalonadas (minuto/hora/día) con su dimensión.
pub struct Family {
    pub minute: &'static str,
    pub hour: &'static str,
    pub day: &'static str,
    pub dim: &'static str,
    /// sufijo de las claves de watermark en meta ("" = legado F1 para iface)
    /// Usado por rollup::maintain (Task F2a-T5).
    pub wm_suffix: &'static str,
}

pub const IFACE_FAMILY: Family = Family {
    minute: "minute_iface",
    hour: "hour_iface",
    day: "day_iface",
    dim: "iface",
    wm_suffix: "",
};

// APP_FAMILY se usa en T5 (rollup), T8 (ingest_apps) y T9 (API TotalApps).
pub const APP_FAMILY: Family = Family {
    minute: "minute_app",
    hour: "hour_app",
    day: "day_app",
    dim: "app",
    wm_suffix: "_app",
};

/// Sufijos públicos de dos niveles más comunes (lista curada mínima — NO es la
/// PSL completa; desviación consciente por footprint, ver spec §7).
const TWO_LEVEL_SUFFIXES: &[&str] = &[
    "co.uk", "org.uk", "gov.uk", "ac.uk", "me.uk", "co.jp", "co.nz", "co.za", "com.au", "net.au",
    "org.au", "com.br", "com.mx", "com.ar", "com.co", "com.tr", "com.cn", "com.hk", "com.sg",
    "com.tw",
];

/// Dominio registrable (eTLD+1) de un host. IPs y hosts sin punto se devuelven
/// tal cual. Aproximación ligera: sufijos de 2 niveles conocidos → 3 labels;
/// resto → últimos 2 labels.
pub fn etld1(host: &str) -> String {
    if host.parse::<IpAddr>().is_ok() || !host.contains('.') {
        return host.to_string();
    }
    let labels: Vec<&str> = host.trim_end_matches('.').split('.').collect();
    let n = labels.len();
    if n <= 2 {
        return labels.join(".");
    }
    let last2 = format!("{}.{}", labels[n - 2], labels[n - 1]);
    if TWO_LEVEL_SUFFIXES.contains(&last2.as_str()) {
        format!("{}.{}", labels[n - 3], last2)
    } else {
        last2
    }
}

/// Etiqueta sintética para la cola agregada del top-N (todo lo que no entra).
pub const OTROS: &str = "(otros)";

/// Aplica el tope top-N por app: por cada app conserva los N remotos de mayor
/// (rx+tx); la cola se agrega en un único remoto "(otros)". Función pura.
///
/// **Idempotente ante una cola preexistente.** Si el input YA contiene una clave
/// `(app, "(otros)")` —porque proviene de una capa con top-N ya aplicado, p. ej.
/// el rollup minuto→hora que lee `minute_app_remote`— esa fila NO compite por un
/// puesto del top-N ni se reemite duplicada: se aparta y la cola nueva se funde
/// sobre ella, emitiendo SIEMPRE una sola `"(otros)"` por app. Reaplicar la
/// función sobre su propia salida deja totales y filas idénticos.
///
/// Esta es la garantía que evita el crash-loop por PK duplicada en el rollup
/// app_remote multinivel (regresión 2026-06-16): sin ella, una `"(otros)"` ya
/// plegada que reentra al top-N produce DOS filas para el mismo `(ts, app)`.
pub fn apply_top_n(
    acc: HashMap<(String, String), (u64, u64)>,
    n: usize,
) -> Vec<(String, String, u64, u64)> {
    // Separa hosts reales (a rankear) de la cola "(otros)" preexistente por app.
    let mut reales: HashMap<String, Vec<(String, u64, u64)>> = HashMap::new();
    let mut otros_seed: HashMap<String, (u64, u64)> = HashMap::new();
    for ((app, remote), (rx, tx)) in acc {
        if remote == OTROS {
            let e = otros_seed.entry(app).or_default();
            e.0 += rx;
            e.1 += tx;
        } else {
            reales.entry(app).or_default().push((remote, rx, tx));
        }
    }
    let mut out = Vec::new();
    for (app, mut remotes) in reales {
        remotes.sort_by_key(|r| std::cmp::Reverse(r.1 + r.2));
        // La cola parte de la "(otros)" preexistente (si la había) y le suma lo
        // que sobra del top-N. Así nunca se emite una segunda fila "(otros)".
        let mut otros = otros_seed.remove(&app).unwrap_or((0, 0));
        for (i, (remote, rx, tx)) in remotes.into_iter().enumerate() {
            if i < n {
                out.push((app.clone(), remote, rx, tx));
            } else {
                otros.0 += rx;
                otros.1 += tx;
            }
        }
        if otros != (0, 0) {
            out.push((app, OTROS.to_string(), otros.0, otros.1));
        }
    }
    // Apps cuya única presencia era una fila "(otros)" (sin hosts reales).
    for (app, otros) in otros_seed {
        if otros != (0, 0) {
            out.push((app, OTROS.to_string(), otros.0, otros.1));
        }
    }
    out
}

pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    migrate(&conn)?;
    Ok(conn)
}

/// Si la base está dañada, la respalda como *.db.corrupt y crea una nueva.
/// Solo se considera "corrupción" cuando el error SQLite es DatabaseCorrupt o
/// NotADatabase. Cualquier otro error (permisos, IO, etc.) se propaga sin tocar
/// el archivo existente.
/// Política del spec: avisar, nunca inventar datos.
pub fn open_or_recover(path: &Path) -> anyhow::Result<Connection> {
    use anyhow::Context as _;
    match open(path) {
        Ok(c) => Ok(c),
        Err(e) => {
            let is_corruption = matches!(
                &e,
                rusqlite::Error::SqliteFailure(f, _)
                    if matches!(
                        f.code,
                        rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
                    )
            );
            if !is_corruption {
                return Err(e).with_context(|| format!("no se pudo abrir {}", path.display()));
            }
            let backup = path.with_extension("db.corrupt");
            eprintln!(
                "caudalghostd: base dañada ({e}); respaldo en {} y esquema nuevo",
                backup.display()
            );
            std::fs::rename(path, &backup)?;
            Ok(open(path)?)
        }
    }
}

fn migrate(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS minute_iface (
            ts INTEGER NOT NULL, iface TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, iface)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS hour_iface (
            ts INTEGER NOT NULL, iface TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, iface)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS day_iface (
            ts INTEGER NOT NULL, iface TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, iface)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS minute_app (
            ts INTEGER NOT NULL, app TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, app)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS hour_app (
            ts INTEGER NOT NULL, app TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, app)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS day_app (
            ts INTEGER NOT NULL, app TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, app)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS minute_app_remote (
            ts INTEGER NOT NULL, app TEXT NOT NULL, remote TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, app, remote)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS hour_app_remote (
            ts INTEGER NOT NULL, app TEXT NOT NULL, remote TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, app, remote)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS day_app_remote (
            ts INTEGER NOT NULL, app TEXT NOT NULL, remote TEXT NOT NULL,
            rx INTEGER NOT NULL, tx INTEGER NOT NULL,
            PRIMARY KEY (ts, app, remote)
        ) WITHOUT ROWID;
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY, value INTEGER NOT NULL
        );",
    )
}

/// Vuelca un minuto de (app, remote). ON CONFLICT suma (reintentos tras crash).
pub fn insert_minute_app_remote(
    conn: &Connection,
    ts: i64,
    rows: &[(String, String, u64, u64)],
) -> rusqlite::Result<()> {
    let txn = conn.unchecked_transaction()?;
    {
        let mut stmt = txn.prepare_cached(
            "INSERT INTO minute_app_remote (ts, app, remote, rx, tx) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(ts, app, remote) DO UPDATE SET
               rx = minute_app_remote.rx + excluded.rx,
               tx = minute_app_remote.tx + excluded.tx",
        )?;
        for (app, remote, rx, tx) in rows {
            stmt.execute(params![ts, app, remote, rx, tx])?;
        }
    }
    txn.commit()
}

/// Top de hosts remotos en [start, end). `app=None` → global (todas las apps);
/// `app=Some(x)` → solo esa app. Mismas 3 ramas por capa que `query_total`,
/// agrupando por `remote` y ordenando por tráfico desc. Retención remoto
/// desacoplada (minuto 7 d, hora 90 d) vía `REMOTE_*_RETENTION_SECS`.
pub fn query_top_remotes(
    conn: &Connection,
    start: i64,
    end: i64,
    app: Option<&str>,
    now: i64,
) -> rusqlite::Result<Vec<(String, u64, u64)>> {
    use caudalghost_core::retention::{REMOTE_HOUR_RETENTION_SECS, REMOTE_MINUTE_RETENTION_SECS};
    let c1 = now - now.rem_euclid(86_400);
    let c2 = now - now.rem_euclid(3_600);
    let m0 = now - REMOTE_MINUTE_RETENTION_SECS;
    let h0 = now - REMOTE_HOUR_RETENTION_SECS;
    // Filtro de app opcional. Para que el set de parámetros coincida con el SQL,
    // se incluye `AND app = :app` solo cuando hay filtro.
    // Enteros (i64 que controlamos) interpolados como en `query_series`; el único
    // parámetro ligado es `:app` (puede contener comillas) cuando hay filtro.
    let af = if app.is_some() { "AND app = :app" } else { "" };
    let body = if start >= m0 {
        format!(
            "SELECT remote, rx, tx FROM minute_app_remote WHERE ts >= {start} AND ts < {end} {af}"
        )
    } else if start >= h0 {
        format!(
            "SELECT remote, rx, tx FROM hour_app_remote WHERE ts >= {start} AND ts < MIN({end}, {c2}) {af}
             UNION ALL
             SELECT remote, rx, tx FROM minute_app_remote WHERE ts >= MAX({start}, {c2}) AND ts < {end} {af}"
        )
    } else {
        format!(
            "SELECT remote, rx, tx FROM day_app_remote WHERE ts >= {start} AND ts < MIN({end}, {c1}) {af}
             UNION ALL
             SELECT remote, rx, tx FROM hour_app_remote WHERE ts >= MAX({start}, {c1}) AND ts < MIN({end}, {c2}) {af}
             UNION ALL
             SELECT remote, rx, tx FROM minute_app_remote WHERE ts >= MAX({start}, {c2}) AND ts < {end} {af}"
        )
    };
    let sql = format!(
        "SELECT remote, SUM(rx), SUM(tx) FROM ({body}) GROUP BY remote ORDER BY SUM(rx)+SUM(tx) DESC"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let map = |r: &rusqlite::Row| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
        ))
    };
    let rows = match app {
        Some(a) => stmt
            .query_map(rusqlite::named_params! { ":app": a }, map)?
            .collect::<Result<Vec<_>, _>>()?,
        None => stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// Vuelca el acumulado de un minuto en la tabla de la familia dada.
/// ON CONFLICT suma (reintentos tras crash).
fn insert_minute_family(
    conn: &Connection,
    fam: &Family,
    ts: i64,
    rows: &[(String, u64, u64)],
) -> rusqlite::Result<()> {
    let sql = format!(
        "INSERT INTO {minute} (ts, {dim}, rx, tx) VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(ts, {dim}) DO UPDATE SET
           rx = {minute}.rx + excluded.rx,
           tx = {minute}.tx + excluded.tx",
        minute = fam.minute,
        dim = fam.dim,
    );
    let txn = conn.unchecked_transaction()?;
    {
        let mut stmt = txn.prepare_cached(&sql)?;
        for (dim_val, rx, tx) in rows {
            stmt.execute(params![ts, dim_val, rx, tx])?;
        }
    }
    txn.commit()
}

/// Vuelca el acumulado de un minuto (iface). ON CONFLICT suma (reintentos tras crash).
pub fn insert_minute(
    conn: &Connection,
    ts: i64,
    rows: &[(String, u64, u64)],
) -> rusqlite::Result<()> {
    insert_minute_family(conn, &IFACE_FAMILY, ts, rows)
}

/// Vuelca el acumulado de un minuto (app). ON CONFLICT suma (reintentos tras crash).
/// Usado por el daemon en T8 (ingest_apps).
pub fn insert_minute_apps(
    conn: &Connection,
    ts: i64,
    rows: &[(String, u64, u64)],
) -> rusqlite::Result<()> {
    insert_minute_family(conn, &APP_FAMILY, ts, rows)
}

/// Totales por dimensión en [start, end), usando la familia indicada.
///
/// Tres ramas según la antigüedad de start:
/// 1. start dentro de la retención de minutos (~90 días): SOLO tabla minute.
///    Exacta para cualquier frontera — la capa fina tiene todos los datos
///    (la poda no toca los últimos 90 días) y los rollups no borran minutos.
/// 2. start dentro de la retención de horas (~2 años): hora [start, C2)
///    ∪ minuto [C2, end). Cabeza no alineada a hora → cuantización ≤1 h
///    sobre una ventana ≥90 días (error ≤0,05%): se descarta el bucket parcial.
/// 3. start más viejo: día [start, C1) ∪ hora [C1, C2)
///    ∪ minuto [C2, end). Cuantización ≤1 día sobre ventana ≥2 años.
///
/// C1 = inicio del día UTC en curso; C2 = inicio de la hora en curso.
/// Invariante (mantenido por rollup::maintain en cada minuto y al arrancar):
/// tabla hora completa hasta C2, tabla día completa hasta C1.
fn query_total_family(
    conn: &Connection,
    fam: &Family,
    start: i64,
    end: i64,
    now: i64,
) -> rusqlite::Result<Vec<(String, u64, u64)>> {
    let c1 = now - now.rem_euclid(86_400);
    let c2 = now - now.rem_euclid(3_600);
    let m0 = now - MINUTE_RETENTION_SECS;
    let h0 = now - HOUR_RETENTION_SECS;
    let dim = fam.dim;
    let minute = fam.minute;
    let hour = fam.hour;
    let day = fam.day;
    if start >= m0 {
        // Rama 1: la capa de minutos es exacta para cualquier frontera.
        let sql = format!(
            "SELECT {dim}, SUM(rx), SUM(tx) FROM {minute}
              WHERE ts >= ?1 AND ts < ?2
              GROUP BY {dim} ORDER BY {dim}",
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt
            .query_map(params![start, end], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? as u64,
                    r.get::<_, i64>(2)? as u64,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else if start >= h0 {
        // Rama 2: hora [start, C2) ∪ minuto [C2, end).
        // El clamp MIN(?2, ?3) / MAX(?1, ?3) preserva la corrección si end < C2.
        let sql = format!(
            "SELECT {dim}, SUM(rx), SUM(tx) FROM (
                SELECT {dim}, rx, tx FROM {hour}
                  WHERE ts >= ?1 AND ts < MIN(?2, ?3)
                UNION ALL
                SELECT {dim}, rx, tx FROM {minute}
                  WHERE ts >= MAX(?1, ?3) AND ts < ?2
             ) GROUP BY {dim} ORDER BY {dim}",
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt
            .query_map(params![start, end, c2], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? as u64,
                    r.get::<_, i64>(2)? as u64,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    } else {
        // Rama 3: día [start, C1) ∪ hora [C1, C2) ∪ minuto [C2, end).
        let sql = format!(
            "SELECT {dim}, SUM(rx), SUM(tx) FROM (
                SELECT {dim}, rx, tx FROM {day}
                  WHERE ts >= ?1 AND ts < MIN(?2, ?4)
                UNION ALL
                SELECT {dim}, rx, tx FROM {hour}
                  WHERE ts >= MAX(?1, ?4) AND ts < MIN(?2, ?3)
                UNION ALL
                SELECT {dim}, rx, tx FROM {minute}
                  WHERE ts >= MAX(?1, ?3) AND ts < ?2
             ) GROUP BY {dim} ORDER BY {dim}",
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt
            .query_map(params![start, end, c2, c1], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, i64>(1)? as u64,
                    r.get::<_, i64>(2)? as u64,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

/// Totales por interfaz en [start, end).
///
/// Tres ramas según la antigüedad de start:
/// 1. start dentro de la retención de minutos (~90 días): SOLO minute_iface.
///    Exacta para cualquier frontera — la capa fina tiene todos los datos
///    (la poda no toca los últimos 90 días) y los rollups no borran minutos.
/// 2. start dentro de la retención de horas (~2 años): hour_iface [start, C2)
///    ∪ minute_iface [C2, end). Cabeza no alineada a hora → cuantización ≤1 h
///    sobre una ventana ≥90 días (error ≤0,05%): se descarta el bucket parcial.
/// 3. start más viejo: day_iface [start, C1) ∪ hour_iface [C1, C2)
///    ∪ minute_iface [C2, end). Cuantización ≤1 día sobre ventana ≥2 años.
///
/// C1 = inicio del día UTC en curso; C2 = inicio de la hora en curso.
/// Invariante (mantenido por rollup::maintain en cada minuto y al arrancar):
/// hour_iface completo hasta C2, day_iface completo hasta C1.
pub fn query_total(
    conn: &Connection,
    start: i64,
    end: i64,
    now: i64,
) -> rusqlite::Result<Vec<TotalRow>> {
    let raw = query_total_family(conn, &IFACE_FAMILY, start, end, now)?;
    Ok(raw
        .into_iter()
        .map(|(iface, rx, tx)| TotalRow { iface, rx, tx })
        .collect())
}

/// Totales por aplicación en [start, end).
///
/// Mismas tres ramas por antigüedad que query_total, aplicadas sobre las
/// tablas minute_app / hour_app / day_app.
/// Usado por el API en T9 (TotalApps).
pub fn query_total_apps(
    conn: &Connection,
    start: i64,
    end: i64,
    now: i64,
) -> rusqlite::Result<Vec<AppRow>> {
    let raw = query_total_family(conn, &APP_FAMILY, start, end, now)?;
    Ok(raw
        .into_iter()
        .map(|(app, rx, tx)| AppRow { app, rx, tx })
        .collect())
}

/// Serie temporal del caudal (todas las interfaces sumadas) en [start, end),
/// re-bucketizada a la granularidad `bucket`.
///
/// Elige la tabla fuente por la antigüedad de `start` (la más fina que aún
/// conserva datos): `minute_iface` (≤90d) · `hour_iface` (≤2a) · `day_iface`
/// (resto), y agrupa por `(ts/bucket_secs)*bucket_secs`. Exige
/// `bucket ≥ granularidad de la fuente`: pedir minutos sobre datos ya
/// compactados a hora/día es un error (esa granularidad fina ya no existe).
///
/// **Limitaciones conscientes v1 (una sola tabla fuente, sin composición
/// multi-capa como `query_total_family`):**
/// - Suma todas las interfaces (filtrado de loopback = refinamiento futuro).
/// - La cabeza no alineada cae en su cubo `floor`.
/// - Si `start` excede la retención de la fuente (>90d ⇒ fuente hora; >2a ⇒
///   fuente día), el cubo PARCIAL en curso del extremo derecho aún vive en una
///   capa más fina sin rolar y NO aparece. El grueso de la serie es exacto y la
///   vista en vivo cubre "ahora"; nunca se interpola (principio 6). La exactitud
///   total exigiría componer capas para la cola (refinamiento futuro).
pub fn query_series(
    conn: &Connection,
    start: i64,
    end: i64,
    bucket: caudalghost_core::proto::Bucket,
    now: i64,
) -> anyhow::Result<Vec<SeriePoint>> {
    let m0 = now - MINUTE_RETENTION_SECS;
    let h0 = now - HOUR_RETENTION_SECS;
    let (table, src_secs) = if start >= m0 {
        (IFACE_FAMILY.minute, 60_i64)
    } else if start >= h0 {
        (IFACE_FAMILY.hour, 3_600_i64)
    } else {
        (IFACE_FAMILY.day, 86_400_i64)
    };
    let b = bucket.secs();
    if b < src_secs {
        anyhow::bail!(
            "granularidad de {b}s no disponible para ese rango (la fuente es de {src_secs}s)"
        );
    }
    // `b` es un i64 de Bucket::secs() (enum cerrado) y `table` una constante de
    // IFACE_FAMILY: ninguno es input de usuario → el format! es seguro.
    let sql = format!(
        "SELECT (ts/{b})*{b} AS bk, SUM(rx), SUM(tx) FROM {table}
          WHERE ts >= ?1 AND ts < ?2
          GROUP BY bk ORDER BY bk"
    );
    let mut stmt = conn.prepare_cached(&sql)?;
    let pts = stmt
        .query_map(params![start, end], |r| {
            Ok(SeriePoint {
                ts: r.get::<_, i64>(0)?,
                rx: r.get::<_, i64>(1)? as u64,
                tx: r.get::<_, i64>(2)? as u64,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(pts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn mem() -> rusqlite::Connection {
        open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn etld1_reduce_dominios_y_respeta_ip() {
        assert_eq!(etld1("audio-fa.scdn.co"), "scdn.co");
        assert_eq!(etld1("api.spotify.com"), "spotify.com");
        assert_eq!(etld1("example.com"), "example.com");
        assert_eq!(etld1("www.bbc.co.uk"), "bbc.co.uk"); // sufijo de 2 niveles
        assert_eq!(etld1("1.2.3.4"), "1.2.3.4"); // IP intacta
        assert_eq!(etld1("(sin nombre)"), "(sin nombre)"); // sin punto, intacto
        assert_eq!(etld1("localhost"), "localhost");
    }

    #[test]
    fn top_n_pliega_la_cola_en_otros_por_app() {
        let mut acc: HashMap<(String, String), (u64, u64)> = HashMap::new();
        acc.insert(("ff".into(), "a.com".into()), (100, 0));
        acc.insert(("ff".into(), "b.com".into()), (50, 0));
        acc.insert(("ff".into(), "c.com".into()), (10, 0));
        let mut out = apply_top_n(acc, 2);
        out.sort();
        assert!(out.contains(&("ff".to_string(), "a.com".to_string(), 100, 0)));
        assert!(out.contains(&("ff".to_string(), "b.com".to_string(), 50, 0)));
        assert!(out.contains(&("ff".to_string(), "(otros)".to_string(), 10, 0)));
        assert_eq!(out.len(), 3);
    }

    /// REGRESIÓN del crash-loop (2026-06-16): cuando el input YA trae una clave
    /// `(app, "(otros)")` —porque viene de una capa con top-N ya aplicado, p. ej.
    /// el rollup minuto→hora que lee `minute_app_remote`— `apply_top_n` debe
    /// FUNDIR esa cola preexistente en UNA sola fila `"(otros)"`, no reemitir una
    /// segunda. Antes emitía dos filas `"(otros)"` para la misma `(ts, app)` →
    /// `UNIQUE constraint failed` al insertar → el daemon moría en bucle.
    #[test]
    fn top_n_funde_otros_preexistente_sin_duplicar() {
        let mut acc: HashMap<(String, String), (u64, u64)> = HashMap::new();
        acc.insert(("ff".into(), "(otros)".into()), (1000, 100)); // cola ya plegada, grande
        acc.insert(("ff".into(), "a.com".into()), (100, 10));
        acc.insert(("ff".into(), "b.com".into()), (50, 5));
        acc.insert(("ff".into(), "c.com".into()), (10, 1));
        let out = apply_top_n(acc, 2);

        // Exactamente UNA fila "(otros)" para ff (antes había dos → choque de PK).
        let otros: Vec<_> = out.iter().filter(|(_, r, _, _)| r == "(otros)").collect();
        assert_eq!(out.iter().filter(|(a, _, _, _)| a == "ff").count(), 3); // a, b, (otros)
        assert_eq!(otros.len(), 1, "debe haber exactamente una fila (otros)");
        // El total se preserva: (otros) = preexistente(1000,100) + cola c.com(10,1).
        assert_eq!((otros[0].2, otros[0].3), (1010, 101));
        // Los top-2 reales se conservan intactos.
        assert!(out.iter().any(|(_, r, rx, tx)| r == "a.com" && *rx == 100 && *tx == 10));
        assert!(out.iter().any(|(_, r, rx, tx)| r == "b.com" && *rx == 50 && *tx == 5));

        // Sin claves (app, remote) duplicadas: la salida es insertable sin colisión.
        let mut keys: Vec<_> = out.iter().map(|(a, r, _, _)| (a.clone(), r.clone())).collect();
        let n = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), n, "no debe haber claves (app, remote) duplicadas");
    }

    /// `apply_top_n` debe ser idempotente: reaplicarla sobre su propia salida no
    /// cambia nada (mismos totales, una sola "(otros)"). Es la propiedad que
    /// garantiza que el rollup multinivel (minuto→hora→día) nunca duplique.
    #[test]
    fn top_n_es_idempotente_sobre_su_salida() {
        let mut acc: HashMap<(String, String), (u64, u64)> = HashMap::new();
        for i in 0..5 {
            acc.insert(("ff".into(), format!("h{i}.com")), (100 - i as u64, 0));
        }
        let primera = apply_top_n(acc, 2);
        let mut segunda_in: HashMap<(String, String), (u64, u64)> = HashMap::new();
        for (app, remote, rx, tx) in &primera {
            segunda_in.insert((app.clone(), remote.clone()), (*rx, *tx));
        }
        let mut segunda = apply_top_n(segunda_in, 2);
        let mut esperado = primera.clone();
        segunda.sort();
        esperado.sort();
        assert_eq!(segunda, esperado);
    }

    #[test]
    fn top_remotes_rango_largo_por_app_cruza_capas() {
        // Rango de 30 días: rama 2 (hora ∪ minuto), con :app repetido en ambos
        // sub-SELECT. Verifica que el binding por nombre funciona multi-rama.
        let conn = mem();
        let now = 100 * 86_400 + 1800; // 100º día + media hora
        let start = now - 30 * 86_400;
        // capa hora (dentro de [start, c2)) y capa minuto (hora en curso)
        conn.execute_batch(
            "INSERT INTO hour_app_remote VALUES (7002000,'firefox','github.com',100,10);
             INSERT INTO hour_app_remote VALUES (7002000,'spotify','scdn.co',200,2);
             INSERT INTO minute_app_remote VALUES (8640060,'firefox','fastly.net',7,0);",
        )
        .unwrap();
        let f = query_top_remotes(&conn, start, now, Some("firefox"), now).unwrap();
        // Solo firefox: github (de hora) + fastly (de minuto), sin scdn de spotify.
        assert_eq!(f.len(), 2);
        assert!(f.iter().any(|(h, rx, _)| h == "github.com" && *rx == 100));
        assert!(f.iter().any(|(h, rx, _)| h == "fastly.net" && *rx == 7));
        assert!(f.iter().all(|(h, _, _)| h != "scdn.co"));
    }

    #[test]
    fn app_remote_insert_y_top_global_y_por_app() {
        let conn = mem();
        insert_minute_app_remote(
            &conn,
            60,
            &[
                ("firefox".into(), "github.com".into(), 100, 10),
                ("firefox".into(), "fastly.net".into(), 5, 0),
                ("spotify".into(), "scdn.co".into(), 200, 2),
            ],
        )
        .unwrap();
        // global en [60,120): scdn.co (200) > github (100) > fastly (5)
        let g = query_top_remotes(&conn, 60, 120, None, 120).unwrap();
        assert_eq!(g[0].0, "scdn.co");
        assert_eq!((g[0].1, g[0].2), (200, 2));
        assert_eq!(g.len(), 3);
        // por-app firefox: github + fastly, sin scdn
        let f = query_top_remotes(&conn, 60, 120, Some("firefox"), 120).unwrap();
        assert_eq!(f.len(), 2);
        assert!(f.iter().all(|(h, _, _)| h != "scdn.co"));
        assert!(f.iter().any(|(h, rx, _)| h == "github.com" && *rx == 100));
    }

    #[test]
    fn abre_con_wal_y_esquema() {
        let conn = mem();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN
                 ('minute_iface','hour_iface','day_iface','meta')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn insert_minute_acumula_en_conflicto() {
        let conn = mem();
        insert_minute(&conn, 60, &[("eth0".into(), 100, 10)]).unwrap();
        insert_minute(&conn, 60, &[("eth0".into(), 50, 5)]).unwrap();
        let (rx, tx): (i64, i64) = conn
            .query_row(
                "SELECT rx, tx FROM minute_iface WHERE ts=60 AND iface='eth0'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rx, tx), (150, 15));
    }

    #[test]
    fn query_total_reciente_exacta_cruzando_frontera_de_hora() {
        let conn = mem();
        let now = 200_000_i64; // C2 = 198_000, mitad de hora
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (196500, 'eth0', 10, 1);
             INSERT INTO minute_iface VALUES (197940, 'eth0', 20, 2);
             INSERT INTO minute_iface VALUES (198060, 'eth0', 30, 3);
             INSERT INTO minute_iface VALUES (196380, 'eth0', 999, 999); -- antes de start
             -- la misma data ya rolada a hour: NO debe double-contarse
             INSERT INTO hour_iface VALUES (194400, 'eth0', 777, 77);",
        )
        .unwrap();
        // ventana tipo '1h' con start no alineado (196_400) cruzando C2
        let rows = query_total(&conn, 196_400, now, now).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].rx, rows[0].tx), (60, 6)); // 10+20+30 exacto, sin duplicar
    }

    #[test]
    fn query_total_inicio_fuera_de_minutos_compone_horas_y_minutos() {
        let conn = mem();
        let now = 86_401_800_i64; // día 1000 a las 00:30 → C1 = C2... no: C2 = 86_400_000, C1 = 86_400_000
        let start = now - 100 * 86_400; // = 77_761_800, fuera de retención de minutos (90d), dentro de horas
        conn.execute_batch(
            "INSERT INTO hour_iface VALUES (77763600, 'eth0', 100, 10);  -- dentro de [start, C2)
             INSERT INTO hour_iface VALUES (77752800, 'eth0', 999, 99);  -- antes de start → fuera (cuantización de cabeza)
             INSERT INTO minute_iface VALUES (86401200, 'eth0', 5, 1);   -- hora en curso
             INSERT INTO day_iface VALUES (86313600, 'eth0', 777, 77);   -- la rama 2 IGNORA day → no debe contarse",
        )
        .unwrap();
        let rows = query_total(&conn, start, now, now).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].rx, rows[0].tx), (105, 11));
    }

    #[test]
    fn query_total_inicio_ancestral_compone_tres_capas() {
        let conn = mem();
        let now = 86_401_800_i64; // C1 = C2 = 86_400_000 (00:30 del día 1000)
        let start = now - 800 * 86_400; // = 17_281_800, fuera de retención de horas (730d)
        conn.execute_batch(
            "INSERT INTO day_iface VALUES (17280000, 'eth0', 999, 99);   -- antes de start → fuera
             INSERT INTO day_iface VALUES (17366400, 'eth0', 50, 5);     -- dentro
             INSERT INTO day_iface VALUES (86313600, 'eth0', 300, 30);   -- día 999, dentro de [start, C1)
             INSERT INTO hour_iface VALUES (86396400, 'eth0', 888, 88);  -- su día YA está en day → no double-count
             INSERT INTO minute_iface VALUES (86401200, 'eth0', 10, 1);  -- hora en curso",
        )
        .unwrap();
        let rows = query_total(&conn, start, now, now).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].rx, rows[0].tx), (360, 36)); // 50+300+10
    }

    #[test]
    fn query_total_ventana_reciente_solo_minutos() {
        let conn = mem();
        let now = 200_000_i64;
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (199800, 'eth0', 10, 1);
             INSERT INTO minute_iface VALUES (199860, 'wlan0', 20, 2);",
        )
        .unwrap();
        let rows = query_total(&conn, now - 600, now, now).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].iface, "eth0");
        assert_eq!(rows[1].iface, "wlan0");
    }

    #[test]
    fn open_or_recover_respalda_base_corrupta() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("caudal.db");
        std::fs::write(&db, b"esto no es una base sqlite valida___").unwrap();
        let conn = open_or_recover(&db).unwrap();
        conn.execute("INSERT INTO minute_iface VALUES (0,'x',1,1)", [])
            .unwrap();
        assert!(db.with_extension("db.corrupt").exists());
    }

    #[test]
    fn open_or_recover_no_respalda_por_error_de_permisos() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("caudal.db");
        {
            // Crear base sana en journal_mode=DELETE (sin WAL) para que
            // el pragma_update("journal_mode","WAL") falle en readonly.
            // open() activa WAL, por lo que usamos Connection::open directo.
            let conn = rusqlite::Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS minute_iface (
                    ts INTEGER NOT NULL, iface TEXT NOT NULL,
                    rx INTEGER NOT NULL, tx INTEGER NOT NULL,
                    PRIMARY KEY (ts, iface)
                ) WITHOUT ROWID;",
            )
            .unwrap();
            conn.execute("INSERT INTO minute_iface VALUES (0,'x',1,1)", [])
                .unwrap();
        }
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o444)).unwrap();
        let res = open_or_recover(&db);
        assert!(
            res.is_err(),
            "error de permisos debe propagarse, no recuperarse"
        );
        assert!(
            !db.with_extension("db.corrupt").exists(),
            "la base sana no debe moverse"
        );
        // restaurar permisos para que tempdir pueda limpiar
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).unwrap();
    }

    // ─────────────────────────────────────────────────────────────────────
    // REPLAY BUG: reproducción empírica de los dos escenarios originales
    // ─────────────────────────────────────────────────────────────────────
    //
    // BUG ORIGINAL: query_total perdía el fragmento de cabeza de ventanas
    // no alineadas a fronteras UTC:
    //   - "1h cruzando frontera" devolvía 10 de 40 bytes (−75%)
    //   - "ayer" en UTC-5 devolvía ~21% de los bytes reales (−79%)
    //
    // FIX: tres ramas por antigüedad de start. start dentro de los últimos
    // 90d → Rama 1 (solo minute_iface) → exacta para cualquier frontera.
    // ─────────────────────────────────────────────────────────────────────

    /// Escenario 1: `total 1h` cruzando frontera de hora.
    ///
    /// Geometría:
    ///   now = 1_749_000_000
    ///   C2  = 1_748_998_800  (inicio hora en curso; now−C2 = 1200s = 20 min)
    ///   start = now−3600 = 1_748_996_400  (en la hora anterior, cruza C2)
    ///
    /// Minutos en [start, now):
    ///   Cabeza [start, C2): ts+60 →10rx, ts+600 →20rx, C2−60 →30rx
    ///   Cola   [C2, now):   C2+60  →40rx, C2+600 →50rx, C2+1140→60rx
    ///   Total esperado: rx=210, tx=21
    ///
    /// También se insertan:
    ///   - Un minuto FUERA de ventana (start−60): no debe contarse.
    ///   - Una fila en hour_iface de esa hora: Rama 1 la ignora (no double-count).
    ///
    /// El bug viejo devolvería solo los 3 minutos de la cola (rx=150) porque la
    /// lógica no consultaba minute_iface para la cabeza pre-C2.
    /// Con el fix: Rama 1 (start>=m0) → solo minute_iface → rx=210 exacto.
    #[test]
    fn replay_bug_1h_cruzando_frontera_de_hora() {
        let conn = mem();

        let now: i64 = 1_749_000_000;
        let c2: i64 = now - now.rem_euclid(3_600); // 1_748_998_800
        let start: i64 = now - 3_600; // 1_748_996_400

        // Sanity: start en hora anterior, dentro de retención de minutos (Rama 1)
        assert!(start < c2, "start debe cruzar C2");
        let m0 = now - MINUTE_RETENTION_SECS;
        assert!(start >= m0, "start debe estar en Rama 1 (≤90d)");

        conn.execute_batch(&format!(
            // Cabeza (hora anterior, antes de C2)
            "INSERT INTO minute_iface VALUES ({c2m60},  'eth0', 10, 1);
             INSERT INTO minute_iface VALUES ({c2m3000},'eth0', 20, 2);
             INSERT INTO minute_iface VALUES ({c2m60b}, 'eth0', 30, 3);
             -- Cola (hora en curso, después de C2)
             INSERT INTO minute_iface VALUES ({c2p60},  'eth0', 40, 4);
             INSERT INTO minute_iface VALUES ({c2p600}, 'eth0', 50, 5);
             INSERT INTO minute_iface VALUES ({c2p1140},'eth0', 60, 6);
             -- Fuera de ventana: no debe contarse
             INSERT INTO minute_iface VALUES ({before}, 'eth0', 999, 99);
             -- En hour_iface de esa hora: Rama 1 lo ignora
             INSERT INTO hour_iface   VALUES ({hbucket},'eth0', 777, 77);",
            c2m60 = start + 60,
            c2m3000 = start + 600,
            c2m60b = c2 - 60,
            c2p60 = c2 + 60,
            c2p600 = c2 + 600,
            c2p1140 = c2 + 1_140,
            before = start - 60,
            hbucket = c2 - 3_600,
        ))
        .unwrap();

        let rows = query_total(&conn, start, now, now).unwrap();

        let expected_rx: u64 = 10 + 20 + 30 + 40 + 50 + 60; // 210
        let expected_tx: u64 = 1 + 2 + 3 + 4 + 5 + 6; // 21

        assert_eq!(rows.len(), 1, "exactamente 1 interfaz (eth0)");
        assert_eq!(
            rows[0].rx, expected_rx,
            "replay_bug: Escenario 1 rx — esperado={expected_rx}, obtenido={} \
             (pérdida de cabeza si obtenido<expected_rx)",
            rows[0].rx
        );
        assert_eq!(
            rows[0].tx, expected_tx,
            "replay_bug: Escenario 1 tx — esperado={expected_tx}, obtenido={}",
            rows[0].tx
        );
    }

    /// Escenario 1b: distribución 3:1 que reproduce la proporción exacta del bug.
    ///
    /// 4 minutos de 10rx cada uno (3 en la cabeza, 1 en la cola).
    /// Bug viejo: devolvía solo 10rx (la cola, −75%).
    /// Fix correcto: devuelve 40rx.
    #[test]
    fn replay_bug_1h_proporcion_original_75_pct() {
        let conn = mem();

        let now: i64 = 1_749_000_000;
        let c2: i64 = now - now.rem_euclid(3_600);
        let start: i64 = now - 3_600;

        conn.execute_batch(&format!(
            "INSERT INTO minute_iface VALUES ({}, 'eth0', 10, 0);
             INSERT INTO minute_iface VALUES ({}, 'eth0', 10, 0);
             INSERT INTO minute_iface VALUES ({}, 'eth0', 10, 0);
             INSERT INTO minute_iface VALUES ({}, 'eth0', 10, 0);",
            start + 120,
            start + 600,
            c2 - 120,
            c2 + 120,
        ))
        .unwrap();

        let rows = query_total(&conn, start, now, now).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].rx, 40,
            "replay_bug: proporción 75% — esperado=40, obtenido={} \
             (el bug viejo devolvía solo 10, −75%)",
            rows[0].rx
        );
    }

    /// Escenario 2: `ayer` en UTC-5 (Ecuador, sin DST).
    ///
    /// Geometría:
    ///   UTC-5: medianoche local = 05:00 UTC
    ///   "ayer" = 2026-06-10T00:00 local → 2026-06-11T00:00 local
    ///          = 2026-06-10T05:00 UTC   → 2026-06-11T05:00 UTC
    ///   start_ayer = 1_781_067_600  (2026-06-10T00:00 ECT = 05:00 UTC)
    ///   end_ayer   = start + 86_400 = 1_781_154_000
    ///   now        = 2026-06-11T15:00 UTC = 1_781_190_000
    ///
    /// 24 minutos (uno por hora local), bytes: rx = h*100+50, tx = h*10+5
    ///   Total rx = Σ(h=0..23)(h*100+50) = 100*276 + 1200 = 28_800
    ///   Total tx = Σ(h=0..23)(h*10+5)  = 10*276 + 120   = 2_880
    ///
    /// start_ayer >= m0 (dentro de 90d) → Rama 1 → solo minute_iface → exacto.
    /// Bug viejo: devolvía ~21% (≈5000 de 24000 bytes en la variante uniforme).
    #[test]
    fn replay_bug_ayer_utc_minus_5() {
        let conn = mem();

        // Base del día 2026-06-10T00:00 UTC (verificado: 1_781_049_600)
        let dia_2026_06_10_utc: i64 = 1_781_049_600;
        let start_ayer: i64 = dia_2026_06_10_utc + 5 * 3_600; // 1_781_067_600
        let end_ayer: i64 = start_ayer + 86_400; // 1_781_154_000
        let now: i64 = dia_2026_06_10_utc + 86_400 + 15 * 3_600; // 1_781_190_000

        // Verificar geometría
        let m0 = now - MINUTE_RETENTION_SECS;
        assert!(start_ayer >= m0, "start_ayer debe estar en Rama 1 (≤90d)");
        assert!(end_ayer < now, "end_ayer debe estar en el pasado");

        // 24 minutos distribuidos por las 24 horas locales de "ayer"
        for h in 0i64..24 {
            let ts = start_ayer + h * 3_600 + 1_800; // minuto 30 de cada hora local
            let rx = (h * 100 + 50) as u64;
            let tx = (h * 10 + 5) as u64;
            conn.execute(
                "INSERT INTO minute_iface VALUES (?1, 'eth0', ?2, ?3)",
                params![ts, rx, tx],
            )
            .unwrap();
        }

        // Minutos fuera de ventana: no deben contarse
        conn.execute_batch(&format!(
            "INSERT INTO minute_iface VALUES ({}, 'eth0', 9999, 999);
             INSERT INTO minute_iface VALUES ({}, 'eth0', 9999, 999);",
            start_ayer - 60,
            end_ayer + 60,
        ))
        .unwrap();

        let expected_rx: u64 = (0i64..24).map(|h| h * 100 + 50).sum::<i64>() as u64; // 28_800
        let expected_tx: u64 = (0i64..24).map(|h| h * 10 + 5).sum::<i64>() as u64; // 2_880

        let rows = query_total(&conn, start_ayer, end_ayer, now).unwrap();

        assert_eq!(rows.len(), 1, "exactamente 1 interfaz (eth0)");
        assert_eq!(
            rows[0].rx,
            expected_rx,
            "replay_bug: Escenario 2 rx — esperado={expected_rx}, obtenido={} \
             (el bug viejo devolvía ~21% = {})",
            rows[0].rx,
            expected_rx * 21 / 100
        );
        assert_eq!(
            rows[0].tx, expected_tx,
            "replay_bug: Escenario 2 tx — esperado={expected_tx}, obtenido={}",
            rows[0].tx
        );
    }

    #[test]
    fn esquema_incluye_tablas_app() {
        let conn = mem();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name IN
                 ('minute_app','hour_app','day_app')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 3);
    }

    #[test]
    fn insert_minute_apps_acumula() {
        let conn = mem();
        insert_minute_apps(&conn, 60, &[("firefox".into(), 100, 10)]).unwrap();
        insert_minute_apps(&conn, 60, &[("firefox".into(), 50, 5)]).unwrap();
        let (rx, tx): (i64, i64) = conn
            .query_row(
                "SELECT rx, tx FROM minute_app WHERE ts=60 AND app='firefox'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rx, tx), (150, 15));
    }

    use caudalghost_core::proto::Bucket;

    #[test]
    fn query_series_minuto_agrega_por_cubo_y_suma_ifaces() {
        let conn = mem();
        let now = 200_000_i64;
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (199800, 'eth0', 10, 1);
             INSERT INTO minute_iface VALUES (199800, 'wlan0', 5, 1);
             INSERT INTO minute_iface VALUES (199860, 'eth0', 20, 2);",
        )
        .unwrap();
        let pts = query_series(&conn, 199_700, now, Bucket::Minute, now).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].ts, pts[0].rx, pts[0].tx), (199_800, 15, 2));
        assert_eq!((pts[1].ts, pts[1].rx, pts[1].tx), (199_860, 20, 2));
    }

    #[test]
    fn query_series_hora_rebucketiza_desde_minutos() {
        let conn = mem();
        let now = 200_000_i64;
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (194460, 'eth0', 10, 1);
             INSERT INTO minute_iface VALUES (195000, 'eth0', 20, 2);
             INSERT INTO minute_iface VALUES (198060, 'eth0', 30, 3);",
        )
        .unwrap();
        let pts = query_series(&conn, 194_400, now, Bucket::Hour, now).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].ts, pts[0].rx, pts[0].tx), (194_400, 30, 3));
        assert_eq!((pts[1].ts, pts[1].rx, pts[1].tx), (198_000, 30, 3));
    }

    #[test]
    fn query_series_bucket_demasiado_fino_para_rango_viejo_es_error() {
        let conn = mem();
        let now = 86_401_800_i64;
        let start = now - 100 * 86_400; // fuera de retención de minutos → fuente = hora
        let res = query_series(&conn, start, now, Bucket::Minute, now);
        assert!(res.is_err(), "minuto sobre fuente de hora debe ser error");
    }

    #[test]
    fn query_series_dia_desde_capa_dia() {
        let conn = mem();
        let now = 86_401_800_i64;
        let start = now - 800 * 86_400; // fuera de retención de horas → fuente = día
        conn.execute_batch(
            "INSERT INTO day_iface VALUES (17366400, 'eth0', 50, 5);
             INSERT INTO day_iface VALUES (17452800, 'eth0', 70, 7);",
        )
        .unwrap();
        let pts = query_series(&conn, start, now, Bucket::Day, now).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].ts, pts[0].rx), (17_366_400, 50));
        assert_eq!((pts[1].ts, pts[1].rx), (17_452_800, 70));
    }

    #[test]
    fn query_series_hora_desde_capa_hora() {
        let conn = mem();
        let now = 86_401_800_i64;
        let start = now - 100 * 86_400; // fuera de retención de minutos → fuente = hour_iface
        conn.execute_batch(
            "INSERT INTO hour_iface VALUES (77763600, 'eth0', 100, 10);
             INSERT INTO hour_iface VALUES (77767200, 'eth0', 40, 4);",
        )
        .unwrap();
        let pts = query_series(&conn, start, now, Bucket::Hour, now).unwrap();
        assert_eq!(pts.len(), 2);
        assert_eq!((pts[0].ts, pts[0].rx, pts[0].tx), (77_763_600, 100, 10));
        assert_eq!((pts[1].ts, pts[1].rx, pts[1].tx), (77_767_200, 40, 4));
    }

    #[test]
    fn query_series_rango_vacio_devuelve_vacio() {
        let conn = mem();
        let now = 200_000_i64;
        let pts = query_series(&conn, 100_000, 100_600, Bucket::Minute, now).unwrap();
        assert!(pts.is_empty());
    }

    #[test]
    fn query_total_apps_rama_reciente_exacta() {
        let conn = mem();
        let now = 200_000_i64;
        conn.execute_batch(
            "INSERT INTO minute_app VALUES (197940, 'firefox', 20, 2);
             INSERT INTO minute_app VALUES (198060, 'firefox', 30, 3);
             INSERT INTO minute_app VALUES (198060, 'spotify', 7, 1);
             INSERT INTO hour_app VALUES (194400, 'firefox', 777, 77); -- ya rolada: no double-count",
        )
        .unwrap();
        let rows = query_total_apps(&conn, 196_400, now, now).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].app, "firefox");
        assert_eq!((rows[0].rx, rows[0].tx), (50, 5));
        assert_eq!((rows[1].rx, rows[1].tx), (7, 1));
    }

    /// Escenario 2b: variante con 24 000 bytes uniformes (proporción original del bug).
    ///
    /// 24 minutos × 1000rx = 24_000 total.
    /// Bug viejo: devolvía ~5000 (−79%). Fix: devuelve 24_000 exactos.
    #[test]
    fn replay_bug_ayer_proporcion_original_79_pct() {
        let conn = mem();

        let dia_2026_06_10_utc: i64 = 1_781_049_600;
        let start_ayer: i64 = dia_2026_06_10_utc + 5 * 3_600;
        let end_ayer: i64 = start_ayer + 86_400;
        let now: i64 = dia_2026_06_10_utc + 86_400 + 15 * 3_600;

        for h in 0i64..24 {
            let ts = start_ayer + h * 3_600 + 1_800;
            conn.execute(
                "INSERT INTO minute_iface VALUES (?1, 'eth0', 1000, 0)",
                params![ts],
            )
            .unwrap();
        }

        let rows = query_total(&conn, start_ayer, end_ayer, now).unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].rx, 24_000,
            "replay_bug: proporción 79% — esperado=24000, obtenido={} \
             (el bug viejo devolvía ~5000, −79%)",
            rows[0].rx
        );
    }
}
