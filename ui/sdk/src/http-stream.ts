import type { AnyMessage, Stream } from "@agentclientprotocol/sdk";

const ACP_CONNECTION_HEADER = "Acp-Connection-Id";
const ACP_SESSION_HEADER = "Acp-Session-Id";

/**
 * Creates an ACP Stream that communicates over the Streamable HTTP transport
 * defined in RFD 721.
 *
 * Protocol flow:
 *   1. `initialize` → POST (no Acp-Connection-Id), returns per-request SSE
 *      with `Acp-Connection-Id` in response headers.
 *   2. JSON-RPC requests → POST with `Acp-Connection-Id`, returns per-request
 *      SSE that delivers notifications + the final response, then closes.
 *   3. Notifications / client responses → POST with `Acp-Connection-Id`,
 *      returns 202 Accepted (no body).
 *   4. `session/new`, `session/load`, `session/fork` responses carry
 *      `Acp-Session-Id` in the response headers (informational).
 */
export function createHttpStream(serverUrl: string): Stream {
  let connectionId: string | null = null;
  const incoming: AnyMessage[] = [];
  const waiters: Array<() => void> = [];
  const abortController = new AbortController();

  function pushMessage(msg: AnyMessage) {
    incoming.push(msg);
    const w = waiters.shift();
    if (w) w();
  }

  function waitForMessage(): Promise<void> {
    if (incoming.length > 0) return Promise.resolve();
    return new Promise<void>((r) => waiters.push(r));
  }

  async function consumeSSE(
    response: Response,
    signal?: AbortSignal,
  ): Promise<void> {
    if (!response.body) return;
    const reader = response.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";

    try {
      while (true) {
        if (signal?.aborted) break;
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        const parts = buffer.split("\n\n");
        buffer = parts.pop() || "";

        for (const part of parts) {
          for (const line of part.split("\n")) {
            if (line.startsWith("data: ") || line.startsWith("data:")) {
              const dataStr = line.startsWith("data: ")
                ? line.slice(6)
                : line.slice(5);
              if (dataStr.trim()) {
                try {
                  pushMessage(JSON.parse(dataStr) as AnyMessage);
                } catch {
                  // ignore malformed JSON
                }
              }
            }
          }
        }
      }
    } catch (e: unknown) {
      if (e instanceof DOMException && e.name === "AbortError") return;
      throw e;
    }
  }

  function isJsonRpcRequest(msg: AnyMessage): boolean {
    return (
      "method" in msg &&
      "id" in msg &&
      msg.id !== undefined &&
      msg.id !== null
    );
  }

  function isInitializeRequest(msg: AnyMessage): boolean {
    return isJsonRpcRequest(msg) && "method" in msg && msg.method === "initialize";
  }

  const readable = new ReadableStream<AnyMessage>({
    async pull(controller) {
      await waitForMessage();
      while (incoming.length > 0) {
        controller.enqueue(incoming.shift()!);
      }
    },
  });

  const writable = new WritableStream<AnyMessage>({
    async write(msg) {
      const headers: Record<string, string> = {
        "Content-Type": "application/json",
        Accept: "application/json, text/event-stream",
      };
      if (connectionId) {
        headers[ACP_CONNECTION_HEADER] = connectionId;
      }

      if (isInitializeRequest(msg)) {
        // Initialize: no Acp-Connection-Id, returns SSE with the header.
        const response = await fetch(`${serverUrl}/acp`, {
          method: "POST",
          headers,
          body: JSON.stringify(msg),
          signal: abortController.signal,
        });

        const connId = response.headers.get(ACP_CONNECTION_HEADER);
        if (connId) connectionId = connId;

        await consumeSSE(response, abortController.signal);
      } else if (isJsonRpcRequest(msg)) {
        // JSON-RPC request: returns a per-request SSE stream.
        const response = await fetch(`${serverUrl}/acp`, {
          method: "POST",
          headers,
          body: JSON.stringify(msg),
          signal: abortController.signal,
        });

        // session/new, session/load, session/fork may return Acp-Session-Id
        const sessionId = response.headers.get(ACP_SESSION_HEADER);
        if (sessionId) {
          // Informational — the SDK tracks sessionId in the response body.
        }

        await consumeSSE(response, abortController.signal);
      } else {
        // Notification or client response: fire-and-forget, expect 202.
        await fetch(`${serverUrl}/acp`, {
          method: "POST",
          headers,
          body: JSON.stringify(msg),
          signal: abortController.signal,
        });
      }
    },

    close() {
      // Terminate the connection.
      if (connectionId) {
        const headers: Record<string, string> = {
          [ACP_CONNECTION_HEADER]: connectionId,
        };
        fetch(`${serverUrl}/acp`, {
          method: "DELETE",
          headers,
        }).catch(() => {});
      }
      abortController.abort();
    },
  });

  return { readable, writable };
}
