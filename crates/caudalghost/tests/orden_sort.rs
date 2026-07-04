//! Tests de integración de proceso completo (spawnean el binario real via
//! `CARGO_BIN_EXE_caudalghost`) que **assertan el ORDEN de las filas** de la
//! salida CSV para `--sort`/`--reverse`. Viven en `tests/` porque el harness
//! de `cargo test` intercepta `print!` en tests in-process; spawnear el binario
//! captura el stdout real (`Command::output()`).
//!
//! Cubren los CUATRO comandos ordenables (top/hosts/total/now), cada uno con su
//! propio tipo de fila, para que si a alguno se le cae el wiring de
//! `ordenar_filas` el test correspondiente falle (el hueco que un test que solo
//! comprueba `run()==Ok` dejaba pasar).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::process::{Command, Output};

/// Levanta un daemon fake que responde `response` una vez y ejecuta el binario
/// real `caudalghost` con `args` (más `--socket <fake>`).
fn run_bin(response: &[u8], args: &[&str]) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let sock = dir.path().join("fake.sock");
    let listener = UnixListener::bind(&sock).expect("bind socket fake");
    let resp = response.to_vec();
    let handle = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("accept");
        let mut reader = BufReader::new(stream.try_clone().expect("clone"));
        let mut line = String::new();
        reader.read_line(&mut line).expect("leer petición");
        let mut s = stream;
        s.write_all(&resp).expect("responder");
    });
    let mut full = vec!["--socket", sock.to_str().unwrap()];
    full.extend_from_slice(args);
    let out = Command::new(env!("CARGO_BIN_EXE_caudalghost"))
        .args(&full)
        .output()
        .expect("spawnear caudalghost");
    handle
        .join()
        .expect("hilo del daemon fake no debe entrar en pánico");
    out
}

/// Primera columna (nombre) de cada fila de datos del CSV, saltando la cabecera.
fn nombres_csv(out: &Output) -> Vec<String> {
    assert!(
        out.status.success(),
        "el binario no debe fallar: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .skip(1) // cabecera
        .map(|l| l.split(',').next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// `a` lleva tx alto (total 101) para que el orden por `total` (default) difiera
// del orden por `rx`: así CADA test de `top` discrimina un wiring caído (si se
// cayera `ordenar_filas`, la salida volvería al default total-desc [a,b,c], que
// NO coincide con ninguno de los órdenes aserados por rx).
const TOP: &[u8] =
    b"{\"type\":\"total_apps\",\"rows\":[{\"app\":\"a\",\"rx\":1,\"tx\":100},{\"app\":\"b\",\"rx\":30,\"tx\":0},{\"app\":\"c\",\"rx\":10,\"tx\":0}]}\n";

/// `top --sort rx` = descendente (b=30, c=10, a=1). Prueba el wiring de `top` histórico.
#[test]
fn top_sort_rx_descendente() {
    let out = run_bin(
        TOP,
        &["top", "hoy", "--sort", "rx", "-o", "csv", "--no-color"],
    );
    assert_eq!(nombres_csv(&out), ["b", "c", "a"]);
}

/// `top --sort rx --reverse` = ascendente (a=1, c=10, b=30). Si se cayera el
/// `ordenar_filas` de `top`, el orden volvería al default (total desc) y esto fallaría.
#[test]
fn top_sort_rx_reverse_ascendente() {
    let out = run_bin(
        TOP,
        &[
            "top",
            "hoy",
            "--sort",
            "rx",
            "--reverse",
            "-o",
            "csv",
            "--no-color",
        ],
    );
    assert_eq!(nombres_csv(&out), ["a", "c", "b"]);
}

/// `hosts --sort host` = descendente por nombre (z, y, x). Prueba el wiring de `hosts` (RemoteRow).
#[test]
fn hosts_sort_host_descendente() {
    let resp = b"{\"type\":\"top_remotes\",\"rows\":[{\"host\":\"x\",\"rx\":5,\"tx\":0},{\"host\":\"z\",\"rx\":5,\"tx\":0},{\"host\":\"y\",\"rx\":5,\"tx\":0}]}\n";
    let out = run_bin(
        resp,
        &["hosts", "hoy", "--sort", "host", "-o", "csv", "--no-color"],
    );
    assert_eq!(nombres_csv(&out), ["z", "y", "x"]);
}

/// `total --sort total --reverse` = ascendente por total. Prueba el wiring de `total` (TotalRow).
#[test]
fn total_sort_total_reverse_ascendente() {
    let resp = b"{\"type\":\"total\",\"rows\":[{\"iface\":\"eth0\",\"rx\":1,\"tx\":0},{\"iface\":\"wlan0\",\"rx\":30,\"tx\":0},{\"iface\":\"enp0\",\"rx\":10,\"tx\":0}]}\n";
    let out = run_bin(
        resp,
        &[
            "total",
            "hoy",
            "--sort",
            "total",
            "--reverse",
            "-o",
            "csv",
            "--no-color",
        ],
    );
    assert_eq!(nombres_csv(&out), ["eth0", "enp0", "wlan0"]);
}

/// `now --sort total` = descendente por total. Prueba el wiring de `now` (ruta máquina, TotalRow).
#[test]
fn now_sort_total_descendente() {
    let resp = b"{\"type\":\"snapshot\",\"samples\":[{\"ts\":0,\"deltas\":[{\"iface\":\"eth0\",\"rx\":1,\"tx\":0},{\"iface\":\"wlan0\",\"rx\":30,\"tx\":0}],\"apps\":[],\"flows\":[]}]}\n";
    let out = run_bin(resp, &["now", "--sort", "total", "-o", "csv", "--no-color"]);
    assert_eq!(nombres_csv(&out), ["wlan0", "eth0"]);
}
