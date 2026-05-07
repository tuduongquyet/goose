declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }

  // Build-time constant for web deployments; injected via vite.config.ts `define`.
  // eslint-disable-next-line no-var
  var __GOOSE_SERVER_URL__: string | undefined;
}

export {};
