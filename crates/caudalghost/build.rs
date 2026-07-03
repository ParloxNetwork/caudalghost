use std::process::Command;

fn main() {
    let hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "desconocido".into());

    // Fecha de build: fuente primaria es `date -u` (fecha ISO real del
    // build). Si el entorno provee SOURCE_DATE_EPOCH (reproducible builds),
    // se prioriza y se convierte a ISO con el propio `date` (así el build
    // sigue siendo reproducible sin depender de crates extra). Si todo
    // falla, degrada a "desconocido" sin romper el build.
    let fecha = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|epoch| {
            Command::new("date")
                .args(["-u", "-d", &format!("@{epoch}"), "+%Y-%m-%d"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .or_else(|| {
            Command::new("date")
                .args(["-u", "+%Y-%m-%d"])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .filter(|s| !s.is_empty())
        })
        .unwrap_or_else(|| "desconocido".into());

    println!("cargo:rustc-env=CG_GIT_HASH={hash}");
    println!("cargo:rustc-env=CG_BUILD_DATE={fecha}");
    println!("cargo:rerun-if-changed=.git/HEAD");
}
