import { createEffect, createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  api,
  type ImportSummary,
  type LookupHit,
  type QueryMode,
  type UrlListLoad,
} from "./api";

type ResultFilter = "all" | "matched" | "none";
type LoadedUrlFile = Omit<UrlListLoad, "urls"> & { loaded_count: number };
type BusyPhase = "import_sources" | "load_url_list" | "lookup";
type TableColumnId =
  | "input_url"
  | "match_mode"
  | "status"
  | "source"
  | "matched_adobe_value"
  | "notes"
  | `metric:${string}`;

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
  const [queryMode, setQueryMode] = createSignal<QueryMode>("url");
  const [manualMatchMode, setManualMatchMode] = createSignal<"" | "FULL_URL_MODE" | "PATH_MODE">("");
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
  const [copiedColumn, setCopiedColumn] = createSignal<TableColumnId | null>(null);
  let copiedColumnTimer: number | undefined;

  const totalRows = createMemo(() =>
    imports().reduce((sum, i) => sum + i.row_count, 0),
  );
  const matchedCount = createMemo(() => hits().filter((h) => h.matched).length);
  const visibleUrlChips = createMemo(() => urlChips().slice(0, CHIP_PREVIEW_LIMIT));
  const hiddenChipCount = createMemo(() => Math.max(0, urlChips().length - CHIP_PREVIEW_LIMIT));
  const hasMixedImports = createMemo(() =>
    imports().some((imp) => imp.match_mode === "MIXED_MODE"),
  );
  const busyLabel = createMemo(() => {
    switch (busyPhase()) {
      case "import_sources":
        return "Importing Adobe export";
      case "load_url_list":
        return queryMode() === "keyword"
          ? "Loading keyword query list"
          : "Loading lookup URL list";
      case "lookup":
        return queryMode() === "keyword"
          ? "Matching keywords against Adobe data"
          : "Scanning URLs against Adobe data";
      default:
        return "Working";
    }
  });
  const busyHint = createMemo(() => {
    switch (busyPhase()) {
      case "import_sources":
        return "Parsing rows, detecting export profile, and indexing keys.";
      case "load_url_list":
        return queryMode() === "keyword"
          ? "Extracting keyword/query values from the selected file."
          : "Extracting URL values from the selected file.";
      case "lookup":
        return queryMode() === "keyword"
          ? "Scanning URL text and finding rows that contain every keyword term."
          : "Evaluating strict match priority rules and collecting candidates.";
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
  });

  onCleanup(() => {
    if (copiedColumnTimer) {
      window.clearTimeout(copiedColumnTimer);
    }
  });

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
    setInfo(null);
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
      const newSummaries: ImportSummary[] = [];
      const importFailures: string[] = [];
      for (const p of paths) {
        try {
          const summary = await api.importFile(p);
          firstNew.push(...summary.metric_columns);
          newSummaries.push(summary);
        } catch (e: any) {
          importFailures.push(String(e ?? ""));
        }
      }
      if (firstNew.length > 0) {
        await refreshImports();
        if (selectedMetrics().size === 0) {
          const metrics = await api.allMetrics();
          setSelectedMetrics(new Set(metrics.slice(0, 6)));
        } else {
          const cur = new Set(selectedMetrics());
          for (const m of firstNew) {
            if (cur.size >= 8) break;
            cur.add(m);
          }
          setSelectedMetrics(cur);
        }

        // Auto-switch to keyword mode when a keyword source is imported
        const hasKeywordSource = newSummaries.some(
          (s) => s.export_profile === "keyword_export",
        );
        if (hasKeywordSource && queryMode() !== "keyword") {
          setQueryMode("keyword");
          setInfo("Keyword source detected — switched to Keyword mode.");
        }

        setHits([]);
        setExpandedDebug(new Set());
        setMissingMetrics([]);
        setSearchedFiles(0);
        setResultFilter("all");
      }

      if (importFailures.length > 0) {
        setError(importFailures[0]);
      }
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
        const loaded = await api.loadLookupFile(p, queryMode());
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

  function looksLikeUrl(s: string): boolean {
    if (!s) return false;
    if (s.startsWith("/")) return true;
    if (/^https?:\/\//i.test(s)) return true;
    if (/^[a-z0-9-]+(\.[a-z0-9-]+)+(\/|$)/i.test(s)) return true; // bare host
    return false;
  }

  function splitInputs(text: string, mode: QueryMode): string[] {
    if (mode === "keyword") {
      const primary = text
        .split(/[\r\n\t]+/)
        .map((s) => s.trim().replace(/^["']|["']$/g, ""))
        .filter((s) => s.length > 0);
      if (primary.length > 0) return primary;
      return text
        .split(",")
        .map((s) => s.trim().replace(/^["']|["']$/g, ""))
        .filter((s) => s.length > 0);
    }
    // Split on any line ending or tab (Excel multi-column paste).
    // Do NOT split on `;` — it's a valid URL character (matrix params, jsessionid).
    const primary = text
      .split(/[\r\n\t]+/)
      .map((s) => s.trim().replace(/^["']|["']$/g, ""))
      .filter((s) => looksLikeUrl(s));
    if (primary.length > 0) return primary;

    // Fallback for CSV-style one-line paste: "url1,url2,url3"
    // Only used when primary parsing found nothing.
    return text
      .split(",")
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
    const items = splitInputs(chipDraft(), queryMode());
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
    const items = splitInputs(text, queryMode());
    // Only intercept if the paste is multi-line / multi-cell (or multiple
    // parsed entries). Otherwise let the input handle it normally so the user
    // can edit a single value in the draft box.
    if (queryMode() === "keyword") {
      if (/[\r\n\t]/.test(text) || items.length > 1) {
        e.preventDefault();
        addChips(items);
        setChipDraft("");
      }
      return;
    }
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

  function changeQueryMode(mode: QueryMode) {
    if (queryMode() === mode) return;
    setQueryMode(mode);
    setHits([]);
    setExpandedDebug(new Set());
    setMissingMetrics([]);
    setSearchedFiles(0);
    setResultFilter("all");
    setError(null);
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
      setError(queryMode() === "keyword" ? "Add at least one keyword query." : "Add at least one URL.");
      return;
    }
    if (hasMixedImports() && !manualMatchMode()) {
      setError("Mixed export format detected. Choose FULL_URL_MODE or PATH_MODE before lookup.");
      return;
    }
    beginBusy("lookup");
    try {
      const resp = await api.lookupUrls(
        urls,
        [...selectedMetrics()],
        imports().map((imp) => imp.batch_id),
        manualMatchMode() || undefined,
        queryMode(),
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
      "input_url",
      "match_mode",
      "exact_match_found",
      "match_count",
      "status",
      "source_file",
      "matched_adobe_value",
      ...sel,
      "notes",
    ];
    const lines = [header.join(",")];
    for (const h of filteredHits()) {
      if (h.rows.length === 0) {
        lines.push(
          [
            csv(h.query),
            csv(h.match_mode),
            "false",
            "0",
            csv(h.status),
            "",
            "",
            ...sel.map(() => ""),
            csv(h.notes ?? ""),
          ].join(","),
        );
      } else {
        for (const r of h.rows) {
          lines.push(
            [
              csv(h.query),
              csv(h.match_mode),
              String(h.matched),
              String(h.match_count),
              csv(h.status),
              csv(r.source_file ?? ""),
              csv(r.source_url),
              ...sel.map((m) => csv(r.metrics[m] ?? "")),
              csv(h.notes ?? ""),
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

  function columnValue(
    columnId: TableColumnId,
    hit: LookupHit,
    row?: LookupHit["rows"][number],
  ) {
    if (columnId === "input_url") return asDisplayValue(hit.query);
    if (columnId === "match_mode") return asDisplayValue(hit.match_mode);
    if (columnId === "status") return asDisplayValue(hit.status);
    if (columnId === "source") return asDisplayValue(row?.source_file);
    if (columnId === "matched_adobe_value") return asDisplayValue(row?.source_url);
    if (columnId === "notes") return asDisplayValue(hit.notes);
    if (columnId.startsWith("metric:")) {
      const metric = columnId.slice("metric:".length);
      return asDisplayValue(row?.metrics[metric]);
    }
    return "-";
  }

  function collectColumnText(columnId: TableColumnId) {
    // Columns that are per-hit (same for every matched row) vs per-row (differ per match).
    const perHit = columnId === "input_url" || columnId === "match_mode" || columnId === "status" || columnId === "notes";
    const lines: string[] = [];
    for (const h of filteredHits()) {
      if (h.rows.length === 0) {
        lines.push(columnValue(columnId, h));
        continue;
      }
      if (perHit) {
        lines.push(columnValue(columnId, h, h.rows[0]));
      } else {
        for (const r of h.rows) {
          lines.push(columnValue(columnId, h, r));
        }
      }
    }
    return lines.join("\n");
  }

  async function writeToClipboard(text: string) {
    if (navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(text);
      return;
    }
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    ta.style.position = "fixed";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    if (!ok) {
      throw new Error("Clipboard copy failed.");
    }
  }

  async function copyColumn(columnId: TableColumnId) {
    setError(null);
    try {
      await writeToClipboard(collectColumnText(columnId));
      setCopiedColumn(columnId);
      if (copiedColumnTimer) window.clearTimeout(copiedColumnTimer);
      copiedColumnTimer = window.setTimeout(() => setCopiedColumn(null), 1500);
    } catch (e: any) {
      setError(String(e));
    }
  }

  function asDisplayValue(value: string | null | undefined) {
    const cleaned = (value ?? "").trim();
    if (!cleaned) return "-";
    if (/^n\/?a$/i.test(cleaned)) return "-";
    return cleaned;
  }

  function renderHeader(label: string, columnId: TableColumnId) {
    return (
      <div class="th-copy-wrap">
        <span>{label}</span>
        <button
          class={`th-copy-btn ${copiedColumn() === columnId ? "copied" : ""}`}
          onClick={() => copyColumn(columnId)}
          type="button"
          title={`Copy "${label}" column as text`}
        >
          {copiedColumn() === columnId ? "Copied" : "Copy"}
        </button>
      </div>
    );
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
    const parts = [imp.match_mode, imp.export_profile.replace(/_/g, " ")];
    if (imp.truncation_cap != null) {
      parts.push(`cap ${imp.truncation_cap}`);
    }
    return parts.join(" · ");
  }

  function describeMatchType(matchType: string) {
    switch (matchType) {
      case "EXACT_MATCH":
        return "Exact match";
      case "EXACT_DUPLICATE":
        return "Duplicate exact matches found";
      case "NO_MATCH":
        return "No exact match found";
      default:
        return matchType;
    }
  }

  function badgeClass(status: string) {
    if (status === "Matched") {
      return "ok";
    }
    if (status === "Duplicate exact matches found" || status === "Mixed export format") {
      return "warn";
    }
    if (status === "No exact match found" || status === "No keyword match found" || status === "Invalid URL") {
      return "neutral";
    }
    return "neutral";
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
    { value: "matched", label: "Matched" },
    { value: "none", label: "No exact match" },
  ];

  const showMultiFile = createMemo(() => imports().length > 1);
  const debugColSpan = createMemo(
    () => 5 + (showMultiFile() ? 1 : 0) + selectedMetrics().size,
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
            <div class="section-title">Loaded sources ({imports().length})</div>
            <Show
              when={imports().length > 0}
              fallback={
                <div style={{ "font-size": "11.5px", color: "var(--muted)", padding: "4px 4px" }}>
                  No sources loaded yet.
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
                      {describeImportShape(imp)} · {imp.export_profile === "keyword_export" ? "Keyword column" : "URL column"}: {imp.url_column}
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
            <div class="section-title">Matching Mode</div>
            <div class="subtle-note">
              V1 uses strict exact matching only. Canonical mappings and fuzzy matching are disabled.
            </div>
            <Show when={hasMixedImports()}>
              <div style={{ "margin-top": "8px" }}>
                <div class="warn">
                  Mixed export format detected. Choose one mode manually before lookup.
                </div>
                <div class="row" style={{ "margin-top": "8px" }}>
                  <button
                    class={`ghost compact ${manualMatchMode() === "FULL_URL_MODE" ? "active" : ""}`}
                    onClick={() => setManualMatchMode("FULL_URL_MODE")}
                    disabled={busy()}
                  >
                    FULL_URL_MODE
                  </button>
                  <button
                    class={`ghost compact ${manualMatchMode() === "PATH_MODE" ? "active" : ""}`}
                    onClick={() => setManualMatchMode("PATH_MODE")}
                    disabled={busy()}
                  >
                    PATH_MODE
                  </button>
                </div>
              </div>
            </Show>
          </div>
        </aside>

        <section class="content">
          <Show
            when={imports().length > 0 || urlChips().length > 0 || loadedUrlFiles().length > 0}
            fallback={
              <div class="empty">
                <div class="icon">📊</div>
                <h3>No Adobe sources loaded</h3>
                <p>
                  Add one or more Adobe Analytics exports on the left, or load a
                  keyword/URL list file to get started.
                </p>
                <div class="row" style={{ "justify-content": "center", "margin-top": "8px" }}>
                  <button
                    class={`ghost compact ${queryMode() === "url" ? "active" : ""}`}
                    onClick={() => changeQueryMode("url")}
                    disabled={busy()}
                  >
                    URL mode
                  </button>
                  <button
                    class={`ghost compact ${queryMode() === "keyword" ? "active" : ""}`}
                    onClick={() => changeQueryMode("keyword")}
                    disabled={busy()}
                  >
                    Keyword mode
                  </button>
                </div>
                <div class="row" style={{ "justify-content": "center", "margin-top": "8px" }}>
                  <button class="ghost" onClick={pickLookupFiles} disabled={busy()}>
                    {queryMode() === "keyword" ? "Load keyword list file" : "Load URL list file"}
                  </button>
                </div>
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
                {queryMode() === "keyword" ? "Keyword Query List" : "URL List"}
                <Show when={urlChips().length > 0}>
                  <span class="count-pill">{urlChips().length}</span>
                </Show>
              </h2>
              <div class="row" style={{ "margin-bottom": "10px" }}>
                <button
                  class={`ghost compact ${queryMode() === "url" ? "active" : ""}`}
                  onClick={() => changeQueryMode("url")}
                  disabled={busy()}
                >
                  URL mode
                </button>
                <button
                  class={`ghost compact ${queryMode() === "keyword" ? "active" : ""}`}
                  onClick={() => changeQueryMode("keyword")}
                  disabled={busy()}
                >
                  Keyword mode
                </button>
              </div>
              <Show when={queryMode() === "keyword"}>
                <div class="subtle-note" style={{ "margin-bottom": "8px" }}>
                  Keyword mode matches rows whose URL text contains all terms in each query.
                </div>
              </Show>
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
                      ? queryMode() === "keyword"
                        ? "Paste keywords (new lines or tabs) — Enter to add"
                        : "Paste URLs (new lines or tabs) — Enter to add"
                      : queryMode() === "keyword"
                        ? "Add another keyword…"
                        : "Add another URL…"
                  }
                  onInput={(e) => setChipDraft(e.currentTarget.value)}
                  onKeyDown={onChipKeyDown}
                  onPaste={onChipPaste}
                  onBlur={() => commitDraft()}
                />
              </div>
              <Show when={hiddenChipCount() > 0}>
                <div class="subtle-note">
                  Showing first {fmtNum(CHIP_PREVIEW_LIMIT)} {queryMode() === "keyword" ? "queries" : "URLs"} only.
                  Lookup still uses all {fmtNum(urlChips().length)} loaded{" "}
                  {queryMode() === "keyword" ? "queries" : "URLs"}.
                </div>
              </Show>
              <Show when={loadedUrlFiles().length > 0}>
                <div class="file-note-list">
                  <For each={loadedUrlFiles()}>
                    {(loaded) => (
                      <div class="file-note">
                        <div>
                          {loaded.file_name} · {fmtNum(loaded.loaded_count)}{" "}
                          {queryMode() === "keyword" ? "queries" : "URLs"} · column{" "}
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
                <button
                  class="ghost"
                  onClick={pickLookupFiles}
                  disabled={busy()}
                >
                  {queryMode() === "keyword" ? "Load keyword list file" : "Load URL list file"}
                </button>
                <button onClick={runLookup} disabled={busy() || imports().length === 0}>
                  {busy() && busyPhase() === "lookup"
                    ? "Scanning…"
                    : imports().length === 0
                      ? "Add an Adobe source first"
                      : `${queryMode() === "keyword" ? "Match keywords against" : "Look up against"} ${imports().length} Adobe source${imports().length === 1 ? "" : "s"}`}
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
                        <th>{renderHeader(queryMode() === "keyword" ? "Input query" : "Input URL", "input_url")}</th>
                        <th>{renderHeader("Match mode", "match_mode")}</th>
                        <th>{renderHeader("Status", "status")}</th>
                        <Show when={showMultiFile()}>
                          <th>{renderHeader("Source", "source")}</th>
                        </Show>
                        <th>{renderHeader("Matched Adobe value", "matched_adobe_value")}</th>
                        <For each={[...selectedMetrics()]}>
                          {(m) => <th>{renderHeader(m, `metric:${m}`)}</th>}
                        </For>
                        <th>{renderHeader("Notes", "notes")}</th>
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
                                  <td class={queryMode() === "keyword" ? "keyword-cell" : "url-cell"}>
                                    {h.query}
                                  </td>
                                  <td>{h.match_mode}</td>
                                  <td>
                                    <span class={`badge ${badgeClass(h.status)}`}>{h.status}</span>
                                    <button
                                      class="debug-btn"
                                      onClick={() => toggleDebug(h, hitIndex())}
                                    >
                                      {isDebugExpanded(h, hitIndex()) ? "Hide debug" : "Show debug"}
                                    </button>
                                  </td>
                                  <Show when={showMultiFile()}>
                                    <td>-</td>
                                  </Show>
                                  <td>-</td>
                                  <For each={[...selectedMetrics()]}>{() => <td>-</td>}</For>
                                  <td>{asDisplayValue(h.notes)}</td>
                                </tr>
                              }
                            >
                              <For each={h.rows}>
                                {(r, rowIdx) => (
                                  <tr class={h.ambiguous ? "amb" : ""}>
                                    {rowIdx() === 0 && (
                                      <td
                                        class={queryMode() === "keyword" ? "keyword-cell" : "url-cell"}
                                        rowSpan={h.rows.length}
                                      >
                                        {h.query}
                                      </td>
                                    )}
                                    {rowIdx() === 0 && (
                                      <td rowSpan={h.rows.length}>{h.match_mode}</td>
                                    )}
                                    {rowIdx() === 0 && (
                                      <td rowSpan={h.rows.length}>
                                        <span class={`badge ${badgeClass(h.status)}`}>{h.status}</span>
                                        <button
                                          class="debug-btn"
                                          onClick={() => toggleDebug(h, hitIndex())}
                                        >
                                          {isDebugExpanded(h, hitIndex()) ? "Hide debug" : "Show debug"}
                                        </button>
                                      </td>
                                    )}
                                    <Show when={showMultiFile()}>
                                      <td class="src-cell">{asDisplayValue(r.source_file)}</td>
                                    </Show>
                                    <td class="url-cell">{asDisplayValue(r.source_url)}</td>
                                    <For each={[...selectedMetrics()]}>
                                      {(m) => <td>{asDisplayValue(r.metrics[m])}</td>}
                                    </For>
                                    {rowIdx() === 0 && (
                                      <td rowSpan={h.rows.length}>{asDisplayValue(h.notes)}</td>
                                    )}
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
