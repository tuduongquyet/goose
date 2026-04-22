import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(async () => ({
  plugins: [react()],
  resolve: {
    alias: {
      "@": "/src",
    },
  },
  define: {
    __GOOSE_SERVER_URL__: JSON.stringify(
      process.env.VITE_GOOSE_SERVER_URL || "",
    ),
  },
  clearScreen: false,
  server: {
    port: Number.parseInt(process.env.VITE_PORT || "1520", 10),
    strictPort: false,
    host: "0.0.0.0",
  },
  preview: {
    host: "0.0.0.0",
    port: 4173,
  },
}));
