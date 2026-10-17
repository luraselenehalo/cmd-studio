import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

// UI tests run in jsdom against the in-memory IPC mock (`src/ipc/mock.ts`); nothing here ever
// reaches Tauri or this computer's settings.
export default defineConfig({
  root: fileURLToPath(new URL(".", import.meta.url)),
  plugins: [react()],
  test: {
    environment: "jsdom",
    globals: false,
    include: ["src/**/*.test.{ts,tsx}"],
    setupFiles: ["src/test/setup.ts"],
    server: {
      deps: {
        inline: ["@tauri-apps/api"],
      },
    },
  },
  server: {
    fs: {
      // The mock reads the preset files of the repository.
      allow: [fileURLToPath(new URL("../..", import.meta.url))],
    },
  },
});
