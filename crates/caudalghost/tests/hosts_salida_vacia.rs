//! Test de integración de proceso completo (spawnea el binario real via
//! `CARGO_BIN_EXE_caudalghost`) para `hosts <rango> --app <inexistente>`
//! contra un daemon fake que responde `TopRemotes { rows: [] }`.
//!
//! Vive en `tests/` (no en `src/main.rs`) porque es la única forma fiable de
//! asertar el conteo exacto de bytes de stdout: el harness de `cargo test`
//! intercepta `print!`/`println!` (incluso desde hilos hijos) vía
//! `std::io::set_output_capture` *antes* de que lleguen al fd 1 real, así
//! que un test in-process no puede distinguir "0 bytes" de "una línea vacía"
//! salvo con `--nocapture` (que no es como se invoca `cargo test` en CI).
//! Spawnear el binario evita ese problema por completo: `Command::output()`
//! captura el stdout/stderr real del proceso hijo.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::Output;

/// Levanta un daemon fake en un socket temporal que responde una única vez
/// `TopRemotes { rows: [] }`, y ejecuta el binario `caudalghost` real con
/// `hosts hoy` + los argumentos extra dados.
fn ejecutar_hosts_contra_top_remotes_vacio(extra_args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("fake.sock");
    let listener = UnixListener::bind(&sock).expect("bind socket fake");
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("leer petición");
        let mut s = stream;
        s.write_all(b"{\"type\":\"top_remotes\",\"rows\":[]}\n")
            .expect("responder top_remotes vacío");
    });

    let mut args = vec!["--socket", sock.to_str().unwrap(), "hosts", "hoy"];
    args.extend_from_slice(extra_args);
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_caudalghost"))
        .args(&args)
        .output()
        .expect("spawnear el binario caudalghost");

    handle
        .join()
        .expect("el hilo del daemon fake no debe entrar en pánico");
    out
}

/// `hosts <rango> --app <inexistente> -o ndjson` con 0 filas debe producir
/// una salida real de 0 bytes: antes del fix, el patrón
/// `print!("{}", render::emitir(...)); println!();` dejaba un `\n` suelto
/// (1 byte) aunque `emitir` devolviera `""`.
#[test]
fn hosts_app_inexistente_ndjson_sale_vacio_de_verdad() {
    let out = ejecutar_hosts_contra_top_remotes_vacio(&["--app", "noexiste", "-o", "ndjson"]);
    assert!(
        out.status.success(),
        "hosts -o ndjson no debe fallar: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.stdout.len(),
        0,
        "hosts --app inexistente -o ndjson debe dar 0 bytes de stdout, salió: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// `hosts <rango> --app <inexistente> -o csv` con 0 filas: a diferencia de
/// ndjson, CSV siempre emite la línea de cabecera (`render::emitir` nunca
/// devuelve `""` para CSV, incluso con 0 filas), así que el contrato aquí es
/// "igual que hoy": cabecera + su salto de línea final, sin bytes de más ni
/// de menos.
#[test]
fn hosts_app_inexistente_csv_mantiene_solo_la_cabecera() {
    let out = ejecutar_hosts_contra_top_remotes_vacio(&["--app", "noexiste", "-o", "csv"]);
    assert!(
        out.status.success(),
        "hosts -o csv no debe fallar: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.stdout,
        b"host,rx,tx,total\n",
        "hosts --app inexistente -o csv debe quedar en solo la cabecera + \\n, salió: {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
}

/// `hosts <rango> --app <inexistente>` en formato tabla (default) con 0
/// filas debe imprimir el aviso "sin datos para la app…" por stderr y NO una
/// tabla de cabeceras vacía por stdout.
#[test]
fn hosts_app_inexistente_tabla_muestra_aviso_y_no_tabla_vacia() {
    let out = ejecutar_hosts_contra_top_remotes_vacio(&["--app", "noexiste"]);
    assert!(
        out.status.success(),
        "hosts table no debe fallar: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        out.stdout.len(),
        0,
        "stdout debe quedar vacío (no debe imprimirse una tabla de cabeceras vacía): {:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("noexiste") && err.contains("hoy"),
        "stderr debe llevar el aviso sin datos para la app, salió: {err:?}"
    );
}
