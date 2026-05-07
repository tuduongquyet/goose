# goose2 ↔ goose2-web Sync Document

**Branch:** `rel/v1.33.1`  
**Sync commit (goose2-web):** `ad6401044` — "Sync goose2 v1.33.1"  
**Scope:** 57 files changed, 3207 insertions, 734 deletions in goose2-web

---

## Part 1 — Applied directly to `goose2` (Tauri/web runtime switch)

The following files in `ui/goose2` were modified to support **both** Tauri desktop and pure web
environments via `window.__TAURI_INTERNALS__` runtime checks. The Tauri code path is preserved
exactly; a web-compatible fallback is added alongside it.

### Files modified in `ui/goose2`

| File | Change |
|---|---|
| `src/env.d.ts` | Added `__GOOSE_SERVER_URL__` global declaration |
| `src/shared/api/acpConnection.ts` | ACP WS URL: Tauri uses `invoke("get_goose_serve_url")`, web uses `__GOOSE_SERVER_URL__` env var |
| `src/shared/api/acpApi.ts` | `listProviders()` wrapped in try/catch; `entries ?? []` null-guard |
| `src/shared/hooks/useZoom.ts` | `applyZoom`: Tauri uses `setZoom()`, web uses `document.documentElement.style.zoom`; removed Tauri-only early return |
| `src/shared/lib/fileManager.ts` | `revealInFileManager`: no-op in web |
| `src/shared/ui/ai-elements/link-safety-modal.tsx` | `openUrl`: Tauri uses plugin, web uses `window.open` |
| `src/features/settings/ui/DoctorCheckRow.tsx` | `openUrl`: Tauri uses plugin, web uses `window.open` |
| `src/features/chat/ui/ContextPanel.tsx` | `openPath`: gated behind `__TAURI_INTERNALS__` |
| `src/features/chat/ui/FilesList.tsx` | `openPath`: gated behind `__TAURI_INTERNALS__` |
| `src/features/chat/ui/MessageBubble.tsx` | `openPath`: gated behind `__TAURI_INTERNALS__` |
| `src/features/chat/hooks/ArtifactPolicyContext.tsx` | `openPath`: gated behind `__TAURI_INTERNALS__` |
| `src/features/chat/ui/ChatInput.tsx` | File/folder pickers: Tauri uses `open()`, web uses `<input type="file">` |
| `src/features/agents/ui/AgentsView.tsx` | Import picker: Tauri uses `open()` + path read, web uses `<input>` + `arrayBuffer()` |
| `src/features/agents/ui/AvatarDropZone.tsx` | Avatar picker: Tauri uses `open()` + `processPath`, web uses `<input>` + `processFile` |
| `src/features/projects/ui/CreateProjectDialog.tsx` | Directory picker: Tauri uses `open({directory:true})`, web uses `window.prompt` |

### Pattern used

```ts
// Static imports are kept — packages ARE installed in goose2
import { openPath } from "@tauri-apps/plugin-opener";

// Call sites are wrapped:
if (window.__TAURI_INTERNALS__) {
  void openPath(path);
} else {
  // web fallback (or no-op where irrelevant)
}
```

### `acpConnection.ts` URL resolution

```ts
const wsUrl = window.__TAURI_INTERNALS__
  ? await invoke<string>("get_goose_serve_url")           // Tauri: ask backend
  : (window.__GOOSE_SERVER_URL__) ||                      // Web: build-time env var
    `http://${window.location.hostname}:3284`;             // Web: localhost fallback
```

`__GOOSE_SERVER_URL__` must be injected by the web build's `vite.config.ts` via `define`.

---

## Part 2 — `goose2-web` specific changes (goose2-web only)

This sync brings `goose2-web` up to parity with `goose2` v1.33.1. The main changes in `goose2` were:
- Complete Skills feature rewrite (new list/detail/dialog/section architecture)
- New shared UI primitives: `page-shell`, `page-columns`, `detail-field`
- ACP layer: `archiveSession`, `renameSession`, `personaId`, `annotations.audience` filtering
- `chatSessionStore` migrated from localStorage overlay → ACP server calls
- Persona avatar improvements

All Tauri-specific APIs were substituted with web-compatible equivalents. See the **Transformation Rules** section below.

---

## Transformation Rules (Tauri → Web)

| Tauri (goose2) | Web replacement (goose2-web) |
|---|---|
| `import { invoke } from "@tauri-apps/api/core"` | `import { invoke } from "@/lib/ipc"` |
| `import { listen } from "@tauri-apps/api/event"` | `import { listen } from "@/lib/ipc"` |
| `import { convertFileSrc } from "@tauri-apps/api/core"` | `import { convertFileSrc } from "@/lib/ipc"` (no-op pass-through) |
| `import { open } from "@tauri-apps/plugin-dialog"` | Inline `<input type="file">` via `new Promise` |
| `import { openUrl } from "@tauri-apps/plugin-opener"` | `window.open(url, "_blank")` |
| `import { openPath } from "@tauri-apps/plugin-opener"` | Dynamic import gated on `window.__TAURI_INTERNALS__` |
| `import { revealItemInDir } from "@tauri-apps/plugin-opener"` | No-op function |
| `getCurrentWebviewWindow().setZoom()` | `document.documentElement.style.zoom = String(level)` |
| Static `import("@tauri-apps/api/...")` | `import("@tauri-apps/api/..." as string)` with type cast |
| `window.__TAURI_INTERNALS__` blocks (window show) | Removed or gated with `as string` dynamic import |
| Tauri URL fetch via `invoke("get_goose_serve_url")` | `__GOOSE_SERVER_URL__` build-time env var with localhost fallback |

---

## Files Changed

### 🆕 New Files (added to goose2-web)

| File | Description |
|---|---|
| `src/features/skills/lib/skillCategories.ts` | Category constants and helpers for skill classification |
| `src/features/skills/lib/skillsHelpers.ts` | Utility functions for skill list processing |
| `src/features/skills/lib/projectHydration.ts` | Hydrates skills with project link information |
| `src/features/skills/ui/SkillDetailPage.tsx` | Full detail view for a single skill |
| `src/features/skills/ui/SkillsDialogs.tsx` | Confirmation/action dialogs for skills (delete, unlink, etc.) |
| `src/features/skills/ui/SkillsEmptyState.tsx` | Empty state component for skills list |
| `src/features/skills/ui/SkillsListSections.tsx` | Grouped list sections by category |
| `src/shared/ui/page-shell.tsx` | Page layout shell with header/content areas |
| `src/shared/ui/page-columns.tsx` | Two-column layout primitive (list + detail) |
| `src/shared/ui/detail-field.tsx` | Label+value field primitive for detail views |
| `src/features/skills/api/skills.test.ts` | Unit tests for skills API |
| `src/features/chat/hooks/__tests__/useChat.personaPreparation.test.ts` | Tests for persona system prompt preparation |
| `src/shared/ui/button.test.tsx` | Button component unit tests |
| `src/shared/ui/page-columns.test.tsx` | page-columns layout unit tests |

---

### 📝 Modified Files

#### `src/env.d.ts`
- Added `Window.__TAURI_INTERNALS__?: unknown` declaration (enables type-safe Tauri environment checks)
- Added `declare const __GOOSE_SERVER_URL__: string | undefined` (build-time env var for ACP server URL)

---

#### ACP / Communication Layer

##### `src/shared/api/acpConnection.ts`
- **Removed:** `invoke("get_goose_serve_url")` Tauri call for server URL
- **Added:** `__GOOSE_SERVER_URL__` env var with fallback `http://${window.location.hostname}:3284`
- WebSocket URL derived as `base.replace(/^http/, "ws") + "/acp"`
- Removed `perfLog` performance timing import

##### `src/shared/api/acpApi.ts`
- **Restored from goose2:** `archiveSession()`, `unarchiveSession()`, `renameSession()`
- **Restored:** `personaId` field passed via `_meta` in `newSession()`
- **Web-specific addition:** `listProviders()` wrapped in try/catch — returns `[DEFAULT_PROVIDER]` if server doesn't support `GooseProvidersList` extension
- **Web-specific addition:** `(result.entries ?? [])` defensive null-check on entries

##### `src/shared/api/acpNotificationHandler.ts`
- Synced to goose2: audience filtering — drops `assistant`-only content blocks from chat state
- Uses `makeTextBlock()` helper to attach `annotations` to text content

##### `src/shared/api/acpSessionTracker.ts`
- Minor sync to goose2 (no web-specific changes)

##### `src/shared/api/acp.ts`
- Synced to goose2: `annotations.audience` used for system prompt content blocks

---

#### Chat Feature

##### `src/features/chat/stores/chatSessionStore.ts`
- **Major change:** Replaced old `sessionMetadataOverlay` localStorage workaround with ACP server calls
- `archiveSession`, `unarchiveSession`, `renameSession` now call `acpApi` directly (same as goose2)
- The `sessionMetadataOverlay` approach is no longer needed — the ACP server supports these operations

##### `src/features/chat/ui/ChatInput.tsx`
- **Removed:** `import { open } from "@tauri-apps/plugin-dialog"`
- **Added:** `openFilePickerWeb()` — inline browser file picker using `<input type="file">`
- `handleAttachFiles` and `handleAttachFolders` rewritten to use browser File API

##### `src/features/chat/ui/MessageBubble.tsx`
- Synced to goose2
- **Web adaptation:** `openPath` replaced with dynamic import gated on `__TAURI_INTERNALS__`

##### `src/features/chat/ui/ContextPanel.tsx`
- Synced to goose2
- **Web adaptation:** `openPath` replaced with dynamic import gated on `__TAURI_INTERNALS__`

##### `src/features/chat/ui/FilesList.tsx`
- Synced to goose2
- **Web adaptation:** `openPath` replaced with dynamic import gated on `__TAURI_INTERNALS__`

##### `src/features/chat/hooks/useAttachmentDropTarget.ts`
- Synced to goose2 (drag-and-drop attachment handling)
- **Web adaptation:** Dynamic `import("@tauri-apps/api/webview")` replaced with `globalThis.__TAURI_INTERNALS__` conditional — uses `as string` to bypass TypeScript module resolution at build time

##### `src/features/chat/hooks/useChatInputAttachments.ts`
- Synced to goose2
- **Web adaptation:** `import { convertFileSrc } from "@tauri-apps/api/core"` → `import { convertFileSrc } from "@/lib/ipc"`

##### `src/features/chat/lib/newChat.ts`
- Synced to goose2 (updated chat initialization logic)

---

#### Skills Feature (Major Rewrite)

##### `src/features/skills/api/skills.ts`
- Complete rewrite: new `SkillInfo`, `SkillProjectLink`, `SkillSourceKind` types
- Added `fileLocation`, `directoryPath`, `sourceKind`, `sourceLabel`, `projectLinks`, `editable` fields
- New functions: `createSkill`, `updateSkill`, `deleteSkill`, `exportSkill`, `linkSkillToProject`, `unlinkSkillFromProject`
- No Tauri deps — verbatim copy

##### `src/features/skills/ui/SkillsView.tsx`
- Complete rewrite (698 diff lines) using new component architecture:
  - `PageShell` + `PageColumns` for layout
  - `SkillListSections` for categorized list
  - `SkillDetailPage` for detail panel
  - `SkillsDialogs` for action dialogs
  - `SkillsEmptyState` for empty state

##### `src/features/skills/ui/CreateSkillDialog.tsx`
- Extended: now handles both create and edit modes
- New `editingSkill` prop for editing existing skills

---

#### Agents Feature

##### `src/features/agents/ui/AgentsView.tsx`
- Synced to goose2
- **Web adaptation:** `import { open } from "@tauri-apps/plugin-dialog"` removed
- Import picker (`handleImportPicker`) rewritten to use browser `<input type="file">` + `ArrayBuffer` read

##### `src/features/agents/ui/AvatarDropZone.tsx`
- Synced to goose2
- **Web adaptation:** `open()` dialog replaced with browser `<input type="file">` calling `processFile()` directly
- `processPath()` (Tauri path-based) removed — not applicable in web

##### `src/features/agents/ui/PersonaDetails.tsx`
- Synced to goose2 (persona detail view updates)

---

#### Providers

##### `src/features/providers/api/inventory.ts`
- Synced to goose2 (no web-specific changes needed — user updated this file directly)

##### `src/features/providers/providerCatalog.ts`
- Minor sync to goose2

---

#### Projects

##### `src/features/projects/ui/CreateProjectDialog.tsx`
- Synced to goose2
- **Web adaptation:** Tauri `open({ directory: true })` dialog replaced with `window.prompt()` fallback for directory path input

---

#### Settings / Extensions

##### `src/features/settings/ui/DoctorCheckRow.tsx`
- Synced to goose2
- **Web adaptation:** `openUrl` → `window.open(url, "_blank")`

##### `src/features/extensions/api/extensions.ts`
- Synced to goose2 — uses updated SDK method names (`GooseConfigExtensionsAdd`, etc.)
- Required SDK rebuild (`ui/sdk/`) to expose new type definitions

##### `src/features/sessions/ui/SessionHistoryView.tsx`
- Synced to goose2 (updated session history UI)

---

#### Shared UI

##### `src/shared/ui/button.tsx`
- Updated variants and sizes

##### `src/shared/ui/accordion.tsx`
- Updated animation/style variants

##### `src/shared/ui/resizable.tsx`
- Updated resize handle styling

##### `src/shared/ui/input.tsx`
- Minor updates

##### `src/shared/ui/SearchBar.tsx`
- Updated styling/layout

##### `src/shared/ui/MainPanelLayout.tsx`
- Minor layout adjustments

##### `src/shared/ui/ai-elements/link-safety-modal.tsx`
- **Web adaptation:** `openUrl` from Tauri replaced with:
  - In Tauri: dynamic import of `@tauri-apps/plugin-opener` gated on `__TAURI_INTERNALS__`
  - In web: `window.open(url, "_blank")`

---

#### Shared Types

##### `src/shared/types/messages.ts`
- Added `ContentAnnotations` type with `audience` field (used for system prompt filtering)

##### `src/shared/types/chat.ts`
- Minor type updates

---

#### Shared Styles / i18n

##### `src/shared/styles/globals.css`
- Added new CSS custom properties and utility classes

##### `src/shared/i18n/locales/en/skills.json`
##### `src/shared/i18n/locales/es/skills.json`
- Many new i18n keys for the rewritten Skills feature

---

#### Zoom

##### `src/shared/hooks/useZoom.ts`
- Synced to goose2
- **Web adaptation:**
  - `applyZoom()`: in Tauri uses `getCurrentWebviewWindow().setZoom()`; **in web uses `document.documentElement.style.zoom`**
  - Early-return guard `if (!window.__TAURI_INTERNALS__) return` removed — zoom now works in browser too
  - Dynamic Tauri import uses `as string` to avoid build-time resolution failure

---

#### Tests

##### `src/features/skills/ui/__tests__/CreateSkillDialog.test.tsx`
- Synced to goose2 — test data updated to include `path` and `fileLocation` fields

##### `src/test/setup.ts`
- Added mock for `@tauri-apps/plugin-opener`

---

## Files NOT Changed (Web-Only, Never Overwrite)

These files are goose2-web-specific and must never be overwritten from goose2:

| File | Purpose |
|---|---|
| `src/lib/ipc.ts` | Drop-in replace for `@tauri-apps/api/core` + `@tauri-apps/api/event` |
| `src/lib/event-bus.ts` | Browser-side event bus replacing Tauri events |
| `src/lib/connect.ts` | Web connection logic |
| `src/lib/polyfills.ts` | Browser polyfills |
| `src/features/chat/hooks/useSSE.ts` | SSE-based chat streaming (web-only) |
| `src/features/chat/hooks/useImageDropTarget.ts` | Web image drag-drop |
| `src/features/chat/lib/sessionMetadataOverlay.ts` | Kept for reference; no longer used by chatSessionStore |
| `src/features/chat/ui/widgets/McpServersWidget.tsx` | Web-only MCP servers widget |

---

## Key Architecture Notes

### `__GOOSE_SERVER_URL__`
Build-time constant injected via `vite.config.ts` `define`. Falls back to `http://${window.location.hostname}:3284`.
Declared in `env.d.ts` as `declare const __GOOSE_SERVER_URL__: string | undefined`.

### `ipc.ts` shim
Central compatibility layer at `src/lib/ipc.ts`. Exports:
- `invoke()` — routes commands to localStorage stubs (projects, personas, skills CRUD), env vars, or Blob downloads
- `listen()` / `once()` — mirrors Tauri event API using `event-bus.ts`
- `convertFileSrc()` — no-op, returns path as-is

### Dynamic Tauri imports
When Tauri APIs cannot be removed but also cannot fail at build time, the pattern is:
```ts
import("@tauri-apps/api/package" as string)
  .then((m: { method: (...) => Promise<void> }) => m.method(...))
```
This bypasses TypeScript module resolution and only executes if `window.__TAURI_INTERNALS__` is truthy.

### SDK rebuild required
After this sync, the local `ui/sdk` workspace package must be rebuilt:
```bash
cd ui/sdk && pnpm build
```
This generates updated `GooseConfigExtensionsAdd/Remove/Toggle` type definitions needed by `extensions.ts`.

---

## Verification

After applying this sync, run:
```bash
cd ui/goose2-web
pnpm typecheck       # zero errors expected
grep -r "tauri-apps" src/ --include="*.ts" --include="*.tsx" | grep -v "as string\|test/setup\|event-bus\|lib/ipc"
# should return no results
```
