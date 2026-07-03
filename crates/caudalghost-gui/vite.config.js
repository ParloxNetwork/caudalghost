import { defineConfig } from "vite";
import { sveltekit } from "@sveltejs/kit/vite";
import { readFileSync } from "node:fs";

// Fuente de verdad ÚNICA de la versión: el Cargo.toml del workspace raíz. Así el footer de la GUI
// SIEMPRE coincide con la versión del release (.deb) y no se desincroniza (package.json vive fuera del
// workspace y quedaba atrás en los releases que solo tocaban el daemon/CLI).
function workspaceVersion() {
  const toml = readFileSync(new URL("../../Cargo.toml", import.meta.url), "utf8");
  const m = toml.match(/\[workspace\.package\][\s\S]*?\bversion\s*=\s*"([^"]+)"/);
  if (!m) throw new Error("no pude leer [workspace.package] version en ../../Cargo.toml");
  return m[1];
}
const APP_VERSION = workspaceVersion();

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [sveltekit()],
  define: { __APP_VERSION__: JSON.stringify(APP_VERSION) },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
