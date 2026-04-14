# Step 03: Create the TypeScript ACP Connection Manager

## Objective

Create a singleton module that manages the lifecycle of the `GooseClient` connection to `goose serve` over WebSocket. This is the TypeScript equivalent of the Rust `GooseAcpManager::start()` singleton.

## Why

All ACP operations (send prompt, list sessions, export, etc.) need a shared, initialized `GooseClient` instance. This module:

1. Fetches the `goose serve` WebSocket URL from the Rust backend (Step 01's command)
2. Creates a WebSocket `Stream` for the ACP SDK
3. Creates a `GooseClient` with that stream
4. Calls `client.initialize()` to complete the ACP handshake
5. Provides the initialized client to all other modules

## New Files

### 1. `src/shared/api/createWebSocketStream.ts` — WebSocket transport for ACP

The `@agentclientprotocol/sdk` defines a `Stream` as `{ readable: ReadableStream<AnyMessage>, writable: WritableStream<AnyMessage> }`. The SDK ships `ndJsonStream` for stdio. The `@aaif/goose-acp` package ships `createHttpStream` for HTTP+SSE. Neither provides a WebSocket transport.

We need a `createWebSocketStream` that bridges a browser `WebSocket` to the ACP `Stream` interface. The `goose serve` WebSocket protocol sends each WS text frame as a single JSON-RPC message (no newline delimiters).

```typescript
/**
 * WebSocket transport for ACP connections.
 *
 * Creates a Stream (readable + writable pair of AnyMessage) backed by a
 * browser WebSocket connection. Each WS text frame is a single JSON-RPC
 * message — no newline delimiters needed.
 *
 * This matches the framing used by goose serve's /acp WebSocket endpoint
 * (see crates/goose-acp/src/transport/websocket.rs).
 */
import type { AnyMessage, Stream } from "@agentclientprotocol/sdk";

export function createWebSocketStream(wsUrl: string): Stream {
  const ws = new WebSocket(wsUrl);

  // Queue of messages received from the server, consumed by the readable stream.
  const incoming: AnyMessage[] = [];
  const waiters: Array<() => void> = [];
  let closed = false;

  function pushMessage(msg: AnyMessage): void {
    incoming.push(msg);
    const waiter = waiters.shift();
    if (waiter) waiter();
  }

  function waitForMessage(): Promise<void> {
    if (incoming.length > 0 || closed) return Promise.resolve();
    return new Promise<void>((resolve) => waiters.push(resolve));
  }

  // Wait for the WebSocket to open before allowing writes.
  const openPromise = new Promise<void>((resolve, reject) => {
    ws.addEventListener("open", () => resolve(), { once: true });
    ws.addEventListener("error", (event) => {
      reject(new Error(`WebSocket connection failed: ${event}`));
    }, { once: true });
  });

  ws.addEventListener("message", (event) => {
    if (typeof event.data !== "string") return;
    try {
      const msg = JSON.parse(event.data) as AnyMessage;
      pushMessage(msg);
    } catch {
      // Ignore malformed JSON
    }
  });

  ws.addEventListener("close", () => {
    closed = true;
    for (const waiter of waiters) waiter();
    waiters.length = 0;
  });

  ws.addEventListener("error", () => {
    closed = true;
    for (const waiter of waiters) waiter();
    waiters.length = 0;
  });

  const readable = new ReadableStream<AnyMessage>({
    async pull(controller) {
      await waitForMessage();
      while (incoming.length > 0) {
        controller.enqueue(incoming.shift()!);
      }
      if (closed && incoming.length === 0) {
        controller.close();
      }
    },
  });

  const writable = new WritableStream<AnyMessage>({
    async write(msg) {
      await openPromise;
      ws.send(JSON.stringify(msg));
    },
    close() {
      ws.close();
    },
    abort() {
      ws.close();
    },
  });

  return { readable, writable };
}
```

### 2. `src/shared/api/acpConnection.ts` — Singleton connection manager

The module uses a promise-based singleton pattern: `clientPromise` ensures only one initialization runs at a time, `resolvedClient` caches the result for synchronous access, and if initialization fails, `clientPromise` resets so the next call retries. This mirrors the Rust `OnceCell<Arc<GooseAcpManager>>` pattern in `manager.rs`.

The notification handler is registered separately (via `setNotificationHandler()` in Step 04) rather than passed at construction time. This avoids a circular dependency: `acpConnection.ts` creates the client, but `acpNotificationHandler.ts` both needs the client and must be registered with the connection.

```typescript
/**
 * Singleton ACP connection manager.
 *
 * Manages the lifecycle of the GooseClient connection to goose serve
 * over WebSocket. All ACP operations go through the client returned
 * by getClient().
 */
import { invoke } from "@tauri-apps/api/core";
import { GooseClient } from "@aaif/goose-acp";
import type {
  Client,
  SessionNotification,
  RequestPermissionRequest,
  RequestPermissionResponse,
} from "@agentclientprotocol/sdk";
import { createWebSocketStream } from "./createWebSocketStream";

// Will be set by Step 04 — the notification handler
let notificationHandler: AcpNotificationHandler | null = null;

/**
 * Interface for the notification handler that processes ACP session events.
 * Implemented in Step 04 (acpNotificationHandler.ts).
 */
export interface AcpNotificationHandler {
  handleSessionNotification(notification: SessionNotification): Promise<void>;
}

/**
 * Register the notification handler. Called once during app initialization
 * after the handler is created in Step 04.
 */
export function setNotificationHandler(handler: AcpNotificationHandler): void {
  notificationHandler = handler;
}

// Singleton state
let clientPromise: Promise<GooseClient> | null = null;
let resolvedClient: GooseClient | null = null;

/**
 * Build the Client implementation that the ACP SDK calls back into.
 *
 * This handles two callback types:
 * - requestPermission: auto-approve with the first option (same as Rust impl)
 * - sessionUpdate: delegate to the registered notification handler
 */
function createClientCallbacks(): () => Client {
  return () => ({
    requestPermission: async (
      args: RequestPermissionRequest,
    ): Promise<RequestPermissionResponse> => {
      const optionId = args.options?.[0]?.optionId ?? "approve";
      return {
        outcome: {
          type: "selected",
          optionId,
        },
      };
    },

    sessionUpdate: async (
      notification: SessionNotification,
    ): Promise<void> => {
      if (notificationHandler) {
        await notificationHandler.handleSessionNotification(notification);
      }
    },
  });
}

/**
 * Initialize the ACP connection.
 *
 * 1. Calls the Rust backend to get the goose serve WebSocket URL
 * 2. Creates a GooseClient with WebSocket transport
 * 3. Sends the ACP initialize handshake
 *
 * This is idempotent — calling it multiple times returns the same client.
 */
async function initializeConnection(): Promise<GooseClient> {
  // Returns something like "ws://127.0.0.1:54321/acp"
  const wsUrl: string = await invoke("get_goose_serve_url");

  const stream = createWebSocketStream(wsUrl);

  const client = new GooseClient(createClientCallbacks(), stream);

  await client.initialize({
    protocolVersion: "2025-03-26",
    capabilities: {},
    clientInfo: {
      name: "goose2",
      version: "0.1.0",
    },
  });

  return client;
}

/**
 * Get the initialized GooseClient singleton.
 *
 * The first call triggers initialization (fetching the URL, creating the
 * WebSocket connection, running the ACP handshake). Subsequent calls return
 * the same client immediately.
 *
 * Throws if initialization fails (e.g., goose serve is not running).
 */
export async function getClient(): Promise<GooseClient> {
  if (resolvedClient) {
    return resolvedClient;
  }

  if (!clientPromise) {
    clientPromise = initializeConnection()
      .then((client) => {
        resolvedClient = client;
        return client;
      })
      .catch((error) => {
        clientPromise = null;
        throw error;
      });
  }

  return clientPromise;
}

/**
 * Check if the client has been initialized.
 * Useful for guards that need to know if ACP is ready without triggering init.
 */
export function isClientReady(): boolean {
  return resolvedClient !== null;
}

/**
 * Get the client synchronously, or null if not yet initialized.
 * Use getClient() for the async version that triggers initialization.
 */
export function getClientSync(): GooseClient | null {
  return resolvedClient;
}
```

## Verification

1. `pnpm typecheck` passes.
2. `pnpm check` passes (Biome lint).
3. The modules can be imported without side effects — initialization only happens when `getClient()` is called.
4. Unit test for `createWebSocketStream`: mock `WebSocket`, verify messages flow bidirectionally.

## Files Created

| File | Purpose |
|------|---------|
| `src/shared/api/createWebSocketStream.ts` | WebSocket → ACP Stream adapter |
| `src/shared/api/acpConnection.ts` | Singleton ACP connection manager |

## Dependencies

- Step 01 (the `get_goose_serve_url` Tauri command must exist)
- Step 02 (`@aaif/goose-acp` and `@agentclientprotocol/sdk` must be installed)

## Notes

- The `goose serve` WebSocket endpoint at `/acp` sends one JSON-RPC message per WS text frame (no trailing newline). This is the same framing the Rust Tauri backend uses in `thread.rs`. `createWebSocketStream` performs the same bridging directly in the browser.
- WebSocket is used over HTTP+SSE because it is the same transport the Rust layer already uses with `goose serve`, provides true bidirectional communication on a single persistent connection, and avoids the quirks of `createHttpStream` (fire-and-forget POSTs, session header management).
- The `Client` interface from `@agentclientprotocol/sdk` uses `sessionUpdate` as the callback method name. The Rust `Client` trait calls it `session_notification` — same callback, different naming convention.
- The `protocolVersion` `"2025-03-26"` matches `ProtocolVersion::LATEST` from the Rust `agent-client-protocol` crate. Use `LATEST_PROTOCOL_VERSION` from `@agentclientprotocol/sdk` if exported; otherwise hardcode the string.
- If `invoke("get_goose_serve_url")` fails, the error propagates to the caller. The app startup code (Step 08) handles this by showing an error state rather than crashing.
- The initial implementation does not handle WebSocket reconnection. If the connection drops, `getClient()` returns the stale client. A future step can monitor `client.closed` / `client.signal` and reset the singleton to trigger reconnection.
