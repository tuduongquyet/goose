import { describe, it, expect } from "vitest";
import { rebuildAttachmentDrafts } from "../attachments";
import type { Message } from "@/shared/types/messages";

describe("rebuildAttachmentDrafts", () => {
  it("skips image metadata entries when content blocks already provide base64", () => {
    const msg: Message = {
      id: "m1",
      role: "user",
      created: Date.now(),
      content: [
        {
          type: "image",
          source: {
            type: "base64",
            mediaType: "image/png",
            data: "iVBORw0KGgo=",
          },
        },
      ],
      metadata: {
        userVisible: true,
        agentVisible: true,
        attachments: [
          {
            type: "file",
            name: "screenshot.png",
            path: "/tmp/screenshot.png",
            mimeType: "image/png",
          },
        ],
      },
    };

    const drafts = rebuildAttachmentDrafts(msg);

    // Should produce exactly 1 image draft from the content block,
    // NOT a second file draft from the metadata entry
    expect(drafts).toHaveLength(1);
    expect(drafts[0].kind).toBe("image");
    if (drafts[0].kind === "image") {
      expect(drafts[0].base64).toBe("iVBORw0KGgo=");
    }
  });

  it("preserves non-image file metadata alongside image content blocks", () => {
    const msg: Message = {
      id: "m2",
      role: "user",
      created: Date.now(),
      content: [
        {
          type: "image",
          source: {
            type: "base64",
            mediaType: "image/jpeg",
            data: "/9j/4AAQ=",
          },
        },
        { type: "text", text: "See attached" },
      ],
      metadata: {
        userVisible: true,
        agentVisible: true,
        attachments: [
          {
            type: "file",
            name: "photo.jpg",
            path: "/tmp/photo.jpg",
            mimeType: "image/jpeg",
          },
          {
            type: "file",
            name: "data.csv",
            path: "/tmp/data.csv",
            mimeType: "text/csv",
          },
          {
            type: "directory",
            name: "src",
            path: "/project/src",
          },
        ],
      },
    };

    const drafts = rebuildAttachmentDrafts(msg);

    // 1 image from content block + 1 csv file + 1 directory = 3
    // photo.jpg metadata entry skipped (image already from content block)
    expect(drafts).toHaveLength(3);
    expect(drafts.map((d) => d.kind)).toEqual(["image", "file", "directory"]);
    expect(drafts[1].name).toBe("data.csv");
    expect(drafts[2].name).toBe("src");
  });

  it("includes image metadata entries when no content blocks exist", () => {
    const msg: Message = {
      id: "m3",
      role: "user",
      created: Date.now(),
      content: [{ type: "text", text: "file attached" }],
      metadata: {
        userVisible: true,
        agentVisible: true,
        attachments: [
          {
            type: "file",
            name: "photo.jpg",
            path: "/tmp/photo.jpg",
            mimeType: "image/jpeg",
          },
        ],
      },
    };

    const drafts = rebuildAttachmentDrafts(msg);

    // No image content blocks → metadata image entry should be included as file
    expect(drafts).toHaveLength(1);
    expect(drafts[0].kind).toBe("file");
    expect(drafts[0].name).toBe("photo.jpg");
  });

  it("preserves pathless browser-uploaded file attachments", () => {
    const msg: Message = {
      id: "m4",
      role: "user",
      created: Date.now(),
      content: [{ type: "text", text: "see attached" }],
      metadata: {
        userVisible: true,
        agentVisible: true,
        attachments: [
          {
            type: "file",
            name: "report.pdf",
            mimeType: "application/pdf",
          },
        ],
      },
    };

    const drafts = rebuildAttachmentDrafts(msg);

    expect(drafts).toHaveLength(1);
    expect(drafts[0].kind).toBe("file");
    expect(drafts[0].name).toBe("report.pdf");
  });
});
