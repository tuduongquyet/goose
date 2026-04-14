import React, {useCallback, useEffect, useMemo, useState} from "react";
import {Box, Text, useInput, useStdout} from "ink";
import {TextInput} from "@inkjs/ui";
import type {GooseClient} from "@aaif/goose-acp";
import {GOLD, RULE_COLOR, TEAL, TEXT_DIM, TEXT_PRIMARY, TEXT_SECONDARY} from "./colors.js";
import {Spinner, SPINNER_FRAMES} from "./components/Spinner.js";
import {ErrorScreen} from "./components/ErrorScreen.js";

// Local TS mirrors of ExtensionConfig variants (flattened with a `type` tag)
type BuiltinCfg = { type: "builtin"; name: string; description: string; display_name?: string | null; timeout?: number | null };
type StdioCfg = { type: "stdio"; name: string; description: string; cmd: string; args: string[]; timeout?: number | null };
type StreamHttpCfg = { type: "streamable_http"; name: string; description: string; uri: string; timeout?: number | null };
type PlatformCfg = { type: "platform"; name: string; description: string; display_name?: string | null };
type SseCfg = { type: "sse"; name: string; description: string; uri?: string | null };

export type AnyExtCfg = BuiltinCfg | StdioCfg | StreamHttpCfg | PlatformCfg | SseCfg;

export type ExtEntry = { enabled: boolean } & AnyExtCfg;

function keyFromName(name: string): string {
  // mirrors crates/goose/src/config/extensions.rs name_to_key
  let out = "";
  for (const ch of name) {
    if (/^[A-Za-z0-9_-]$/.test(ch)) out += ch;
    else if (/^\s$/.test(ch)) { /* skip whitespace */ }
    else out += "_";
  }
  return out.toLowerCase();
}

function isExtEntry(v: any): v is ExtEntry {
  return v && typeof v === "object" && typeof v.enabled === "boolean" && typeof v.type === "string" && typeof v.name === "string";
}

type Phase = "loading" | "list" | "edit" | "saving" | "error";

type EditorState = {
  mode: "add" | "update";
  draft: ExtEntry;
  focusField: number; // index into dynamic field order
};

const FIELD_LABELS: Record<AnyExtCfg["type"], string[]> = {
  builtin: ["name", "description", "display_name", "timeout"],
  stdio: ["name", "description", "cmd", "args", "timeout"],
  streamable_http: ["name", "description", "uri", "timeout"],
  platform: ["name", "description", "display_name"],
  sse: ["name", "description", "uri"],
};

function fieldCountForType(t: AnyExtCfg["type"]): number { return FIELD_LABELS[t].length; }

function normalizeDraft(d: ExtEntry): ExtEntry {
  // Coerce optional numeric fields properly
  const coerce = (n: any) => (n === undefined || n === null || n === "" ? null : Number(n));
  if (d.type === "builtin" || d.type === "stdio" || d.type === "streamable_http") {
    const anyDraft: any = { ...d };
    if ("timeout" in anyDraft) anyDraft.timeout = coerce(anyDraft.timeout);
    return anyDraft as ExtEntry;
  }
  return d;
}

function getFieldValue(d: ExtEntry, key: string): string {
  const anyD: any = d;
  const v = anyD[key];
  if (Array.isArray(v)) return v.join(", ");
  return v == null ? "" : String(v);
}

function setFieldValue(d: ExtEntry, key: string, text: string): ExtEntry {
  const anyD: any = { ...d };
  if (key === "args") {
    anyD.args = text.split(",").map(s => s.trim()).filter(Boolean);
  } else if (key === "timeout") {
    anyD.timeout = text.trim() === "" ? null : Number(text.trim());
  } else if (key === "enabled") {
    anyD.enabled = text === "true";
  } else if (key === "display_name") {
    anyD.display_name = text.trim() === "" ? null : text;
  } else if (key === "uri" || key === "cmd" || key === "description" || key === "name") {
    anyD[key] = text;
  }
  return anyD as ExtEntry;
}

export default function ExtensionsManager({
  client,
  sessionId,
  width,
  height,
  onClose,
}: {
  client: GooseClient;
  sessionId: string;
  width: number;
  height: number;
  onClose: () => void;
}) {
  const { stdout } = useStdout();
  const columns = stdout?.columns ?? 80;

  const [phase, setPhase] = useState<Phase>("loading");
  const [spinIdx, setSpinIdx] = useState(0);
  const [errorMsg, setErrorMsg] = useState<string>("");
  const [entries, setEntries] = useState<ExtEntry[]>([]);
  const [mapping, setMapping] = useState<Record<string, ExtEntry>>({});
  const [warnings, setWarnings] = useState<string[]>([]);
  const [selectedIdx, setSelectedIdx] = useState(0);
  const [editor, setEditor] = useState<EditorState | null>(null);
  const [editType, setEditType] = useState<AnyExtCfg["type"]>("builtin");

  useEffect(() => {
    const t = setInterval(() => setSpinIdx(i => (i + 1) % SPINNER_FRAMES.length), 300);
    return () => clearInterval(t);
  }, []);

  const reload = useCallback(async () => {
    setPhase("loading");
    try {
      const resp = await client.goose.GooseConfigExtensions({});
      const list: ExtEntry[] = (resp.extensions as any[]).filter(isExtEntry) as any;
      setEntries(list);
      setWarnings(resp.warnings ?? []);
      // Try to read raw mapping for save operations
      try {
        const raw = await client.goose.GooseConfigRead({ key: "extensions" });
        const obj = (raw.value && typeof raw.value === "object") ? (raw.value as Record<string, any>) : {};
        const parsed: Record<string, ExtEntry> = {};
        for (const [k, v] of Object.entries(obj)) {
          if (isExtEntry(v)) parsed[k] = v as ExtEntry;
        }
        setMapping(parsed);
      } catch {
        setMapping({});
      }
      setPhase("list");
    } catch (e: any) {
      setErrorMsg(e?.message ?? String(e));
      setPhase("error");
    }
  }, [client]);

  useEffect(() => { reload(); }, [reload]);

  const maxWidth = Math.min(columns - 4, 82);
  const listHeight = Math.max(height - 9, 4); // header+footer budget
  const visibleEntries = entries;

  const startEdit = useCallback((mode: "add" | "update", base?: ExtEntry) => {
    let draft: ExtEntry;
    if (mode === "update" && base) {
      draft = JSON.parse(JSON.stringify(base));
      setEditType(base.type);
    } else {
      const empty: Record<string, any> = { enabled: true, type: editType, name: "", description: "" };
      if (editType === "stdio") { empty.cmd = ""; empty.args = []; }
      if (editType === "streamable_http") { empty.uri = ""; }
      if (editType === "sse") { empty.uri = ""; }
      draft = empty as ExtEntry;
    }
    setEditor({ mode, draft, focusField: 0 });
    setPhase("edit");
  }, [editType]);

  const saveMapping = useCallback(async (newMap: Record<string, ExtEntry>) => {
    await client.goose.GooseConfigUpsert({ key: "extensions", value: newMap as any });
  }, [client]);

  const removeSelected = useCallback(async () => {
    const sel = visibleEntries[selectedIdx];
    if (!sel) return;
    const key = keyFromName(sel.name);
    const newMap = { ...mapping };
    delete newMap[key];
    setPhase("saving");
    try {
      await saveMapping(newMap);
      setMapping(newMap);
      // Also attempt to remove from live session (best-effort)
      try { await client.goose.GooseExtensionsRemove({ sessionId, name: sel.name }); } catch {}
      await reload();
    } catch (e: any) {
      setErrorMsg(e?.message ?? String(e));
      setPhase("error");
    }
  }, [visibleEntries, selectedIdx, mapping, saveMapping, client, sessionId, reload]);

  const toggleEnabled = useCallback(async () => {
    const sel = visibleEntries[selectedIdx];
    if (!sel) return;
    const key = keyFromName(sel.name);
    const updated = { ...(mapping[key] ?? sel), enabled: !(mapping[key]?.enabled ?? sel.enabled) } as ExtEntry;
    const newMap = { ...mapping, [key]: updated };
    setPhase("saving");
    try {
      await saveMapping(newMap);
      setMapping(newMap);
      await reload();
    } catch (e: any) {
      setErrorMsg(e?.message ?? String(e));
      setPhase("error");
    }
  }, [visibleEntries, selectedIdx, mapping, saveMapping, reload]);

  const attachToSession = useCallback(async () => {
    const sel = visibleEntries[selectedIdx];
    if (!sel) return;
    setPhase("saving");
    try {
      await client.goose.GooseExtensionsAdd({ sessionId, config: sel as any });
      await reload();
    } catch (e: any) {
      setErrorMsg(e?.message ?? String(e));
      setPhase("error");
    }
  }, [client, sessionId, visibleEntries, selectedIdx, reload]);

  useInput((ch, key) => {
    if (phase === "list") {
      if (key.escape) { onClose(); return; }
      if (key.upArrow) { setSelectedIdx(i => Math.max(i - 1, 0)); return; }
      if (key.downArrow) { setSelectedIdx(i => Math.min(i + 1, visibleEntries.length - 1)); return; }
      if (ch === "a") { startEdit("add"); return; }
      if (ch === "e" || key.return) { const sel = visibleEntries[selectedIdx]; if (sel) startEdit("update", sel); return; }
      if (key.delete || key.backspace) { removeSelected(); return; }
      if (ch === " ") { toggleEnabled(); return; }
      if (ch === "s") { attachToSession(); return; }
      return;
    }
    if (phase === "edit" && editor) {
      if (key.escape) { setEditor(null); setPhase("list"); return; }
      if (key.tab || key.rightArrow) {
        setEditor(ed => ed ? { ...ed, focusField: (ed.focusField + 1) % fieldCountForType(ed.draft.type) } : ed);
        return;
      }
      if (key.leftArrow) {
        setEditor(ed => ed ? { ...ed, focusField: (ed.focusField - 1 + fieldCountForType(ed.draft.type)) % fieldCountForType(ed.draft.type) } : ed);
        return;
      }
    }
  });

  const renderList = () => {
    return (
      <Box flexDirection="column" width={columns} height={height}>
        <Box justifyContent="center" marginTop={1}><Text color={TEXT_PRIMARY} bold>◆ Manage extensions ◆</Text></Box>
        <Box justifyContent="center"><Text color={TEXT_DIM}>a add · e/enter edit · space toggle enabled · del remove · s attach to session · esc back</Text></Box>
        <Box marginTop={1} flexDirection="column" paddingX={2} height={listHeight}>
          {visibleEntries.length === 0 ? (
            <Box justifyContent="center" alignItems="center" height={Math.max(listHeight - 2, 1)}>
              <Text color={TEXT_DIM}>No extensions configured</Text>
            </Box>
          ) : (
            visibleEntries.map((ext, idx) => {
              const active = idx === selectedIdx;
              const nameW = Math.min(28, maxWidth - 20);
              const descW = Math.max(10, maxWidth - nameW - 10);
              return (
                <Box key={`${ext.type}:${ext.name}`}>
                  <Text color={active ? GOLD : TEXT_DIM}>{active ? "▸ " : "  "}</Text>
                  <Box width={nameW}><Text color={active ? TEXT_PRIMARY : TEXT_DIM} bold={active} wrap="truncate">{ext.name}</Text></Box>
                  <Text> </Text>
                  <Box width={descW}><Text color={TEXT_DIM} wrap="truncate">{ext.description || ""}</Text></Box>
                  <Text> </Text>
                  <Text color={ext.enabled ? TEAL : TEXT_DIM}>{ext.enabled ? "enabled" : "disabled"}</Text>
                </Box>
              );
            })
          )}
        </Box>
        {warnings.length > 0 && (
          <Box marginTop={1} paddingX={2} flexDirection="column">
            <Text color={GOLD}>Warnings</Text>
            {warnings.map((w, i) => (
              <Box key={i} width={maxWidth}><Text color={TEXT_DIM} wrap="wrap">• {w}</Text></Box>
            ))}
          </Box>
        )}
      </Box>
    );
  };

  const renderEditor = () => {
    if (!editor) return null;
    const { draft, mode, focusField } = editor;
    const fields = FIELD_LABELS[draft.type];

    const onChange = (text: string) => setEditor(ed => ed ? { ...ed, draft: setFieldValue(ed.draft, fields[focusField]!, text) } : ed);
    const onSubmit = async (text: string) => {
      const updatedDraft = setFieldValue(editor.draft, fields[focusField]!, text);
      const norm = normalizeDraft(updatedDraft);
      const k = keyFromName(norm.name);
      const newMap = { ...mapping, [k]: norm };
      setPhase("saving");
      try {
        await saveMapping(newMap);
        setMapping(newMap);
        // Best-effort attach to current session when enabled
        if (norm.enabled) { try { await client.goose.GooseExtensionsAdd({ sessionId, config: norm as any }); } catch {} }
        setEditor(null);
        await reload();
      } catch (e: any) {
        setErrorMsg(e?.message ?? String(e));
        setPhase("error");
      }
    };

    const help = "tab/→ next · ← prev · enter confirm · esc cancel";

    return (
      <Box flexDirection="column" width={columns} height={height} alignItems="center">
        <Box marginTop={1}><Text color={TEXT_PRIMARY} bold>{mode === "add" ? "Add extension" : `Edit ${draft.name}`}</Text></Box>
        <Box marginTop={1}>
          <Box borderStyle="round" borderColor={RULE_COLOR} paddingX={2} width={Math.min(60, maxWidth)}>
            <Text color={GOLD} bold>{"❯ "}</Text>
            <TextInput
              key={`field-${fields[focusField]}-${focusField}`}
              placeholder={fields[focusField]}
              defaultValue={getFieldValue(draft, fields[focusField]!)}
              onChange={onChange}
              onSubmit={onSubmit}
            />
          </Box>
        </Box>
        <Box marginTop={1}><Text color={TEXT_DIM}>{help}</Text></Box>
        {/* Field summary */}
        <Box marginTop={1} flexDirection="column" width={Math.min(70, maxWidth)}>
          {fields.map((f, i) => (
            <Box key={f}>
              <Text color={i === focusField ? GOLD : TEXT_DIM}>{i === focusField ? "▸ " : "  "}</Text>
              <Box width={18}><Text color={i === focusField ? TEXT_PRIMARY : TEXT_DIM}>{f}</Text></Box>
              <Box width={Math.min(50, maxWidth - 22)}>
                <Text color={TEXT_DIM} wrap="truncate">{getFieldValue(draft, f) || ""}</Text>
              </Box>
            </Box>
          ))}
        </Box>
      </Box>
    );
  };

  if (phase === "loading" || phase === "saving") {
    return (
      <Box flexDirection="column" justifyContent="center" alignItems="center" width={columns} height={height}>
        <Spinner idx={spinIdx} />
        <Box marginTop={1}><Text color={TEXT_DIM}>{phase === "loading" ? "loading extensions…" : "saving…"}</Text></Box>
      </Box>
    );
  }

  if (phase === "error") {
    return (
      <Box flexDirection="column" height={height} alignItems="center" width={columns}>
        <ErrorScreen errorMsg={errorMsg} onRetry={() => reload()} />
      </Box>
    );
  }

  if (phase === "edit") return renderEditor();
  return renderList();
}
