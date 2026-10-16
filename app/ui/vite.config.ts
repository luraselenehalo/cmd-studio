import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// The UI is a static page Tauri embeds (frontendDist = app/ui/dist). In development the Tauri
// CLI starts this server (devUrl = http://localhost:1420) and the window loads it; the same
// server, with VITE_MOCK_IPC=1, is the visual check in a browser without Tauri (the IPC is then
// the in-memory mock, never the real machine).
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
  },
});
