# Instalar CaudalGhost en Ubuntu/Kubuntu (.deb)

Dos paquetes: `caudalghost` (daemon + CLI, la mitad privilegiada) y
`caudalghost-gui` (la GUI, depende del primero).

## Instalar

```bash
# Construir (sin sudo):
cargo build --release
( cd crates/caudalghostd && cargo deb --no-build )
( cd crates/caudalghost-gui && cargo tauri build --bundles deb )

# Instalar ambos (esto SÍ pide tu contraseña una vez — es la instalación,
# no el uso diario):
sudo apt install ./target/debian/caudalghost_*_amd64.deb \
                 ./crates/caudalghost-gui/src-tauri/target/release/bundle/deb/*.deb
```

El paquete del daemon crea el usuario de sistema `caudalghost`, instala y
**habilita** el servicio (arranca solo en cada boot, sin sudo), y coloca la regla
polkit para que F4 resuelva hostnames. El paquete de la GUI te añade al grupo
`caudalghost`.

## Un paso manual: re-login

El acceso al socket es por grupo. Tras instalar, **cierra sesión y vuelve a
entrar** (o `newgrp caudalghost`) para que la CLI/GUI conecten sin sudo.

## Comprobar

```bash
systemctl status caudalghostd        # activo, como usuario caudalghost
caudalghost ping                     # responde, sin sudo
caudalghost hosts 5m                 # hostnames (no solo IPs) → polkit OK
```

## Desinstalar

```bash
sudo apt remove caudalghost-gui caudalghost
```
