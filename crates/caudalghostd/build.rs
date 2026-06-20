// DESVIACIÓN del plan (Task 1 spike):
// aya-build 0.1.3 cambió la API respecto al patrón que asumía el plan:
//   1. `aya_build::cargo_metadata` ya NO se re-exporta -> usamos el crate
//      `cargo_metadata` directamente como build-dependency.
//   2. `build_ebpf` ahora recibe `aya_build::Package` (struct con campos
//      `&'a str`), NO un `cargo_metadata::Package`, y un 2º argumento
//      `Toolchain`. Construimos el `aya_build::Package` a mano a partir de
//      los metadatos del workspace.
use anyhow::{anyhow, Context as _};
use aya_build::{Package, Toolchain};
use cargo_metadata::MetadataCommand;

fn main() -> anyhow::Result<()> {
    let metadata = MetadataCommand::new()
        .no_deps()
        .exec()
        .context("MetadataCommand::exec")?;
    let ebpf_pkg = metadata
        .packages
        .iter()
        .find(|p| p.name.as_str() == "caudalghost-ebpf")
        .ok_or_else(|| anyhow!("caudalghost-ebpf no encontrado en el workspace"))?;
    // El root_dir del crate es el directorio que contiene su Cargo.toml.
    let root_dir = ebpf_pkg
        .manifest_path
        .parent()
        .ok_or_else(|| anyhow!("manifest_path sin directorio padre"))?;

    let package = Package {
        name: ebpf_pkg.name.as_str(),
        root_dir: root_dir.as_str(),
        no_default_features: false,
        features: &[],
    };
    aya_build::build_ebpf([package], Toolchain::default())
}
