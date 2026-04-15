import { createSignal, createMemo, For, Show, onMount } from "solid-js";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { api, type ImportSummary, type LookupHit } from "./api";

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
  const [missingMetrics, setMissingMetrics] = createSignal<string[]>([]);
  const [searchedFiles, setSearchedFiles] = createSignal(0);
  const [error, setError] = createSignal<string | null>(null);
  const [busy, setBusy] = createSignal(false);

  const totalRows = createMemo(() =>
    imports().reduce((sum, i) => sum + i.row_count, 0),
  );
  const matchedCount = createMemo(() => hits().filter((h) => h.matched).length);

  onMount(async () => {
    setImports(await api.listImports());
    setAllMetrics(await api.allMetrics());
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

  async function pickFile() {
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
      setBusy(true);
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
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
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
      setError("Upload at least one file first.");
      return;
    }
    // Commit any pending text in the input first
    if (chipDraft().trim()) commitDraft();
    const urls = urlChips();
    if (urls.length === 0) {
      setError("Add at least one URL.");
      return;
    }
    setBusy(true);
    try {
      const resp = await api.lookupUrls(urls, [...selectedMetrics()]);
      setHits(resp.hits);
      setMissingMetrics(resp.missing_metrics);
      setSearchedFiles(resp.searched_files);
    } catch (e: any) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  function exportCsv() {
    const sel = [...selectedMetrics()];
    const header = ["query", "matched", "match_count", "source_file", "source_url", ...sel];
    const lines = [header.join(",")];
    for (const h of hits()) {
      if (h.rows.length === 0) {
        lines.push([csv(h.query), "false", "0", "", "", ...sel.map(() => "")].join(","));
      } else {
        for (const r of h.rows) {
          lines.push(
            [
              csv(h.query),
              "true",
              String(h.match_count),
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
    a.download = "lookup-results.csv";
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

  const showMultiFile = createMemo(() => imports().length > 1);

  return (
    <div class="app">
      <header class="titlebar" data-tauri-drag-region onMouseDown={onTitlebarMouseDown}>
        <h1>Adobe Analytics Parser</h1>
        <span class="meta">
          <Show
            when={imports().length > 0}
            fallback="No files loaded"
          >
            {imports().length} file{imports().length === 1 ? "" : "s"} · {fmtNum(totalRows())} rows
          </Show>
        </span>
      </header>

      <main>
        <aside class="sidebar">
          <div>
            <div class="section-title">Source</div>
            <button class="full" onClick={pickFile} disabled={busy()}>
              {busy() ? "Working…" : "Add CSV / XLSX"}
            </button>
            <Show when={error()}>
              <div class="err">{error()}</div>
            </Show>
          </div>

          <div>
            <div class="section-title">Imports ({imports().length})</div>
            <Show
              when={imports().length > 0}
              fallback={
                <div style={{ "font-size": "11.5px", color: "var(--muted)", padding: "4px 4px" }}>
                  No imports yet.
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
                    <Show when={imp.warnings.length > 0}>
                      <div class="warn">{imp.warnings.join(" · ")}</div>
                    </Show>
                  </div>
                )}
              </For>
            </Show>
          </div>
        </aside>

        <section class="content">
          <Show
            when={imports().length > 0}
            fallback={
              <div class="empty">
                <div class="icon">📊</div>
                <h3>No files loaded</h3>
                <p>
                  Add one or more CSV / XLSX exports on the left. Lookups search
                  across every file at once.
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
                    No metric columns detected.
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
                URLs
                <Show when={urlChips().length > 0}>
                  <span class="count-pill">{urlChips().length}</span>
                </Show>
              </h2>
              <div class="chip-input" onClick={(e) => {
                const input = (e.currentTarget as HTMLElement).querySelector("input");
                input?.focus();
              }}>
                <For each={urlChips()}>
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
                      ? "Paste URLs (newlines or ; separated) — Enter to add"
                      : "Add another…"
                  }
                  onInput={(e) => setChipDraft(e.currentTarget.value)}
                  onKeyDown={onChipKeyDown}
                  onPaste={onChipPaste}
                  onBlur={() => commitDraft()}
                />
              </div>
              <div class="row" style={{ "margin-top": "12px" }}>
                <button onClick={runLookup} disabled={busy()}>
                  {busy() ? "Looking up…" : `Look up across ${imports().length} file${imports().length === 1 ? "" : "s"}`}
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
              <Show when={missingMetrics().length > 0}>
                <div class="warn">
                  Not present in any file: {missingMetrics().join(", ")}
                </div>
              </Show>
            </div>

            <Show when={hits().length > 0}>
              <div class="card">
                <h2>Results · searched {searchedFiles()} file{searchedFiles() === 1 ? "" : "s"}</h2>
                <div style={{ overflow: "auto", "max-height": "calc(100vh - 360px)" }}>
                  <table>
                    <thead>
                      <tr>
                        <th>Query</th>
                        <th>Status</th>
                        <Show when={showMultiFile()}>
                          <th>Source</th>
                        </Show>
                        <th>Matched URL</th>
                        <For each={[...selectedMetrics()]}>{(m) => <th>{m}</th>}</For>
                      </tr>
                    </thead>
                    <tbody>
                      <For each={hits()}>
                        {(h) => (
                          <Show
                            when={h.rows.length > 0}
                            fallback={
                              <tr class="miss">
                                <td class="url-cell">
                                  {h.query}
                                  <div class="normalized-hint">
                                    looked up as: {h.normalized_query}
                                  </div>
                                </td>
                                <td>
                                  <span class="badge err">no match</span>
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
                              {(r, i) => (
                                <tr class={h.ambiguous ? "amb" : ""}>
                                  <td class="url-cell">
                                    {i() === 0 ? h.query : ""}
                                  </td>
                                  <td>
                                    <Show
                                      when={i() === 0}
                                      fallback={<span class="badge ok">+ same query</span>}
                                    >
                                      <Show
                                        when={h.ambiguous}
                                        fallback={<span class="badge ok">match</span>}
                                      >
                                        <span class="badge warn">{h.match_count} matches</span>
                                      </Show>
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
