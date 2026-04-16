import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { open, save } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  api,
  type CanonicalMapping,
  type ImportSummary,
  type LookupHit,
  type UrlListLoad,
} from "./api";

type ResultFilter = "all" | "matched" | "none";
type LoadedUrlFile = Omit<UrlListLoad, "urls"> & { loaded_count: number };
type BusyPhase = "import_sources" | "load_url_list" | "lookup" | "mapping";

const CHIP_PREVIEW_LIMIT = 300;

async function onTitlebarMouseDown(e: MouseEvent) {
  if (e.button !== 0) return;
  if (e.detail === 2) {
    try {
      const w = getCurrentWindow();
      const max = await w.isMaximized();
      if (max) await w.unmaximize();
      else await w.maximize();
    } catch {}
    return;
  }
  try {
    await getCurrentWindow().startDragging();
  } catch {}
}

export default function App() {
  const [imports, setImports] = createSignal<ImportSummary[]>([]);
  const [allMetrics, setAllMetrics] = createSignal<string[]>([]);
  const [urlChips, setUrlChips] = createSignal<string[]>([]);
  const [chipDraft, setChipDraft] = createSignal("");
  const [selectedMetrics, setSelectedMetrics] = createSignal<Set<string>>(new Set());
  const [hits, setHits] = createSignal<LookupHit[]>([]);
  const [canonicalMappings, setCanonicalMappings] = createSignal<CanonicalMapping[]>([]);
  const [mappingSourcePattern, setMappingSourcePattern] = createSignal("");
  const [mappingTargetPath, setMappingTargetPath] = createSignal("");
  const [mappingHostPattern, setMappingHostPattern] = createSignal("");
  const [mappingExportProfile, setMappingExportProfile] = createSignal("");
  const [mappingPriority, setMappingPriority] = createSignal("100");
  const [mappingNotes, setMappingNotes] = createSignal("");
  const [editingMappingId, setEditingMappingId] = createSignal<string | null>(null);
  const [editSourcePattern, setEditSourcePattern] = createSignal("");
  const [editTargetPath, setEditTargetPath] = createSignal("");
  const [editHostPattern, setEditHostPattern] = createSignal("");
  const [editExportProfile, setEditExportProfile] = createSignal("");
  const [editPriority, setEditPriority] = createSignal("100");
  const [editNotes, setEditNotes] = createSignal("");
  const [editActive, setEditActive] = createSignal(true);
  const [missingMetrics, setMissingMetrics] = createSignal<string[]>([]);
  const [searchedFiles, setSearchedFiles] = createSignal(0);
  const [resultFilter, setResultFilter] = createSignal<ResultFilter>("all");
  const [loadedUrlFiles, setLoadedUrlFiles] = createSignal<LoadedUrlFile[]>([]);
  const [error, setError] = createSignal<string | null>(null);
  const [info, setInfo] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);
  const [busyPhase, setBusyPhase] = createSignal<BusyPhase | null>(null);
  const [busyStartedAt, setBusyStartedAt] = createSignal<number | null>(null);
  const [busyTick, setBusyTick] = createSignal(0);
  const [expandedDebug, setExpandedDebug] = createSignal<Set<string>>(new Set());

  const totalRows = createMemo(() =>
    imports().reduce((sum, i) => sum + i.row_count, 0),
  );
  const matchedCount = createMemo(() => hits().filter((h) => h.matched).length);
  const visibleUrlChips = createMemo(() => urlChips().slice(0, CHIP_PREVIEW_LIMIT));
  const hiddenChipCount = createMemo(() => Math.max(0, urlChips().length - CHIP_PREVIEW_LIMIT));
  const busyLabel = createMemo(() => {
    switch (busyPhase()) {
      case "import_sources":
        return "Importing Adobe export";
      case "load_url_list":
        return "Loading lookup URL list";
      case "lookup":
        return "Scanning URLs against Adobe data";
      case "mapping":
        return "Applying canonical mapping updates";
      default:
        return "Working";
    }
  });
  const busyHint = createMemo(() => {
    switch (busyPhase()) {
      case "import_sources":
        return "Parsing rows, detecting export profile, and indexing keys.";
      case "load_url_list":
        return "Extracting URL values from the selected file.";
      case "lookup":
        return "Evaluating strict match priority rules and collecting candidates.";
      case "mapping":
        return "Reindexing loaded imports with the latest mapping rules.";
      default:
        return "Please wait.";
    }
  });
  const busySeconds = createMemo(() => {
    busyTick();
    const started = busyStartedAt();
    if (!started) return 0;
    return Math.floor((Date.now() - started) / 1000);
  });

  function beginBusy(phase: BusyPhase) {
    setBusyPhase(phase);
    setBusyStartedAt(Date.now());
    setBusyTick(0);
    setBusy(true);
  }

  function endBusy() {
    setBusy(false);
    setBusyPhase(null);
    setBusyStartedAt(null);
    setBusyTick(0);
  }

  createEffect(() => {
    if (!busy()) return;
    const timer = window.setInterval(() => {
      setBusyTick((tick) => tick + 1);
    }, 500);
    onCleanup(() => window.clearInterval(timer));
  });

  onMount(async () => {
    setImports(await api.listImports());
    setAllMetrics(await api.allMetrics());
    setCanonicalMappings(await api.listCanonicalMappings());
  });

  async function refreshCanonicalMappings() {
    setCanonicalMappings(await api.listCanonicalMappings());
  }

  async function refreshImports() {
    setImports(await api.listImports());
    const metrics = await api.allMetrics();
    setAllMetrics(metrics);
    // Drop selected metrics that no longer exist in any file
    const valid = new Set(metrics);
    const newSelected = new Set([...selectedMetrics()].filter((m) => valid.has(m)));
    setSelectedMetrics(newSelected);
  }

  async function pickSourceFiles() {
    setError(null);
    try {
      const picked = await open({
        multiple: true,
        filters: [
          { name: "Spreadsheet", extensions: ["csv", "tsv", "txt", "xlsx", "xls", "xlsm"] },
        ],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      beginBusy("import_sources");
      let firstNew: string[] = [];
      for (const p of paths) {
        const summary = await api.importFile(p);
        firstNew.push(...summary.metric_columns);
      }
      await refreshImports();
      // Auto-select metrics from the newly added file(s) if user has none yet
      if (selectedMetrics().size === 0) {
        const metrics = await api.allMetrics();
        setSelectedMetrics(new Set(metrics.slice(0, 6)));
      } else {
        // Add new metrics not previously seen, up to 6
        const cur = new Set(selectedMetrics());
        for (const m of firstNew) {
          if (cur.size >= 8) break;
          cur.add(m);
        }
        setSelectedMetrics(cur);
      }
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  async function pickLookupFiles() {
    setError(null);
    try {
      const picked = await open({
        multiple: true,
        filters: [
          { name: "Spreadsheet", extensions: ["csv", "tsv", "txt", "xlsx", "xls", "xlsm"] },
        ],
      });
      if (!picked) return;

      const paths = Array.isArray(picked) ? picked : [picked];
      beginBusy("load_url_list");

      const loadedFiles: LoadedUrlFile[] = [];
      for (const p of paths) {
        const loaded = await api.loadLookupFile(p);
        addChips(loaded.urls);
        loadedFiles.push({
          file_name: loaded.file_name,
          row_count: loaded.row_count,
          url_column: loaded.url_column,
          warnings: loaded.warnings,
          loaded_count: loaded.urls.length,
        });
      }

      setLoadedUrlFiles((current) => [...current, ...loadedFiles]);
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  async function deleteImport(batchId: string, ev: MouseEvent) {
    ev.stopPropagation();
    await api.removeImport(batchId);
    await refreshImports();
    setHits([]);
    setExpandedDebug(new Set());
    setMissingMetrics([]);
    setSearchedFiles(0);
    setResultFilter("all");
  }

  async function addCanonicalMapping() {
    const source = mappingSourcePattern().trim();
    const target = mappingTargetPath().trim();
    const host = mappingHostPattern().trim();
    const exportProfile = mappingExportProfile().trim();
    const notes = mappingNotes().trim();
    const priority = Number.parseInt(mappingPriority().trim(), 10);
    if (!source || !target) {
      setError("Add both source pattern and target canonical path.");
      return;
    }
    beginBusy("mapping");
    setError(null);
    setInfo(null);
    try {
      await api.addCanonicalMapping(
        source,
        target,
        "path_map",
        host || undefined,
        exportProfile || undefined,
        Number.isFinite(priority) ? priority : 100,
        notes || undefined,
      );
      setMappingSourcePattern("");
      setMappingTargetPath("");
      setMappingHostPattern("");
      setMappingExportProfile("");
      setMappingPriority("100");
      setMappingNotes("");
      await refreshCanonicalMappings();
      setInfo("Canonical mapping added.");
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  async function removeCanonicalMapping(mappingId: string) {
    beginBusy("mapping");
    setError(null);
    setInfo(null);
    try {
      await api.removeCanonicalMapping(mappingId);
      await refreshCanonicalMappings();
      setInfo("Canonical mapping removed.");
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  function beginEditMapping(mapping: CanonicalMapping) {
    setEditingMappingId(mapping.mapping_id);
    setEditSourcePattern(mapping.source_pattern);
    setEditTargetPath(mapping.target_canonical_path);
    setEditHostPattern(mapping.host_pattern ?? "");
    setEditExportProfile(mapping.export_profile ?? "");
    setEditPriority(String(mapping.priority ?? 100));
    setEditNotes(mapping.notes ?? "");
    setEditActive(Boolean(mapping.active));
  }

  function cancelEditMapping() {
    setEditingMappingId(null);
    setEditSourcePattern("");
    setEditTargetPath("");
    setEditHostPattern("");
    setEditExportProfile("");
    setEditPriority("100");
    setEditNotes("");
    setEditActive(true);
  }

  async function saveEditMapping(mappingId: string) {
    const source = editSourcePattern().trim();
    const target = editTargetPath().trim();
    if (!source || !target) {
      setError("Source pattern and target canonical path are required.");
      return;
    }
    const parsedPriority = Number.parseInt(editPriority().trim(), 10);

    beginBusy("mapping");
    setError(null);
    setInfo(null);
    try {
      await api.updateCanonicalMapping(mappingId, {
        source_pattern: source,
        target_canonical_path: target,
        host_pattern: editHostPattern().trim() || "",
        export_profile: editExportProfile().trim() || "",
        priority: Number.isFinite(parsedPriority) ? parsedPriority : 100,
        notes: editNotes().trim() || "",
        active: editActive(),
      });
      await refreshCanonicalMappings();
      cancelEditMapping();
      setInfo("Canonical mapping updated.");
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  async function reorderMapping(mappingId: string, direction: "up" | "down") {
    beginBusy("mapping");
    setError(null);
    setInfo(null);
    try {
      await api.reorderCanonicalMapping(mappingId, direction);
      await refreshCanonicalMappings();
      setInfo("Canonical mapping order updated.");
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  async function importCanonicalMappingsFromJson() {
    beginBusy("mapping");
    setError(null);
    setInfo(null);
    try {
      const picked = await open({
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!picked || Array.isArray(picked)) return;
      const added = await api.importCanonicalMappings(picked);
      await refreshCanonicalMappings();
      setInfo(`Imported ${added} canonical mapping${added === 1 ? "" : "s"}.`);
      setHits([]);
      setExpandedDebug(new Set());
      setMissingMetrics([]);
      setSearchedFiles(0);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  async function exportCanonicalMappingsToJson() {
    beginBusy("mapping");
    setError(null);
    setInfo(null);
    try {
      const destination = await save({
        defaultPath: "canonical-mappings.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!destination) return;
      await api.exportCanonicalMappings(destination);
      setInfo("Canonical mappings exported.");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  function looksLikeUrl(s: string): boolean {
    if (!s) return false;
    if (s.startsWith("/")) return true;
    if (/^https?:\/\//i.test(s)) return true;
    if (/^[a-z0-9-]+(\.[a-z0-9-]+)+(\/|$)/i.test(s)) return true; // bare host
    return false;
  }

  function splitUrls(text: string): string[] {
    // Split on any line ending or tab (Excel multi-column paste).
    // Do NOT split on `;` — it's a valid URL character (matrix params, jsessionid).
    return text
      .split(/[\r\n\t]+/)
      .map((s) => s.trim().replace(/^["']|["']$/g, ""))
      .filter((s) => looksLikeUrl(s));
  }

  function addChips(items: string[]) {
    if (items.length === 0) return;
    // Preserve the exact pasted order, including duplicates — the user wants
    // input order = on-screen order = exported order, 1:1.
    setUrlChips([...urlChips(), ...items]);
  }

  function commitDraft() {
    const items = splitUrls(chipDraft());
    if (items.length > 0) {
      addChips(items);
      setChipDraft("");
    }
  }

  function removeChip(i: number) {
    const next = urlChips().slice();
    next.splice(i, 1);
    setUrlChips(next);
  }

  function clearChips() {
    setUrlChips([]);
    setChipDraft("");
    setHits([]);
    setExpandedDebug(new Set());
    setMissingMetrics([]);
    setSearchedFiles(0);
    setResultFilter("all");
    setLoadedUrlFiles([]);
  }

  function onChipPaste(e: ClipboardEvent) {
    const text = e.clipboardData?.getData("text") ?? "";
    const items = splitUrls(text);
    // Only intercept if the paste is multi-line / multi-cell, or contains a
    // URL-shaped value. Otherwise let the input handle it normally (so the
    // user can correct a typo in the draft).
    if (/[\r\n\t]/.test(text) || items.length > 1) {
      e.preventDefault();
      addChips(items);
      setChipDraft("");
    }
  }

  function onChipKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      commitDraft();
    } else if (e.key === "Backspace" && chipDraft() === "" && urlChips().length > 0) {
      removeChip(urlChips().length - 1);
    }
  }

  function toggleMetric(m: string) {
    const s = new Set(selectedMetrics());
    if (s.has(m)) s.delete(m);
    else s.add(m);
    setSelectedMetrics(s);
  }

  async function runLookup() {
    setError(null);
    if (imports().length === 0) {
      setError("Upload at least one Adobe source first.");
      return;
    }
    // Commit any pending text in the input first
    if (chipDraft().trim()) commitDraft();
    const urls = urlChips();
    if (urls.length === 0) {
      setError("Add at least one URL.");
      return;
    }
    beginBusy("lookup");
    try {
      const resp = await api.lookupUrls(
        urls,
        [...selectedMetrics()],
        imports().map((imp) => imp.batch_id),
      );
      setHits(resp.hits);
      setExpandedDebug(new Set());
      setMissingMetrics(resp.missing_metrics);
      setSearchedFiles(resp.searched_files);
      setResultFilter("all");
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  function matchesFilter(hit: LookupHit, filter: ResultFilter) {
    switch (filter) {
      case "matched":
        return hit.matched;
      case "none":
        return !hit.matched;
      default:
        return true;
    }
  }

  const filteredHits = createMemo(() =>
    hits().filter((hit) => matchesFilter(hit, resultFilter())),
  );

  const resultFilterCounts = createMemo(() => {
    const all = hits();
    return {
      all: all.length,
      matched: all.filter((hit) => matchesFilter(hit, "matched")).length,
      none: all.filter((hit) => matchesFilter(hit, "none")).length,
    } satisfies Record<ResultFilter, number>;
  });

  function exportCsv() {
    const sel = [...selectedMetrics()];
    const header = [
      "query",
      "exact_adobe_match",
      "match_count",
      "match_type",
      "match_score",
      "source_file",
      "source_url",
      ...sel,
    ];
    const lines = [header.join(",")];
    for (const h of filteredHits()) {
      if (h.rows.length === 0) {
        lines.push(
          [csv(h.query), "false", "0", csv(h.match_type), "", "", "", ...sel.map(() => "")].join(","),
        );
      } else {
        for (const r of h.rows) {
          lines.push(
            [
              csv(h.query),
              String(h.matched),
              String(h.match_count),
              csv(r.match_type || h.match_type),
              csv(r.match_score != null ? r.match_score.toFixed(3) : ""),
              csv(r.source_file ?? ""),
              csv(r.source_url),
              ...sel.map((m) => csv(r.metrics[m] ?? "")),
            ].join(","),
          );
        }
      }
    }
    const blob = new Blob([lines.join("\n")], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download =
      resultFilter() === "all" ? "lookup-results.csv" : `lookup-results-${resultFilter()}.csv`;
    a.click();
    URL.revokeObjectURL(url);
  }

  function csv(s: string) {
    if (s.includes(",") || s.includes('"') || s.includes("\n")) {
      return '"' + s.replace(/"/g, '""') + '"';
    }
    return s;
  }

  function fmtNum(n: number) {
    return n.toLocaleString();
  }

  function describeImportShape(imp: ImportSummary) {
    const parts = [imp.export_profile.replace(/_/g, " ")];
    if (imp.truncation_cap != null) {
      parts.push(`cap ${imp.truncation_cap}`);
    }
    return parts.join(" · ");
  }

  function describeMatchType(matchType: string, score?: number) {
    switch (matchType) {
      case "RAW_EXACT":
        return "Raw exact URL match";
      case "NORMALIZED_EXACT":
        return "Normalized URL exact match";
      case "PAGE_IDENTITY_MATCH":
        return "Page-identity match";
      case "HOST_AND_PATH_MATCH":
        return "Host + path match";
      case "PATH_ONLY_MATCH":
        return "Path exact match";
      case "CANONICAL_PATH_MATCH":
        return "Canonical path match";
      case "AMBIGUOUS_MATCH":
        return "Ambiguous match";
      case "SUGGESTION_ONLY":
        return "Suggestion only";
      case "NO_MATCH":
        return "No match";
      default:
        return "no match";
    }
  }

  function badgeClass(matchType: string) {
    if (matchType === "NO_MATCH" || matchType === "AMBIGUOUS_MATCH") {
      return "neutral";
    }
    if (matchType === "SUGGESTION_ONLY") {
      return "warn";
    }
    return "ok";
  }

  function bestMatchScore(hit: LookupHit) {
    return hit.rows.reduce((max, row) => Math.max(max, row.match_score ?? 0), 0);
  }

  function hitDebugKey(hit: LookupHit, idx: number) {
    return `${idx}::${hit.query}::${hit.normalized_query}`;
  }

  function toggleDebug(hit: LookupHit, idx: number) {
    const key = hitDebugKey(hit, idx);
    const next = new Set(expandedDebug());
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setExpandedDebug(next);
  }

  function isDebugExpanded(hit: LookupHit, idx: number) {
    return expandedDebug().has(hitDebugKey(hit, idx));
  }

  const resultFilterOptions: Array<{ value: ResultFilter; label: string }> = [
    { value: "all", label: "All" },
    { value: "matched", label: "Confirmed matches" },
    { value: "none", label: "No Adobe evidence" },
  ];

  const showMultiFile = createMemo(() => imports().length > 1);
  const debugColSpan = createMemo(
    () => 3 + (showMultiFile() ? 1 : 0) + selectedMetrics().size,
  );

  return (
    <div class="app">
      <header class="titlebar" data-tauri-drag-region onMouseDown={onTitlebarMouseDown}>
        <h1>Adobe Analytics Parser</h1>
        <span class="meta">
          <Show
            when={imports().length > 0}
            fallback="No Adobe sources loaded"
          >
            {imports().length} Adobe source{imports().length === 1 ? "" : "s"} · {fmtNum(totalRows())} rows
          </Show>
        </span>
      </header>

      <main>
        <aside class="sidebar">
          <div>
            <div class="section-title">Adobe Sources</div>
            <button class="full" onClick={pickSourceFiles} disabled={busy()}>
              {busy() ? "Working…" : "Add Adobe source CSV / XLSX"}
            </button>
            <Show when={info()}>
              <div class="ok-msg">{info()}</div>
            </Show>
            <Show when={error()}>
              <div class="err">{error()}</div>
            </Show>
          </div>

          <div>
            <div class="section-title">Loaded Adobe sources ({imports().length})</div>
            <Show
              when={imports().length > 0}
              fallback={
                <div style={{ "font-size": "11.5px", color: "var(--muted)", padding: "4px 4px" }}>
                  No Adobe sources yet.
                </div>
              }
            >
              <For each={imports()}>
                {(imp) => (
                  <div class="import-row">
                    <div class="import-row-head">
                      <div class="name" title={imp.file_name}>
                        {imp.file_name}
                      </div>
                      <button
                        class="x-btn"
                        title="Remove"
                        onClick={(e) => deleteImport(imp.batch_id, e)}
                      >
                        ×
                      </button>
                    </div>
                    <div class="sub">
                      {fmtNum(imp.row_count)} rows · {imp.metric_columns.length} metrics
                    </div>
                    <div class="sub">
                      {describeImportShape(imp)} · URL column: {imp.url_column}
                    </div>
                    <Show when={imp.warnings.length > 0}>
                      <div class="warn">{imp.warnings.join(" · ")}</div>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </div>

          <div>
            <div class="section-title">Canonical Mappings ({canonicalMappings().length})</div>
            <input
              type="text"
              placeholder="Source pattern (e.g. /se/en/products/*)"
              value={mappingSourcePattern()}
              onInput={(e) => setMappingSourcePattern(e.currentTarget.value)}
              disabled={busy()}
            />
            <input
              type="text"
              placeholder="Target canonical path (e.g. /products)"
              value={mappingTargetPath()}
              onInput={(e) => setMappingTargetPath(e.currentTarget.value)}
              disabled={busy()}
              style={{ "margin-top": "8px" }}
            />
            <input
              type="text"
              placeholder="Host pattern (optional, e.g. *.hitachienergy.com)"
              value={mappingHostPattern()}
              onInput={(e) => setMappingHostPattern(e.currentTarget.value)}
              disabled={busy()}
              style={{ "margin-top": "8px" }}
            />
            <input
              type="text"
              placeholder="Export profile (optional)"
              value={mappingExportProfile()}
              onInput={(e) => setMappingExportProfile(e.currentTarget.value)}
              disabled={busy()}
              style={{ "margin-top": "8px" }}
            />
            <input
              type="text"
              placeholder="Priority (default 100)"
              value={mappingPriority()}
              onInput={(e) => setMappingPriority(e.currentTarget.value)}
              disabled={busy()}
              style={{ "margin-top": "8px" }}
            />
            <input
              type="text"
              placeholder="Notes (optional)"
              value={mappingNotes()}
              onInput={(e) => setMappingNotes(e.currentTarget.value)}
              disabled={busy()}
              style={{ "margin-top": "8px" }}
            />
            <div class="row" style={{ "margin-top": "8px" }}>
              <button class="ghost" onClick={addCanonicalMapping} disabled={busy()}>
                Add mapping
              </button>
              <button class="ghost" onClick={importCanonicalMappingsFromJson} disabled={busy()}>
                Import JSON
              </button>
              <button class="ghost" onClick={exportCanonicalMappingsToJson} disabled={busy()}>
                Export JSON
              </button>
            </div>
            <Show when={canonicalMappings().length > 0}>
              <div class="file-note-list">
                <For each={canonicalMappings()}>
                  {(mapping) => (
                    <div class="file-note">
                      <Show
                        when={editingMappingId() === mapping.mapping_id}
                        fallback={
                          <>
                            <div>{mapping.source_pattern} → {mapping.target_canonical_path}</div>
                            <Show when={mapping.host_pattern || mapping.export_profile}>
                              <div class="subtle-note">
                                {mapping.host_pattern ? `host=${mapping.host_pattern}` : ""}
                                {mapping.host_pattern && mapping.export_profile ? " · " : ""}
                                {mapping.export_profile ? `profile=${mapping.export_profile}` : ""}
                              </div>
                            </Show>
                            <Show when={mapping.notes}>
                              <div class="subtle-note">notes: {mapping.notes}</div>
                            </Show>
                            <div class="row mapping-actions" style={{ "margin-top": "6px" }}>
                              <span class="subtle-note">
                                {mapping.rule_type} · priority {mapping.priority}
                              </span>
                              <span class="spacer" />
                              <button
                                class="ghost compact"
                                title="Move up"
                                onClick={() => reorderMapping(mapping.mapping_id, "up")}
                                disabled={busy()}
                              >
                                ↑
                              </button>
                              <button
                                class="ghost compact"
                                title="Move down"
                                onClick={() => reorderMapping(mapping.mapping_id, "down")}
                                disabled={busy()}
                              >
                                ↓
                              </button>
                              <button
                                class="ghost compact"
                                title="Edit mapping"
                                onClick={() => beginEditMapping(mapping)}
                                disabled={busy()}
                              >
                                Edit
                              </button>
                              <button
                                class="x-btn"
                                title="Remove mapping"
                                onClick={() => removeCanonicalMapping(mapping.mapping_id)}
                                disabled={busy()}
                              >
                                ×
                              </button>
                            </div>
                          </>
                        }
                      >
                        <div class="mapping-edit-grid">
                          <input
                            type="text"
                            value={editSourcePattern()}
                            onInput={(e) => setEditSourcePattern(e.currentTarget.value)}
                            placeholder="Source pattern"
                            disabled={busy()}
                          />
                          <input
                            type="text"
                            value={editTargetPath()}
                            onInput={(e) => setEditTargetPath(e.currentTarget.value)}
                            placeholder="Target path"
                            disabled={busy()}
                          />
                          <input
                            type="text"
                            value={editHostPattern()}
                            onInput={(e) => setEditHostPattern(e.currentTarget.value)}
                            placeholder="Host pattern (optional)"
                            disabled={busy()}
                          />
                          <input
                            type="text"
                            value={editExportProfile()}
                            onInput={(e) => setEditExportProfile(e.currentTarget.value)}
                            placeholder="Export profile (optional)"
                            disabled={busy()}
                          />
                          <input
                            type="text"
                            value={editPriority()}
                            onInput={(e) => setEditPriority(e.currentTarget.value)}
                            placeholder="Priority"
                            disabled={busy()}
                          />
                          <input
                            type="text"
                            value={editNotes()}
                            onInput={(e) => setEditNotes(e.currentTarget.value)}
                            placeholder="Notes (optional)"
                            disabled={busy()}
                          />
                          <label class="mapping-active">
                            <input
                              type="checkbox"
                              checked={editActive()}
                              onChange={(e) => setEditActive(e.currentTarget.checked)}
                              disabled={busy()}
                            />
                            active
                          </label>
                        </div>
                        <div class="row mapping-actions" style={{ "margin-top": "6px" }}>
                          <button
                            class="ghost compact"
                            onClick={() => saveEditMapping(mapping.mapping_id)}
                            disabled={busy()}
                          >
                            Save
                          </button>
                          <button
                            class="ghost compact"
                            onClick={cancelEditMapping}
                            disabled={busy()}
                          >
                            Cancel
                          </button>
                        </div>
                      </Show>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>
        </aside>

        <section class="content">
          <Show
            when={imports().length > 0}
            fallback={
              <div class="empty">
                <div class="icon">📊</div>
                <h3>No Adobe sources loaded</h3>
                <p>
                  Add one or more Adobe Analytics exports on the left. Then paste URLs
                  or load a URL list file to cross-check them against the loaded exports.
                </p>
              </div>
            }
          >
            <div class="card">
              <h2>Metrics ({allMetrics().length} available)</h2>
              <Show
                when={allMetrics().length > 0}
                fallback={
                  <div style={{ color: "var(--muted)", "font-size": "12px" }}>
                    No metric columns detected in the loaded Adobe sources.
                  </div>
                }
              >
                <div class="metric-grid">
                  <For each={allMetrics()}>
                    {(m) => (
                      <label class={`metric ${selectedMetrics().has(m) ? "active" : ""}`}>
                        <input
                          type="checkbox"
                          checked={selectedMetrics().has(m)}
                          onChange={() => toggleMetric(m)}
                        />
                        {m}
                      </label>
                    )}
                  </For>
                </div>
              </Show>
            </div>

            <div class="card">
              <h2>
                URL List
                <Show when={urlChips().length > 0}>
                  <span class="count-pill">{urlChips().length}</span>
                </Show>
              </h2>
              <div class="chip-input" onClick={(e) => {
                const input = (e.currentTarget as HTMLElement).querySelector("input");
                input?.focus();
              }}>
                <For each={visibleUrlChips()}>
                  {(u, i) => (
                    <span class="chip" title={u}>
                      <span class="chip-text">{u}</span>
                      <button
                        class="chip-x"
                        onClick={(ev) => {
                          ev.stopPropagation();
                          removeChip(i());
                        }}
                        title="Remove"
                      >
                        ×
                      </button>
                    </span>
                  )}
                </For>
                <input
                  type="text"
                  class="chip-draft"
                  value={chipDraft()}
                  placeholder={
                    urlChips().length === 0
                      ? "Paste URLs (new lines or tabs) — Enter to add"
                      : "Add another…"
                  }
                  onInput={(e) => setChipDraft(e.currentTarget.value)}
                  onKeyDown={onChipKeyDown}
                  onPaste={onChipPaste}
                  onBlur={() => commitDraft()}
                />
              </div>
              <Show when={hiddenChipCount() > 0}>
                <div class="subtle-note">
                  Showing first {fmtNum(CHIP_PREVIEW_LIMIT)} URLs only. Lookup still uses all{" "}
                  {fmtNum(urlChips().length)} loaded URLs.
                </div>
              </Show>
              <Show when={loadedUrlFiles().length > 0}>
                <div class="file-note-list">
                  <For each={loadedUrlFiles()}>
                    {(loaded) => (
                      <div class="file-note">
                        <div>
                          {loaded.file_name} · {fmtNum(loaded.loaded_count)} URLs · column{" "}
                          {loaded.url_column}
                        </div>
                        <Show when={loaded.warnings.length > 0}>
                          <div class="file-note-warn">{loaded.warnings.join(" · ")}</div>
                        </Show>
                      </div>
                    )}
                  </For>
                </div>
              </Show>
              <div class="row" style={{ "margin-top": "12px" }}>
                <button class="ghost" onClick={pickLookupFiles} disabled={busy()}>
                  Load URL list file
                </button>
                <button onClick={runLookup} disabled={busy()}>
                  {busy() && busyPhase() === "lookup"
                    ? "Scanning…"
                    : `Look up against ${imports().length} Adobe source${imports().length === 1 ? "" : "s"}`}
                </button>
                <Show when={hits().length > 0}>
                  <button class="ghost" onClick={exportCsv}>
                    Export CSV
                  </button>
                </Show>
                <Show when={urlChips().length > 0}>
                  <button class="ghost" onClick={clearChips}>
                    Clear all
                  </button>
                </Show>
                <span class="spacer" />
                <Show when={hits().length > 0}>
                  <span class="badge ok">
                    {matchedCount()} / {hits().length} matched
                  </span>
                </Show>
              </div>
              <Show when={busy()}>
                <div class="scan-status" role="status" aria-live="polite">
                  <div class="scan-spinner" />
                  <div class="scan-copy">
                    <div class="scan-title">{busyLabel()}…</div>
                    <div class="scan-sub">{busyHint()}</div>
                  </div>
                  <div class="scan-time">{busySeconds()}s</div>
                </div>
              </Show>
              <Show when={missingMetrics().length > 0}>
                <div class="warn">
                  Not present in the loaded Adobe sources: {missingMetrics().join(", ")}
                </div>
              </Show>
            </div>

            <Show when={hits().length > 0}>
              <div class="card">
                <div class="results-head">
                  <h2>
                    Results · searched {searchedFiles()} Adobe source
                    {searchedFiles() === 1 ? "" : "s"}
                  </h2>
                  <div class="results-meta">
                    showing {filteredHits().length} of {hits().length} queries
                  </div>
                </div>
                <div class="results-filters">
                  <For each={resultFilterOptions}>
                    {(option) => (
                      <button
                        class={`ghost filter-btn ${resultFilter() === option.value ? "active" : ""}`}
                        onClick={() => setResultFilter(option.value)}
                      >
                        {option.label} · {resultFilterCounts()[option.value]}
                      </button>
                    )}
                  </For>
                </div>
                <div style={{ overflow: "auto", "max-height": "calc(100vh - 360px)" }}>
                  <table>
                    <thead>
                      <tr>
                        <th>Query</th>
                        <th>Match Status</th>
                        <Show when={showMultiFile()}>
                          <th>Source</th>
                        </Show>
                        <th>Adobe Export URL</th>
                        <For each={[...selectedMetrics()]}>{(m) => <th>{m}</th>}</For>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={filteredHits()}>
                        {(h, hitIndex) => (
                          <>
                            <Show
                              when={h.rows.length > 0}
                              fallback={
                                <tr class="miss">
                                  <td class="url-cell">
                                    {h.query}
                                    <div class="normalized-hint">
                                      looked up as: {h.normalized_query}
                                    </div>
                                    <Show when={h.warnings.length > 0}>
                                      <div class="normalized-hint">
                                        {h.warnings.join(" · ")}
                                      </div>
                                    </Show>
                                    <Show when={h.discarded_variants.length > 0}>
                                      <div class="normalized-hint">
                                        discarded: {h.discarded_variants.slice(0, 3).join(" | ")}
                                      </div>
                                    </Show>
                                  </td>
                                  <td>
                                    <span class="badge neutral">no Adobe evidence</span>
                                    <button
                                      class="debug-btn"
                                      onClick={() => toggleDebug(h, hitIndex())}
                                    >
                                      {isDebugExpanded(h, hitIndex()) ? "Hide debug" : "Show debug"}
                                    </button>
                                  </td>
                                  <Show when={showMultiFile()}>
                                    <td>—</td>
                                  </Show>
                                  <td>—</td>
                                  <For each={[...selectedMetrics()]}>{() => <td>—</td>}</For>
                                </tr>
                              }
                            >
                              <For each={h.rows}>
                                {(r) => (
                                  <tr class={h.ambiguous ? "amb" : ""}>
                                    <td class="url-cell">{h.query}</td>
                                    <td>
                                      <span class={`badge ${badgeClass(r.match_type || h.match_type)}`}>
                                        {describeMatchType(
                                          r.match_type || h.match_type,
                                          r.match_score ?? bestMatchScore(h),
                                        )}
                                        {` (${(r.match_score ?? bestMatchScore(h)).toFixed(2)})`}
                                      </span>
                                      <button
                                        class="debug-btn"
                                        onClick={() => toggleDebug(h, hitIndex())}
                                      >
                                        {isDebugExpanded(h, hitIndex()) ? "Hide debug" : "Show debug"}
                                      </button>
                                      <Show when={h.warnings.length > 0}>
                                        <div class="normalized-hint">
                                          {h.warnings.join(" · ")}
                                        </div>
                                      </Show>
                                      <Show when={h.discarded_variants.length > 0}>
                                        <div class="normalized-hint">
                                          discarded: {h.discarded_variants.slice(0, 3).join(" | ")}
                                        </div>
                                      </Show>
                                    </td>
                                    <Show when={showMultiFile()}>
                                      <td class="src-cell">{r.source_file ?? ""}</td>
                                    </Show>
                                    <td class="url-cell">{r.source_url}</td>
                                    <For each={[...selectedMetrics()]}>
                                      {(m) => <td>{r.metrics[m] ?? "—"}</td>}
                                    </For>
                                  </tr>
                                )}
                              </For>
                            </Show>
                            <Show when={isDebugExpanded(h, hitIndex())}>
                              <tr class="debug-row">
                                <td colSpan={debugColSpan()}>
                                  <div class="debug-grid">
                                    <div>profile: {h.export_profile}</div>
                                    <div>type: {h.match_type}</div>
                                    <div>confidence: {h.match_confidence.toFixed(2)}</div>
                                    <div>count: {h.match_count}</div>
                                  </div>
                                  <Show when={h.warnings.length > 0}>
                                    <div class="normalized-hint">warnings: {h.warnings.join(" | ")}</div>
                                  </Show>
                                  <Show when={h.discarded_variants.length > 0}>
                                    <div class="normalized-hint">
                                      discarded variants: {h.discarded_variants.join(" || ")}
                                    </div>
                                  </Show>
                                  <Show when={h.rows.length > 0}>
                                    <For each={h.rows.slice(0, 3)}>
                                      {(row) => (
                                        <div class="debug-extra">
                                          <span class="debug-url">{row.source_url}</span>
                                          <Show when={Object.keys(row.extras).length > 0}>
                                            <span class="debug-kv">
                                              {Object.entries(row.extras)
                                                .map(([key, value]) => `${key}=${value}`)
                                                .join(" | ")}
                                            </span>
                                          </Show>
                                        </div>
                                      )}
                                    </For>
                                  </Show>
                                </td>
                              </tr>
                            </Show>
                          </>
                        )}
                      </For>
                    </tbody>
                  </table>
                </div>
              </div>
            </Show>
          </Show>
        </section>
      </main>
    </div>
  );
}
