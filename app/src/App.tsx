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
      case "import_sources": return "Importing…";
      case "load_url_list": return "Loading…";
      case "lookup": return "Scanning…";
      default: return "Working…";
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
    if (copiedColumnTimer) window.clearTimeout(copiedColumnTimer);
  });

  async function refreshImports() {
    setImports(await api.listImports());
    const metrics = await api.allMetrics();
    setAllMetrics(metrics);
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
        filters: [{ name: "Spreadsheet", extensions: ["csv", "tsv", "txt", "xlsx", "xls", "xlsm"] }],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      beginBusy("import_sources");
      let firstNew: string[] = [];
      const newSummaries: ImportSummary[] = [];
      for (const p of paths) {
        try {
          const summary = await api.importFile(p);
          firstNew.push(...summary.metric_columns);
          newSummaries.push(summary);
        } catch (e: any) {
          setError(String(e));
        }
      }
      if (firstNew.length > 0) {
        await refreshImports();
        if (selectedMetrics().size === 0) {
          const metrics = await api.allMetrics();
          setSelectedMetrics(new Set(metrics.slice(0, 6)));
        }
        if (newSummaries.some(s => s.export_profile === "keyword_export") && queryMode() !== "keyword") {
          setQueryMode("keyword");
          setInfo("Switched to Keyword mode.");
        }
        setHits([]);
      }
    } finally {
      endBusy();
    }
  }

  async function pickLookupFiles() {
    setError(null);
    try {
      const picked = await open({
        multiple: true,
        filters: [{ name: "Spreadsheet", extensions: ["csv", "tsv", "txt", "xlsx", "xls", "xlsm"] }],
      });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      beginBusy("load_url_list");
      for (const p of paths) {
        const loaded = await api.loadLookupFile(p, queryMode());
        addChips(loaded.urls);
        setLoadedUrlFiles(prev => [...prev, { ...loaded, loaded_count: loaded.urls.length }]);
      }
      setHits([]);
    } finally {
      endBusy();
    }
  }

  async function deleteImport(batchId: string, ev: MouseEvent) {
    ev.stopPropagation();
    await api.removeImport(batchId);
    await refreshImports();
    setHits([]);
  }

  function looksLikeUrl(s: string): boolean {
    if (!s) return false;
    return s.startsWith("/") || /^https?:\/\//i.test(s) || /^[a-z0-9-]+(\.[a-z0-9-]+)+(\/|$)/i.test(s);
  }

  function splitInputs(text: string, mode: QueryMode): string[] {
    const regex = /[\r\n\t]+/;
    if (mode === "keyword") {
      return text.split(regex).map(s => s.trim().replace(/^["']|["']$/g, "")).filter(s => s.length > 0);
    }
    return text.split(regex).map(s => s.trim().replace(/^["']|["']$/g, "")).filter(s => looksLikeUrl(s));
  }

  function addChips(items: string[]) {
    if (items.length > 0) setUrlChips([...urlChips(), ...items]);
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
    setLoadedUrlFiles([]);
  }

  function toggleMetric(m: string) {
    const s = new Set(selectedMetrics());
    if (s.has(m)) s.delete(m); else s.add(m);
    setSelectedMetrics(s);
  }

  async function runLookup() {
    setError(null);
    if (chipDraft().trim()) commitDraft();
    const urls = urlChips();
    if (urls.length === 0 || imports().length === 0) return;
    beginBusy("lookup");
    try {
      const resp = await api.lookupUrls(urls, [...selectedMetrics()], imports().map(i => i.batch_id), manualMatchMode() || undefined, queryMode());
      setHits(resp.hits);
      setSearchedFiles(resp.searched_files);
    } catch (e: any) {
      setError(String(e));
    } finally {
      endBusy();
    }
  }

  function exportCsv() {
    const sel = [...selectedMetrics()];
    const header = ["input_url", "match_mode", "status", "source", "matched_adobe_value", ...sel].join(",");
    const lines = [header];
    for (const h of hits()) {
      if (h.rows.length === 0) {
        lines.push([csv(h.query), csv(h.match_mode), csv(h.status), "", "", ...sel.map(() => "")].join(","));
      } else {
        for (const r of h.rows) {
          lines.push([csv(h.query), csv(h.match_mode), csv(h.status), csv(r.source_file ?? ""), csv(r.source_url), ...sel.map(m => csv(r.metrics[m] ?? ""))].join(","));
        }
      }
    }
    const blob = new Blob([lines.join("\n")], { type: "text/csv" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url; a.download = "lookup-results.csv"; a.click();
    URL.revokeObjectURL(url);
  }

  function asDisplayValue(v: any) {
    const s = String(v ?? "").trim();
    return !s || /^n\/?a$/i.test(s) ? "-" : s;
  }

  function csv(s: string) {
    if (s.includes(",") || s.includes('"') || s.includes("\n")) return '"' + s.replace(/"/g, '""') + '"';
    return s;
  }

  function fmtNum(n: number) { return n.toLocaleString(); }

  function dotClass(status: string) {
    if (status === "Matched") return "ok";
    if (status.includes("Duplicate") || status.includes("Mixed")) return "warn";
    if (status.includes("No match") || status.includes("Invalid")) return "err";
    return "neutral";
  }

  const resultFilterOptions: Array<{ value: ResultFilter; label: string }> = [
    { value: "all", label: "All" },
    { value: "matched", label: "Matched" },
    { value: "none", label: "Missing" },
  ];

  const filteredHits = createMemo(() => hits().filter(h => {
    if (resultFilter() === "matched") return h.matched;
    if (resultFilter() === "none") return !h.matched;
    return true;
  }));

  const debugColSpan = createMemo(() => 5 + (imports().length > 1 ? 1 : 0) + selectedMetrics().size);

  return (
    <div class="app">
      <header class="titlebar" data-tauri-drag-region onMouseDown={onTitlebarMouseDown}>
        <h1>Adobe Analytics Parser</h1>
        <span class="meta">
          {imports().length > 0 ? `${imports().length} Sources · ${fmtNum(totalRows())} Rows` : "No sources loaded"}
        </span>
      </header>

      <main>
        <aside class="sidebar">
          <div class="sidebar-section">
            <div class="section-title">Sources</div>
            <button class="ghost compact sidebar-action-btn" onClick={pickSourceFiles}>
              + Add Adobe Export
            </button>
            <Show when={imports().length === 0}>
              <div class="sidebar-empty">No sources loaded.</div>
            </Show>
            <For each={imports()}>
              {(imp) => (
                <div class="source-item">
                  <div class="source-info">
                    <div class="source-name" title={imp.file_name}>{imp.file_name}</div>
                    <div class="source-meta">{fmtNum(imp.row_count)} rows</div>
                  </div>
                  <button class="x-btn" onClick={(e) => deleteImport(imp.batch_id, e)}>×</button>
                </div>
              )}
            </For>
          </div>

          <Show when={hasMixedImports()}>
            <div class="mixed-mode-alert">
              <div class="warn">Mixed export format detected.</div>
              <div class="toolbar-group" style={{ "margin-top": "8px" }}>
                <div class="segmented-control">
                  <button class={`segmented-btn ${manualMatchMode() === "FULL_URL_MODE" ? "active" : ""}`} onClick={() => setManualMatchMode("FULL_URL_MODE")}>URL</button>
                  <button class={`segmented-btn ${manualMatchMode() === "PATH_MODE" ? "active" : ""}`} onClick={() => setManualMatchMode("PATH_MODE")}>Path</button>
                </div>
              </div>
            </div>
          </Show>

          <div class="sidebar-section" style={{ "margin-top": "auto" }}>
            <Show when={error()}>
              <div class="sidebar-empty" style={{ color: "var(--err)" }}>{error()}</div>
            </Show>
            <Show when={info()}>
              <div class="sidebar-empty" style={{ color: "var(--ok)" }}>{info()}</div>
            </Show>
          </div>
        </aside>

        <section class="content">
          <div class="toolbar">
            <div class="toolbar-group">
              <div class="segmented-control">
                <button class={`segmented-btn ${queryMode() === "url" ? "active" : ""}`} onClick={() => setQueryMode("url")}>URL Mode</button>
                <button class={`segmented-btn ${queryMode() === "keyword" ? "active" : ""}`} onClick={() => setQueryMode("keyword")}>Keyword Mode</button>
              </div>
            </div>
            <div class="toolbar-group">
              <span class="meta">{selectedMetrics().size} metrics active</span>
            </div>
            <div style={{ "margin-left": "auto" }}>
              <button onClick={runLookup} disabled={busy() || imports().length === 0}>
                {busy() ? "Working…" : "Run Scan"}
              </button>
            </div>
          </div>

          <div class="main-scroller">
            <Show when={imports().length === 0 && urlChips().length === 0} fallback={
              <>
                <div class="panel-section">
                  <div class="panel-header">
                    <h2>Configuration</h2>
                  </div>
                  <div class="metric-grid">
                    <For each={allMetrics()}>
                      {(m) => (
                        <label class={`metric ${selectedMetrics().has(m) ? "active" : ""}`}>
                          <input type="checkbox" checked={selectedMetrics().has(m)} onChange={() => toggleMetric(m)} hidden />
                          {m}
                        </label>
                      )}
                    </For>
                  </div>
                </div>

                <div class="panel-section">
                  <div class="panel-header">
                    <h2>{queryMode() === "keyword" ? "Keyword List" : "URL List"}</h2>
                    <Show when={urlChips().length > 0}>
                      <button class="ghost compact" onClick={clearChips}>Clear All</button>
                    </Show>
                  </div>
                  <div class="chip-input-container">
                    <div class="chip-input" onClick={(e) => (e.currentTarget.querySelector("input") as any)?.focus()}>
                      <For each={visibleUrlChips()}>
                        {(u, i) => (
                          <span class="chip">
                            <span class="chip-text">{u}</span>
                            <button class="chip-x" onClick={(e) => { e.stopPropagation(); removeChip(i()); }}>×</button>
                          </span>
                        )}
                      </For>
                      <input type="text" class="chip-draft" value={chipDraft()} placeholder="Paste or type..." onInput={e => setChipDraft(e.currentTarget.value)} onKeyDown={e => e.key === "Enter" && commitDraft()} onBlur={commitDraft} />
                    </div>
                    <div class="toolbar-group">
                      <button class="ghost compact" onClick={pickLookupFiles}>Import File</button>
                      <Show when={hiddenChipCount() > 0}>
                        <span class="meta">+{hiddenChipCount()} more items</span>
                      </Show>
                    </div>
                  </div>
                </div>

                <Show when={busy()}>
                  <div class="scan-status">
                    <div class="scan-spinner" />
                    <div class="scan-title">{busyLabel()}</div>
                    <div class="scan-time">{busySeconds()}s</div>
                  </div>
                </Show>

                <Show when={hits().length > 0}>
                  <div class="panel-section">
                    <div class="panel-header">
                      <h2>Results ({hits().length})</h2>
                      <div class="toolbar-group">
                        <div class="segmented-control">
                          <For each={resultFilterOptions}>
                            {(opt) => (
                              <button class={`segmented-btn ${resultFilter() === opt.value ? "active" : ""}`} onClick={() => setResultFilter(opt.value)}>{opt.label}</button>
                            )}
                          </For>
                        </div>
                        <button class="ghost compact" onClick={exportCsv}>Export CSV</button>
                      </div>
                    </div>
                    <div class="results-panel">
                      <div class="table-container">
                        <table>
                          <thead>
                            <tr>
                              <th>Input</th>
                              <th>Status</th>
                              <For each={[...selectedMetrics()]}>{(m) => <th>{m}</th>}</For>
                              <th>Notes</th>
                            </tr>
                          </thead>
                          <tbody>
                            <For each={filteredHits()}>
                              {(h) => (
                                <>
                                  <For each={h.rows.length > 0 ? h.rows : [null]}>
                                    {(row, idx) => (
                                      <tr>
                                        {idx() === 0 && <td class="url-cell" rowSpan={Math.max(1, h.rows.length)}>{h.query}</td>}
                                        {idx() === 0 && <td rowSpan={Math.max(1, h.rows.length)}>
                                          <div class="toolbar-group">
                                            <div class={`status-dot ${dotClass(h.status)}`} />
                                            <span class="meta">{h.status}</span>
                                          </div>
                                        </td>}
                                        <For each={[...selectedMetrics()]}>
                                          {(m) => <td>{asDisplayValue(row?.metrics[m])}</td>}
                                        </For>
                                        {idx() === 0 && <td rowSpan={Math.max(1, h.rows.length)}>{asDisplayValue(h.notes)}</td>}
                                      </tr>
                                    )}
                                  </For>
                                </>
                              )}
                            </For>
                          </tbody>
                        </table>
                      </div>
                    </div>
                  </div>
                </Show>
              </>
            }>
              <div class="empty">
                <div class="icon">✦</div>
                <h3>Start your analysis</h3>
                <p>Import Adobe Analytics exports and a list of target URLs or keywords to begin matching.</p>
                <div class="toolbar-group" style={{ "margin-top": "32px" }}>
                  <button onClick={pickSourceFiles}>Add Adobe Sources</button>
                  <button class="ghost" onClick={pickLookupFiles}>Import Target List</button>
                </div>
              </div>
            </Show>
          </div>
        </section>
      </main>
    </div>
  );
}
