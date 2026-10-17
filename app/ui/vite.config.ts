import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// The UI is a static page Tauri embeds (frontendDist = app/ui/dist), and the window always loads
// that build. tauri.conf.json deliberately has no devUrl, so no exe ever hands a localhost page
// the IPC of all 27 commands. This dev server only serves the visual check in a browser without
// Tauri; with VITE_MOCK_IPC=1 the IPC there is the in-memory mock, never the real machine.
export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    fs: {
      // The mock reads the preset files of the repository (`presets/*.cmdtheme`).
      allow: [fileURLToPath(new URL("../..", import.meta.url))],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    sourcemap: false,
    // Never inline small assets as data: URIs; the CSP (img-src 'self'; font-src 'self') blocks them.
    assetsInlineLimit: 0,
  },
});
