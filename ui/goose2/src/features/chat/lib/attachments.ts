import type {
  ChatAttachmentDraft,
  Message,
  MessageAttachment,
} from "@/shared/types/messages";

function formatAttachmentReference(attachment: ChatAttachmentDraft): string {
  const location =
    attachment.kind === "image"
      ? `${attachment.name} (image attached)`
      : (attachment.path ?? attachment.name);
  return `- [${attachment.kind}] ${location}`;
}

export function buildAttachmentPromptPreamble(
  attachments: ChatAttachmentDraft[] | undefined,
): string {
  const referencedAttachments = attachments ?? [];

  if (referencedAttachments.length === 0) {
    return "";
  }

  return [
    "Attached items:",
    ...referencedAttachments.map(formatAttachmentReference),
    "",
  ].join("\n");
}

export function buildMessageAttachments(
  attachments: ChatAttachmentDraft[] | undefined,
): MessageAttachment[] | undefined {
  const messageAttachments: MessageAttachment[] = [];

  for (const attachment of attachments ?? []) {
    if (attachment.kind === "directory") {
      messageAttachments.push({
        type: "directory",
        name: attachment.name,
        path: attachment.path,
      });
      continue;
    }

    messageAttachments.push({
      type: "file",
      name: attachment.name,
      ...(attachment.path ? { path: attachment.path } : {}),
      ...(attachment.kind === "image" || attachment.mimeType
        ? { mimeType: attachment.mimeType }
        : {}),
    });
  }

  return messageAttachments.length > 0 ? messageAttachments : undefined;
}

export function buildAcpImages(
  attachments: ChatAttachmentDraft[] | undefined,
): { base64: string; mimeType: string }[] | undefined {
  const images = (attachments ?? []).flatMap((attachment) =>
    attachment.kind === "image"
      ? [{ base64: attachment.base64, mimeType: attachment.mimeType }]
      : [],
  );

  return images.length > 0 ? images : undefined;
}

/**
 * Reconstruct ChatAttachmentDraft[] from a stored user message so retry/edit
 * can forward attachments that were present on the original send.  Image
 * content blocks carry base64 data; file/directory attachments are rebuilt
 * from metadata.attachments.
 */
export function rebuildAttachmentDrafts(
  message: Message,
): ChatAttachmentDraft[] {
  const drafts: ChatAttachmentDraft[] = [];

  // Rebuild image drafts from ImageContent blocks embedded in the message
  for (const block of message.content) {
    if (block.type === "image" && block.source.type === "base64") {
      drafts.push({
        id: crypto.randomUUID(),
        kind: "image",
        name: "image",
        mimeType: block.source.mediaType,
        base64: block.source.data,
        previewUrl: `data:${block.source.mediaType};base64,${block.source.data}`,
      });
    }
  }

  // Rebuild file/directory drafts from metadata.attachments, skipping image
  // entries when we already reconstructed image drafts from content blocks.
  // An uploaded image is stored as both a base64 content block and a metadata
  // file entry — including both would duplicate the image in the re-send.
  //
  // TODO: This heuristic assumes all uploaded images produce both a content
  // block and a metadata entry. If a future code path stores an image only in
  // metadata (without a corresponding content block), this broad skip will
  // silently drop it. If that changes, switch to per-image matching by
  // name or content hash instead of the blanket `hasImageDrafts` flag.
  const hasImageDrafts = drafts.some((d) => d.kind === "image");
  for (const att of message.metadata?.attachments ?? []) {
    if (att.type === "directory" && att.path) {
      drafts.push({
        id: crypto.randomUUID(),
        kind: "directory",
        name: att.name,
        path: att.path,
      });
    } else if (att.type === "file") {
      // Skip image file entries when content blocks already provide the base64
      if (hasImageDrafts && att.mimeType?.startsWith("image/")) continue;
      drafts.push({
        id: crypto.randomUUID(),
        kind: "file",
        name: att.name,
        ...(att.path ? { path: att.path } : {}),
        mimeType: att.mimeType,
      });
    }
  }

  return drafts;
}
