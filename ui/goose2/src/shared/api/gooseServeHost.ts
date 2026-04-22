import { invoke } from "@tauri-apps/api/core";

export interface GooseServeHostInfo {
  httpBaseUrl: string;
  secretKey: string;
}

let hostInfoPromise: Promise<GooseServeHostInfo> | null = null;

export async function getGooseServeHostInfo(): Promise<GooseServeHostInfo> {
  if (!hostInfoPromise) {
    hostInfoPromise = invoke<GooseServeHostInfo>("get_goose_serve_host_info");
  }

  return hostInfoPromise;
}

export async function postGooseServeJson<TResponse>(
  path: string,
  body: unknown,
): Promise<TResponse> {
  const { httpBaseUrl, secretKey } = await getGooseServeHostInfo();
  const response = await fetch(`${httpBaseUrl}${path}`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "X-Secret-Key": secretKey,
    },
    body: JSON.stringify(body),
  });

  if (!response.ok) {
    throw new Error(`Goose serve request failed (${response.status})`);
  }

  return (await response.json()) as TResponse;
}
