import {
  AppRenderer,
  type AppRendererProps,
  type McpUiHostContext,
} from "@mcp-ui/client";
import type { GooseToolCallResponse } from "@aaif/goose-sdk";
import type {
  CallToolResult,
  ReadResourceResult,
} from "@modelcontextprotocol/sdk/types.js";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getClient } from "@/shared/api/acpConnection";
import { getGooseServeHostInfo } from "@/shared/api/gooseServeHost";
import { cn } from "@/shared/lib/cn";
import { useTheme } from "@/shared/theme/ThemeProvider";
import type {
  McpAppPayload,
  ToolResponseContent,
} from "@/shared/types/messages";
import {
  extractRenderableMcpAppDocument,
  type McpAppResourceCsp,
} from "./mcpAppPayload";

interface McpAppViewProps {
  payload: McpAppPayload;
  toolInput?: Record<string, unknown>;
  toolResponse?: ToolResponseContent;
  onSendMessage?: (text: string) => void | Promise<void>;
  onAutoScrollRequest?: (element: HTMLElement | null) => void;
}

const DEFAULT_APP_HEIGHT = 240;
const INLINE_DISPLAY_MODES = ["inline"] as const;
type SizeChangedParams = Parameters<
  NonNullable<AppRendererProps["onSizeChanged"]>
>[0];
type MessageParams = Parameters<NonNullable<AppRendererProps["onMessage"]>>[0];

function appendDomains(
  params: URLSearchParams,
  key: string,
  domains: string[] | undefined,
) {
  if (domains && domains.length > 0) {
    params.set(key, domains.join(","));
  }
}

function buildProxyUrl(
  httpBaseUrl: string,
  secretKey: string,
  csp: McpAppResourceCsp | null,
): URL {
  const params = new URLSearchParams({ secret: secretKey });
  appendDomains(params, "connect_domains", csp?.connectDomains);
  appendDomains(params, "resource_domains", csp?.resourceDomains);
  appendDomains(params, "frame_domains", csp?.frameDomains);
  appendDomains(params, "base_uri_domains", csp?.baseUriDomains);
  appendDomains(params, "script_domains", csp?.scriptDomains);
  return new URL(`/mcp-app-proxy?${params.toString()}`, httpBaseUrl);
}

function buildToolResult(
  toolResponse: ToolResponseContent | undefined,
): CallToolResult | undefined {
  if (!toolResponse) {
    return undefined;
  }

  return {
    content: [{ type: "text", text: toolResponse.result }],
    isError: toolResponse.isError,
    structuredContent:
      toolResponse.structuredContent as CallToolResult["structuredContent"],
  };
}

export function McpAppView({
  payload,
  toolInput,
  toolResponse,
  onSendMessage,
  onAutoScrollRequest,
}: McpAppViewProps) {
  const { t } = useTranslation("chat");
  const { resolvedTheme } = useTheme();
  const [hostInfo, setHostInfo] = useState<{
    httpBaseUrl: string;
    secretKey: string;
  } | null>(null);
  const [inlineHeight, setInlineHeight] = useState(DEFAULT_APP_HEIGHT);
  const [renderError, setRenderError] = useState<string | null>(null);
  const [activeToolInput, setActiveToolInput] = useState<
    Record<string, unknown> | undefined
  >();
  const [activeToolResult, setActiveToolResult] = useState<
    CallToolResult | undefined
  >();
  const autoScrollTimersRef = useRef<number[]>([]);
  const rootRef = useRef<HTMLDivElement>(null);

  const renderableDocument = useMemo(
    () => extractRenderableMcpAppDocument(payload),
    [payload],
  );
  const initialToolResult = useMemo(
    () => buildToolResult(toolResponse),
    [toolResponse],
  );
  const currentToolInput = activeToolInput ?? toolInput;
  const currentToolResult = activeToolResult ?? initialToolResult;

  const requestAutoScroll = useCallback(() => {
    if (!onAutoScrollRequest) {
      return;
    }

    for (const timer of autoScrollTimersRef.current) {
      window.clearTimeout(timer);
    }
    autoScrollTimersRef.current = [];

    const runAutoScroll = () => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          onAutoScrollRequest(rootRef.current);
        });
      });
    };

    runAutoScroll();

    for (const delay of [120, 300, 650]) {
      const timer = window.setTimeout(() => {
        runAutoScroll();
      }, delay);
      autoScrollTimersRef.current.push(timer);
    }
  }, [onAutoScrollRequest]);

  useEffect(
    () => () => {
      for (const timer of autoScrollTimersRef.current) {
        window.clearTimeout(timer);
      }
      autoScrollTimersRef.current = [];
    },
    [],
  );

  useEffect(() => {
    let cancelled = false;

    getGooseServeHostInfo()
      .then((info) => {
        if (!cancelled) {
          setHostInfo(info);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setRenderError(t("message.mcpAppRenderError"));
        }
      });

    return () => {
      cancelled = true;
    };
  }, [t]);

  useEffect(() => {
    if (!import.meta.env.DEV) {
      return;
    }

    console.groupCollapsed(
      `[McpAppView] ${payload.tool.extensionName}/${payload.tool.name}`,
    );
    console.debug("payload", payload);
    console.debug("renderableDocument", renderableDocument);
    console.debug("currentToolInput", currentToolInput ?? null);
    console.debug("currentToolResult", currentToolResult ?? null);
    console.debug("hostInfo", hostInfo);
    console.groupEnd();
  }, [
    currentToolInput,
    currentToolResult,
    hostInfo,
    payload,
    renderableDocument,
  ]);

  const sandbox = useMemo(() => {
    if (!hostInfo || !renderableDocument) {
      return null;
    }

    return {
      url: buildProxyUrl(
        hostInfo.httpBaseUrl,
        hostInfo.secretKey,
        renderableDocument.csp,
      ),
    };
  }, [hostInfo, renderableDocument]);

  const hostContext = useMemo<McpUiHostContext>(
    () => ({
      theme: resolvedTheme,
      displayMode: "inline",
      availableDisplayModes: [...INLINE_DISPLAY_MODES],
      locale: navigator.language,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      platform: "desktop",
    }),
    [resolvedTheme],
  );

  const handleOpenLink = useCallback(async ({ url }: { url: string }) => {
    await openUrl(url);
    return { status: "success" as const };
  }, []);

  const handleMessage = useCallback(
    async ({ role, content }: MessageParams) => {
      if (role !== "user" || !onSendMessage) {
        return { isError: true };
      }

      const text = content
        .filter((block): block is { type: "text"; text: string } => {
          return (
            block.type === "text" &&
            typeof block.text === "string" &&
            block.text.trim().length > 0
          );
        })
        .map((block) => block.text.trim())
        .join("\n\n");

      if (!text) {
        return { isError: true };
      }

      await onSendMessage(text);
      return {};
    },
    [onSendMessage],
  );

  const handleCallTool = useCallback(
    async ({
      name,
      arguments: args,
    }: {
      name: string;
      arguments?: Record<string, unknown>;
    }) => {
      const acpSessionId = payload.gooseSessionId ?? payload.sessionId;

      setActiveToolInput(args ?? {});
      setActiveToolResult(undefined);

      const client = await getClient();
      const response = (await client.extMethod("_goose/tool/call", {
        sessionId: acpSessionId,
        name: `${payload.tool.extensionName}__${name}`,
        arguments: args ?? {},
      })) as GooseToolCallResponse;

      const toolResult: CallToolResult = {
        content: (response.content ?? []) as CallToolResult["content"],
        isError: response.isError,
        structuredContent:
          response.structuredContent as CallToolResult["structuredContent"],
        _meta: response._meta as CallToolResult["_meta"],
      };

      setActiveToolResult(toolResult);
      return toolResult;
    },
    [payload.gooseSessionId, payload.sessionId, payload.tool.extensionName],
  );

  const handleReadResource = useCallback(
    async ({ uri }: { uri: string }) => {
      const acpSessionId = payload.gooseSessionId ?? payload.sessionId;
      const client = await getClient();
      const response = await client.goose.GooseResourceRead({
        sessionId: acpSessionId,
        uri,
        extensionName: payload.tool.extensionName,
      });

      return (response.result ?? { contents: [] }) as ReadResourceResult;
    },
    [payload.gooseSessionId, payload.sessionId, payload.tool.extensionName],
  );

  const handleSizeChanged = useCallback(
    ({ height }: SizeChangedParams) => {
      if (typeof height === "number" && height > 0) {
        setInlineHeight(height);
        requestAutoScroll();
      }
    },
    [requestAutoScroll],
  );

  const handleRenderError = useCallback(() => {
    setRenderError(t("message.mcpAppRenderError"));
  }, [t]);

  const shouldRenderApp =
    renderableDocument !== null && sandbox !== null && renderError === null;
  const shouldShowFallback =
    renderError !== null || renderableDocument === null;

  useEffect(() => {
    if (!import.meta.env.DEV || !shouldShowFallback) {
      return;
    }

    console.debug("[McpAppView] fallback", {
      payload,
      renderableDocument,
      renderError,
      readError: payload.resource.readError,
    });
  }, [payload, renderableDocument, renderError, shouldShowFallback]);

  useEffect(() => {
    if (!shouldRenderApp) {
      return;
    }

    requestAutoScroll();
  }, [requestAutoScroll, shouldRenderApp]);

  // Step 3 starts by rendering the stored MCP App payload inline. If anything
  // goes wrong, we intentionally keep the JSON snapshot visible so the chat
  // still exposes the persisted payload we built in step 2.
  return (
    <div ref={rootRef} className="my-3" data-testid="mcp-app-view">
      {shouldRenderApp ? (
        <div
          className={cn(
            "overflow-hidden rounded-lg bg-background/40",
            renderableDocument.prefersBorder &&
              "border border-border-primary shadow-sm",
          )}
          style={{ height: inlineHeight }}
        >
          <AppRenderer
            toolName={payload.tool.name}
            toolResourceUri={renderableDocument.resourceUri}
            html={renderableDocument.html}
            sandbox={sandbox}
            toolInput={currentToolInput}
            toolResult={currentToolResult}
            hostContext={hostContext}
            onOpenLink={handleOpenLink}
            onMessage={handleMessage}
            onCallTool={handleCallTool}
            onReadResource={handleReadResource}
            onSizeChanged={handleSizeChanged}
            onError={handleRenderError}
          />
        </div>
      ) : renderableDocument && renderError === null ? (
        <div className="rounded-lg border border-dashed border-border px-4 py-3 text-muted-foreground text-sm">
          {t("message.mcpAppLoading")}
        </div>
      ) : null}

      {shouldShowFallback && (
        <div className="mt-3">
          <div className="mb-2 text-muted-foreground text-xs uppercase tracking-wide">
            {t("message.mcpApp")}
          </div>
          {(renderError || payload.resource.readError) && (
            <p className="mb-3 text-muted-foreground text-sm">
              {renderError ?? payload.resource.readError}
            </p>
          )}
        </div>
      )}
    </div>
  );
}
