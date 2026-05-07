import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: {
      "@": "/src",
    },
  },
  define: {
    // @ts-expect-error process is a nodejs global
    __GOOSE_SERVER_URL__: JSON.stringify(process.env.VITE_GOOSE_SERVER_URL || ""),
  },
  clearScreen: false,
  server: {
    port: parseInt(process.env.VITE_PORT || "1520", 10),
    strictPort: true,
    host: host || "0.0.0.0",
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: parseInt(process.env.VITE_PORT || "1520", 10) + 1,
        }
      : undefined,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
