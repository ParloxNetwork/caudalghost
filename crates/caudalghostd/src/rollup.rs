use crate::storage::{
    Family, APP_FAMILY, HOUR_RETENTION_SECS, IFACE_FAMILY, MINUTE_RETENTION_SECS,
};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;

/// Tope de hosts por (cubo, app) al rolar la familia app_remote (spec §3.4).
const REMOTE_TOP_N: usize = 50;

/// Acumulador de tráfico por (app, remote) dentro de un cubo.
type RemoteAcc = HashMap<(String, String), (u64, u64)>;

pub fn meta_get(conn: &Connection, key: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
        .optional()
}

pub fn meta_set(conn: &Connection, key: &str, value: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Punto de entrada único para el mantenimiento periódico de la base de datos.
///
/// El orden es obligatorio e irrompible, y se aplica a **cada familia** por separado:
///   1. `rollup_hours(fam)`: agrega `fam.minute` → `fam.hour`.
///   2. `rollup_days(fam)`: lee `fam.hour` (que el paso anterior acaba de completar).
///      Si se invocara antes, las horas del hueco de un apagón aún no existirían
///      y el watermark de días avanzaría de todas formas, dejando esos días incompletos
///      para siempre.
///   3. `prune(fam)`: borra solo después de que los rollups ya han agregado todo lo
///      necesario. Ejecutar prune primero podría borrar minutos/horas aún no rolados
///      tras un apagón largo, causando pérdida silenciosa de datos.
///
/// Nota de diseño: los minutos fuera de retención tras un apagón ultra-largo (> 90 días)
/// no se recuperan. Esta es una decisión consciente; el histórico de días (`day_*`)
/// nunca se poda y conserva el resumen.
pub fn maintain(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    for fam in [&IFACE_FAMILY, &APP_FAMILY] {
        rollup_hours(conn, fam, now)?;
        rollup_days(conn, fam, now)?;
        prune(conn, fam, now)?;
    }
    // Familia app_remote: dimensión compuesta (app, remote) con top-N reaplicado;
    // mismo orden obligatorio horas→días→poda. Retención desacoplada (7 d / 90 d / ∞).
    rollup_app_remote_hours(conn, now)?;
    rollup_app_remote_days(conn, now)?;
    prune_app_remote(conn, now)?;
    Ok(())
}

fn rollup_app_remote_hours(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    rollup_app_remote_level(
        conn,
        now,
        "minute_app_remote",
        "hour_app_remote",
        3_600,
        "hour_watermark_app_remote",
    )
}

fn rollup_app_remote_days(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    rollup_app_remote_level(
        conn,
        now,
        "hour_app_remote",
        "day_app_remote",
        86_400,
        "day_watermark_app_remote",
    )
}

/// Agrega `src` → `dst` por cubo `bucket`, reaplicando top-N por (cubo, app).
/// Reconstruye cada cubo afectado con DELETE+INSERT (idempotente con watermark).
fn rollup_app_remote_level(
    conn: &Connection,
    now: i64,
    src: &str,
    dst: &str,
    bucket: i64,
    wm_key: &str,
) -> rusqlite::Result<()> {
    let floor = now - now.rem_euclid(bucket);
    let wm = meta_get(conn, wm_key)?.unwrap_or(0);
    let wm_floor = wm - wm.rem_euclid(bucket);
    let sql = format!(
        "SELECT (ts/{bucket})*{bucket} AS b, app, remote, SUM(rx), SUM(tx)
           FROM {src} WHERE ts >= ?1 AND ts < ?2 GROUP BY b, app, remote"
    );
    let mut by_bucket: HashMap<i64, RemoteAcc> = HashMap::new();
    {
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![wm_floor, floor], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)? as u64,
                r.get::<_, i64>(4)? as u64,
            ))
        })?;
        for row in rows {
            let (b, app, remote, rx, tx) = row?;
            let e = by_bucket
                .entry(b)
                .or_default()
                .entry((app, remote))
                .or_default();
            e.0 += rx;
            e.1 += tx;
        }
    }
    let txn = conn.unchecked_transaction()?;
    for (b, acc) in by_bucket {
        txn.execute(&format!("DELETE FROM {dst} WHERE ts = ?1"), params![b])?;
        let capped = crate::storage::apply_top_n(acc, REMOTE_TOP_N);
        {
            // Upsert idempotente (igual que el nivel minuto, storage.rs): aunque
            // `apply_top_n` ya garantiza claves (app, remote) únicas por cubo, el
            // ON CONFLICT es defensa en profundidad — funde dentro del lote en vez
            // de violar la PK. Es la asimetría que causaba el crash-loop: aquí era
            // el único INSERT plano de la familia app_remote.
            let mut ins = txn.prepare_cached(&format!(
                "INSERT INTO {dst} (ts, app, remote, rx, tx) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(ts, app, remote) DO UPDATE SET
                   rx = {dst}.rx + excluded.rx,
                   tx = {dst}.tx + excluded.tx"
            ))?;
            for (app, remote, rx, tx) in capped {
                ins.execute(params![b, app, remote, rx, tx])?;
            }
        }
    }
    txn.commit()?;
    meta_set(conn, wm_key, floor)
}

fn prune_app_remote(conn: &Connection, now: i64) -> rusqlite::Result<()> {
    use caudalghost_core::retention::{REMOTE_HOUR_RETENTION_SECS, REMOTE_MINUTE_RETENTION_SECS};
    conn.execute(
        "DELETE FROM minute_app_remote WHERE ts < ?1",
        [now - REMOTE_MINUTE_RETENTION_SECS],
    )?;
    conn.execute(
        "DELETE FROM hour_app_remote WHERE ts < ?1",
        [now - REMOTE_HOUR_RETENTION_SECS],
    )?;
    Ok(())
}

/// Agrega minutos completos a horas para la familia dada. Solo procesa desde el watermark,
/// y nunca la hora en curso. REPLACE reconstruye la hora entera, así que
/// re-correr desde un watermark viejo es seguro (idempotente).
///
/// Nota de diseño: el watermark se alinea al bucket (`(?1 / 3600) * 3600`)
/// para que un minuto tardío dentro de la hora del watermark no se pierda
/// al re-agregar con REPLACE.
///
/// Seguridad ante crash: si el proceso muere entre REPLACE e meta_set, el
/// watermark queda en el valor anterior y la siguiente ejecución re-rola desde
/// ahí; REPLACE reconstruye el resultado idéntico (idempotente).
///
/// Se invoca solo vía `maintain`.
fn rollup_hours(conn: &Connection, fam: &Family, now: i64) -> rusqlite::Result<()> {
    let hour_floor = now - now.rem_euclid(3_600);
    let wm_key = format!("hour_watermark{}", fam.wm_suffix);
    let wm = meta_get(conn, &wm_key)?.unwrap_or(0);
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {hour} (ts, {dim}, rx, tx)
             SELECT h, {dim}, SUM(rx), SUM(tx) FROM (
                SELECT (ts / 3600) * 3600 AS h, {dim}, rx, tx
                  FROM {minute}
                 WHERE ts >= (?1 / 3600) * 3600 AND ts < ?2
             ) GROUP BY h, {dim}",
            hour = fam.hour,
            dim = fam.dim,
            minute = fam.minute,
        ),
        params![wm, hour_floor],
    )?;
    meta_set(conn, &wm_key, hour_floor)
}

/// Agrega horas completas a días UTC para la familia dada. Misma mecánica de watermark.
/// Lee de `fam.hour`, por lo que debe invocarse después de `rollup_hours`.
///
/// Se invoca solo vía `maintain`.
fn rollup_days(conn: &Connection, fam: &Family, now: i64) -> rusqlite::Result<()> {
    let day_floor = now - now.rem_euclid(86_400);
    let wm_key = format!("day_watermark{}", fam.wm_suffix);
    let wm = meta_get(conn, &wm_key)?.unwrap_or(0);
    conn.execute(
        &format!(
            "INSERT OR REPLACE INTO {day} (ts, {dim}, rx, tx)
             SELECT d, {dim}, SUM(rx), SUM(tx) FROM (
                SELECT (ts / 86400) * 86400 AS d, {dim}, rx, tx
                  FROM {hour}
                 WHERE ts >= (?1 / 86400) * 86400 AND ts < ?2
             ) GROUP BY d, {dim}",
            day = fam.day,
            dim = fam.dim,
            hour = fam.hour,
        ),
        params![wm, day_floor],
    )?;
    meta_set(conn, &wm_key, day_floor)
}

/// Poda por retención para la familia dada. La tabla de días no se poda jamás (histórico indefinido).
/// Debe invocarse después de los rollups para no borrar datos aún no agregados.
///
/// Se invoca solo vía `maintain`.
fn prune(conn: &Connection, fam: &Family, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        &format!("DELETE FROM {} WHERE ts < ?1", fam.minute),
        [now - MINUTE_RETENTION_SECS],
    )?;
    conn.execute(
        &format!("DELETE FROM {} WHERE ts < ?1", fam.hour),
        [now - HOUR_RETENTION_SECS],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage;
    use std::path::Path;

    fn mem() -> rusqlite::Connection {
        storage::open(Path::new(":memory:")).unwrap()
    }

    #[test]
    fn rollup_app_remote_agrega_y_reaplica_top_n() {
        let conn = mem();
        conn.execute_batch(
            "INSERT INTO minute_app_remote VALUES (60,'ff','github.com',100,10);
             INSERT INTO minute_app_remote VALUES (120,'ff','github.com',50,5);
             INSERT INTO minute_app_remote VALUES (60,'ff','fastly.net',1,0);",
        )
        .unwrap();
        rollup_app_remote_hours(&conn, 3700).unwrap(); // hora 0 completa
        let (rx, tx): (i64, i64) = conn
            .query_row(
                "SELECT rx, tx FROM hour_app_remote WHERE ts=0 AND app='ff' AND remote='github.com'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rx, tx), (150, 15));
    }

    /// Inserta en `minute_app_remote` una fila `"(otros)"` preexistente + más de
    /// REMOTE_TOP_N hosts reales para la misma app, dentro del mismo cubo de hora.
    fn siembra_minuto_con_otros(conn: &rusqlite::Connection) {
        let mut sql =
            String::from("INSERT INTO minute_app_remote VALUES (60,'ff','(otros)',9999,999);");
        for i in 0..(REMOTE_TOP_N as u64 + 10) {
            sql.push_str(&format!(
                "INSERT INTO minute_app_remote VALUES (60,'ff','h{i}.com',{},0);",
                100 + i
            ));
        }
        conn.execute_batch(&sql).unwrap();
    }

    /// REGRESIÓN del crash-loop (2026-06-16), nivel HORA: el rollup lee minutos
    /// que YA contienen una fila `"(otros)"` y reaplica top-N. Sin idempotencia
    /// generaba DOS filas `"(otros)"` por `(ts, app)` → `UNIQUE constraint failed:
    /// hour_app_remote...` → el daemon moría. Debe rolar sin error y dejar UNA.
    #[test]
    fn rollup_app_remote_horas_no_peta_con_otros_preexistente() {
        let conn = mem();
        siembra_minuto_con_otros(&conn);
        rollup_app_remote_hours(&conn, 3700).unwrap(); // antes: Err(UNIQUE constraint failed)
        let n_otros: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM hour_app_remote WHERE ts=0 AND app='ff' AND remote='(otros)'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_otros, 1, "exactamente una fila (otros) en la hora");
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM hour_app_remote WHERE ts=0 AND app='ff'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total as usize, REMOTE_TOP_N + 1, "top-N reales + 1 (otros)");
    }

    /// REGRESIÓN del crash-loop, nivel DÍA y end-to-end: tras limpiar las horas en
    /// el portátil, el choque se MUDÓ a `day_app_remote` (confirmado 2026-06-16).
    /// `maintain` debe rolar minuto→hora→día completo sin violar la PK en NINGÚN
    /// nivel, dejando una sola `"(otros)"` en cada tabla.
    #[test]
    fn maintain_no_peta_con_otros_en_app_remote_hasta_dias() {
        let conn = mem();
        siembra_minuto_con_otros(&conn);
        maintain(&conn, 2 * 86_400).unwrap(); // dos días después: rola todo el camino
        for tbl in ["hour_app_remote", "day_app_remote"] {
            let n_otros: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {tbl} WHERE app='ff' AND remote='(otros)'"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n_otros, 1, "{tbl}: exactamente una fila (otros)");
        }
    }

    #[test]
    fn agrega_minutos_a_horas_completas() {
        let conn = mem();
        // Dos minutos de la hora 0 y uno de la hora en curso (no debe rolarse).
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (60, 'eth0', 100, 10);
             INSERT INTO minute_iface VALUES (120, 'eth0', 50, 5);
             INSERT INTO minute_iface VALUES (3660, 'eth0', 999, 99);",
        )
        .unwrap();
        rollup_hours(&conn, &IFACE_FAMILY, 3_700).unwrap(); // hora en curso = [3600, 7200)
        let (rx, tx): (i64, i64) = conn
            .query_row(
                "SELECT rx, tx FROM hour_iface WHERE ts=0 AND iface='eth0'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rx, tx), (150, 15));
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM hour_iface", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn watermark_evita_reagregar_y_cubre_huecos() {
        let conn = mem();
        conn.execute_batch("INSERT INTO minute_iface VALUES (60, 'eth0', 100, 10);")
            .unwrap();
        rollup_hours(&conn, &IFACE_FAMILY, 3_700).unwrap();
        assert_eq!(meta_get(&conn, "hour_watermark").unwrap(), Some(3_600));
        // Llega un minuto tardío de una hora ya rolada (daemon estuvo apagado):
        // al reprocesar desde un watermark anterior manual, el REPLACE lo absorbe.
        conn.execute_batch("INSERT INTO minute_iface VALUES (120, 'eth0', 7, 3);")
            .unwrap();
        meta_set(&conn, "hour_watermark", 0).unwrap();
        rollup_hours(&conn, &IFACE_FAMILY, 3_700).unwrap();
        let (rx, tx): (i64, i64) = conn
            .query_row("SELECT rx, tx FROM hour_iface WHERE ts=0", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((rx, tx), (107, 13));
    }

    #[test]
    fn agrega_horas_a_dias_utc() {
        let conn = mem();
        conn.execute_batch(
            "INSERT INTO hour_iface VALUES (0, 'eth0', 10, 1);
             INSERT INTO hour_iface VALUES (3600, 'eth0', 20, 2);",
        )
        .unwrap();
        rollup_days(&conn, &IFACE_FAMILY, 90_000).unwrap(); // día en curso = [86400, 172800)
        let (rx, tx): (i64, i64) = conn
            .query_row("SELECT rx, tx FROM day_iface WHERE ts=0", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((rx, tx), (30, 3));
    }

    #[test]
    fn prune_borra_segun_retencion() {
        let conn = mem();
        let now = 1_000 * 86_400; // día 1000
        conn.execute_batch(&format!(
            "INSERT INTO minute_iface VALUES ({}, 'e', 1, 1);
             INSERT INTO minute_iface VALUES ({}, 'e', 1, 1);
             INSERT INTO hour_iface VALUES (0, 'e', 1, 1);
             INSERT INTO day_iface VALUES (0, 'e', 1, 1);",
            now - storage::MINUTE_RETENTION_SECS - 60, // viejo → fuera
            now - 60,                                  // reciente → queda
        ))
        .unwrap();
        prune(&conn, &IFACE_FAMILY, now).unwrap();
        let m: i64 = conn
            .query_row("SELECT COUNT(*) FROM minute_iface", [], |r| r.get(0))
            .unwrap();
        let h: i64 = conn
            .query_row("SELECT COUNT(*) FROM hour_iface", [], |r| r.get(0))
            .unwrap();
        let d: i64 = conn
            .query_row("SELECT COUNT(*) FROM day_iface", [], |r| r.get(0))
            .unwrap();
        assert_eq!((m, h, d), (1, 0, 1)); // day jamás se poda
    }

    #[test]
    fn maintain_rola_tambien_la_familia_app() {
        let conn = mem();
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (60, 'eth0', 100, 10);
             INSERT INTO minute_app VALUES (60, 'firefox', 40, 4);
             INSERT INTO minute_app VALUES (120, 'firefox', 60, 6);",
        )
        .unwrap();
        maintain(&conn, 2 * 86_400).unwrap(); // dos días después: todo rolado
        let (rx, tx): (i64, i64) = conn
            .query_row(
                "SELECT rx, tx FROM day_app WHERE ts=0 AND app='firefox'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((rx, tx), (100, 10));
        // y la familia iface sigue funcionando
        let (rx, _): (i64, i64) = conn
            .query_row("SELECT rx, tx FROM day_iface WHERE ts=0", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(rx, 100);
    }

    #[test]
    fn watermarks_de_familias_son_independientes() {
        let conn = mem();
        maintain(&conn, 7_200).unwrap();
        assert_eq!(meta_get(&conn, "hour_watermark").unwrap(), Some(7_200));
        assert_eq!(meta_get(&conn, "hour_watermark_app").unwrap(), Some(7_200));
    }

    #[test]
    fn maintain_cubre_apagon_multidia() {
        let conn = mem();
        // Minutos de dos días distintos (día 1 y día 2), daemon "apagado" desde entonces.
        conn.execute_batch(
            "INSERT INTO minute_iface VALUES (86460, 'eth0', 100, 10);
             INSERT INTO minute_iface VALUES (90060, 'eth0', 50, 5);
             INSERT INTO minute_iface VALUES (172860, 'eth0', 200, 20);",
        )
        .unwrap();
        // Arranque en el día 5: una sola llamada debe dejar day_iface completo.
        maintain(&conn, 5 * 86_400 + 100).unwrap();
        let d1: (i64, i64) = conn
            .query_row("SELECT rx, tx FROM day_iface WHERE ts=86400", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let d2: (i64, i64) = conn
            .query_row("SELECT rx, tx FROM day_iface WHERE ts=172800", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(d1, (150, 15));
        assert_eq!(d2, (200, 20));
    }
}
