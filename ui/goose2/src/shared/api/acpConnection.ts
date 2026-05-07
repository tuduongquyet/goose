import { invoke } from "@/lib/ipc";
import { GooseClient } from "@aaif/goose-sdk";
import {
  PROTOCOL_VERSION,
  type Client,
  type SessionNotification,
  type RequestPermissionRequest,
  type RequestPermissionResponse,
} from "@agentclientprotocol/sdk";
import { createWebSocketStream } from "./createWebSocketStream";
import { perfLog } from "@/shared/lib/perfLog";

let notificationHandler: AcpNotificationHandler | null = null;

export interface AcpNotificationHandler {
  handleSessionNotification(notification: SessionNotification): Promise<void>;
}

export function setNotificationHandler(handler: AcpNotificationHandler): void {
  notificationHandler = handler;
}

let clientPromise: Promise<GooseClient> | null = null;
let resolvedClient: GooseClient | null = null;

function createClientCallbacks(): () => Client {
  return () => ({
    requestPermission: async (
      args: RequestPermissionRequest,
    ): Promise<RequestPermissionResponse> => {
      const optionId = args.options?.[0]?.optionId ?? "approve";
      return {
        outcome: {
          outcome: "selected",
          optionId,
        },
      };
    },

    sessionUpdate: async (notification: SessionNotification): Promise<void> => {
      if (notificationHandler) {
        await notificationHandler.handleSessionNotification(notification);
      }
    },
  });
}

function monitorConnection(client: GooseClient): void {
  client.closed
    .then(() => {
      console.warn(
        "[acp] Connection closed. Will reconnect on next getClient().",
      );
      resolvedClient = null;
      clientPromise = null;
    })
    .catch(() => {
      console.warn(
        "[acp] Connection error. Will reconnect on next getClient().",
      );
      resolvedClient = null;
      clientPromise = null;
    });
}

async function initializeConnection(): Promise<GooseClient> {
  const tStart = performance.now();
  const baseUrl = window.__TAURI_INTERNALS__
    ? await invoke<string>("get_goose_serve_url")
    : (typeof __GOOSE_SERVER_URL__ !== "undefined" && __GOOSE_SERVER_URL__) ||
      `http://${window.location.hostname}:3284`;
  if (window.__TAURI_INTERNALS__) {
    perfLog(
      `[perf:conn] get_goose_serve_url in ${(performance.now() - tStart).toFixed(1)}ms`,
    );
  }
  const wsUrl = baseUrl.replace(/^http/, "ws") + "/acp";

  const tStream = performance.now();
  const stream = createWebSocketStream(wsUrl);

  const client = new GooseClient(createClientCallbacks(), stream);
  perfLog(
    `[perf:conn] ws stream + client created in ${(performance.now() - tStream).toFixed(1)}ms`,
  );

  const tInit = performance.now();
  const INITIALIZE_TIMEOUT_MS = 10_000;
  await Promise.race([
    client.initialize({
      protocolVersion: PROTOCOL_VERSION,
      clientCapabilities: {},
      clientInfo: {
        name: "goose2",
        version: "0.1.0",
      },
    }),
    new Promise<never>((_, reject) =>
      setTimeout(
        () =>
          reject(
            new Error(
              `ACP initialize timed out after ${INITIALIZE_TIMEOUT_MS / 1000}s — is the goose server running at ${baseUrl}?`,
            ),
          ),
        INITIALIZE_TIMEOUT_MS,
      ),
    ),
  ]);
  perfLog(
    `[perf:conn] client.initialize in ${(performance.now() - tInit).toFixed(1)}ms (total ${(performance.now() - tStart).toFixed(1)}ms)`,
  );

  monitorConnection(client);

  return client;
}

export async function getClient(): Promise<GooseClient> {
  if (resolvedClient) {
    return resolvedClient;
  }

  if (!clientPromise) {
    perfLog("[perf:conn] getClient() → initializing new ACP connection");
    clientPromise = initializeConnection()
      .then((client) => {
        resolvedClient = client;
        return client;
      })
      .catch((error) => {
        clientPromise = null;
        throw error;
      });
  } else {
    perfLog("[perf:conn] getClient() awaiting in-flight initializeConnection");
  }

  return clientPromise;
}

export function isClientReady(): boolean {
  return resolvedClient !== null;
}

export function getClientSync(): GooseClient | null {
  return resolvedClient;
}
