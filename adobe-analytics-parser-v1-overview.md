# Adobe Analytics Parser App — V1 Overview

## Purpose
This application is a local desktop tool for analytics-export parsing. Its primary job is to accept uploaded Adobe Analytics exports, normalize the data, match one or more URLs, and return the requested metrics without manual hunting inside Adobe Analytics.

V1 focuses on deterministic parsing and extraction. AI is intentionally excluded from the critical extraction path in V1 so the output stays transparent, debuggable, and repeatable.

## Recommended Tech Stack

| Layer | Recommended tech | Why this fits V1 |
|---|---|---|
| Desktop shell | Tauri v2 | Tauri is designed for building cross-platform desktop apps with a web frontend and a Rust backend, which fits a local internal tool well. [cite:25][cite:27] |
| Frontend | SolidJS + TypeScript + Vite | This is a strong fit for a fast internal UI with reactive filters, inputs, tables, and lightweight state management. |
| Backend commands | Rust | Rust is a good choice for file parsing, normalization, validation, and deterministic extraction in a desktop app built with Tauri. [cite:25][cite:27] |
| Local query engine | DuckDB | DuckDB is well suited for local analytics workflows and can query local files such as CSV, Parquet, and JSON efficiently without needing a server. [cite:41][cite:44] |
| Spreadsheet support | SheetJS | SheetJS supports browser-style reading and writing of spreadsheet files, which makes it practical for XLSX ingest now and export generation later. [cite:31][cite:37] |
| Future AI layer | Local lightweight model or heuristic layer | Keep this optional and separate from the extraction engine in V1. |

## Why This Stack
The app is file-based, local-first, and parser-heavy. That makes a desktop architecture more suitable than a cloud-first web application for the first release.

Tauri v2 is the best-fit shell because it supports a modern web UI while keeping system-level logic in Rust. [cite:25][cite:27] DuckDB is the right analytics layer because it is designed for local analytical queries on files and structured data. [cite:41][cite:44] SheetJS rounds out the workflow because it can read uploaded spreadsheet files and later support generated workbook outputs. [cite:31][cite:37]

## V1 Scope
The first version should do only the following:

- Accept CSV and XLSX uploads.
- Detect headers and normalize them into a standard schema.
- Normalize URLs so matching is reliable.
- Support exact match and normalized match for URLs.
- Return one or more requested metrics from the uploaded dataset.
- Show the matched row or rows used for the result.
- Export extracted results later if needed, but keep reporting automation out of scope for the first release.

## Non-Goals for V1
These items should not be in the first build:

- Full AI assistant behavior.
- Automatic narrative report writing.
- Advanced formatting templates.
- Background syncing with Adobe Analytics.
- OCR or image-based extraction.
- Fine-tuning or training a custom LLM.

## Core Data Flow

1. The user uploads a CSV or XLSX export.
2. The frontend sends the file path or parsed payload to the Rust layer.
3. Rust validates the file, normalizes headers, and prepares a clean table.
4. The normalized data is inserted into DuckDB or queried through DuckDB for extraction logic. [cite:41][cite:44]
5. The user enters a URL and chooses a metric or asks for all metrics.
6. The backend normalizes the URL and runs the lookup.
7. The UI shows the result, matched source rows, and any warnings.

## Normalization Rules
The parser should apply a consistent normalization pipeline before matching:

- Trim whitespace.
- Lowercase the hostname.
- Remove trailing slashes when appropriate.
- Ignore URL fragments.
- Optionally strip query parameters when the export treats canonical pages as the same URL.
- Preserve the raw original value for auditing.

This prevents false mismatches between variants such as `https://www.hitachienergy.com` and `https://www.hitachienergy.com/`.

## Suggested Folder Structure

```text
adobe-analytics-parser/
├── app/
│   ├── package.json
│   ├── vite.config.ts
│   ├── src/
│   │   ├── main.tsx
│   │   ├── App.tsx
│   │   ├── components/
│   │   │   ├── UploadPanel.tsx
│   │   │   ├── UrlSearchForm.tsx
│   │   │   ├── MetricSelector.tsx
│   │   │   ├── ResultsTable.tsx
│   │   │   ├── ResultCards.tsx
│   │   │   └── WarningBanner.tsx
│   │   ├── features/
│   │   │   ├── uploads/
│   │   │   ├── parsing/
│   │   │   ├── metrics/
│   │   │   └── search/
│   │   ├── lib/
│   │   │   ├── api.ts
│   │   │   ├── url.ts
│   │   │   ├── schema.ts
│   │   │   └── format.ts
│   │   └── styles/
│   │       └── app.css
│   └── src-tauri/
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs
│           ├── commands/
│           │   ├── import_file.rs
│           │   ├── extract_metrics.rs
│           │   ├── validate_schema.rs
│           │   └── export_results.rs
│           ├── parser/
│           │   ├── csv_parser.rs
│           │   ├── xlsx_parser.rs
│           │   ├── header_mapper.rs
│           │   └── normalizer.rs
│           ├── query/
│           │   ├── duckdb.rs
│           │   ├── search.rs
│           │   └── metrics.rs
│           ├── models/
│           │   ├── upload.rs
│           │   ├── row.rs
│           │   └── result.rs
│           └── utils/
│               ├── errors.rs
│               └── logging.rs
├── docs/
│   ├── overview.md
│   ├── schema-mapping.md
│   └── roadmap.md
└── samples/
    ├── sample-export.csv
    └── sample-export.xlsx
```

## Module Responsibilities

### Frontend
The frontend handles user interaction only:
- File upload.
- URL input.
- Metric selection.
- Result display.
- Error and warning display.
- Export action triggers.

### Rust backend
The Rust backend handles deterministic logic:
- File validation.
- Header mapping.
- URL normalization.
- Schema normalization.
- Query execution.
- Export generation.

### DuckDB layer
DuckDB should be treated as the analytics/query engine rather than the source of business rules. It is there to make filtering, lookup, grouping, and future comparisons fast and maintainable. [cite:41][cite:44]

## Standard Schema for V1
The app should normalize source exports into a standard schema such as:

| Standard field | Purpose |
|---|---|
| `source_url` | Original URL from upload |
| `normalized_url` | URL used for matching |
| `page_title` | Optional page name if available |
| `traffic_source` | Organic, direct, referral, or raw source value |
| `metric_sessions` | Sessions or visits equivalent |
| `metric_pageviews` | Page views equivalent |
| `metric_users` | Unique visitors or users equivalent |
| `metric_engagement` | Engagement metric if present |
| `raw_row_id` | Traceability back to the uploaded row |
| `import_batch_id` | Distinguishes one upload from another |

## V1 Screens
The initial product can stay compact with four screens or views:

1. Upload view.
2. Parsed file summary.
3. URL lookup and metric extraction view.
4. Results table with optional export action.

## Error Handling
The app should clearly explain these cases:
- Missing required URL column.
- Unsupported file type.
- No matching URL found.
- More than one ambiguous match found.
- Metric requested but not present in the uploaded export.
- Empty rows or malformed spreadsheets.

## Testing Priorities
The first test set should cover:
- CSV import success.
- XLSX import success.
- Header alias mapping.
- URL normalization behavior.
- Exact match lookup.
- Normalized match lookup.
- Missing metric handling.
- Duplicate URL handling.
- Export output correctness.

## Future Roadmap
After V1 is stable, the next layers can be added in this order:

1. Saved mapping profiles for recurring export formats.
2. Batch URL lookup.
3. Comparison mode across uploads.
4. Output file generation for reporting.
5. Template-based formatting for recurring deliverables.
6. Lightweight AI for natural-language commands.
7. AI summaries and report drafting.

## Decision Summary
The recommended foundation for the parser app is:
- Tauri v2 for the desktop shell. [cite:25][cite:27]
- SolidJS + TypeScript for the UI.
- Rust for parsing and command execution. [cite:25][cite:27]
- DuckDB for local analytical querying. [cite:41][cite:44]
- SheetJS for spreadsheet ingestion and future workbook output. [cite:31][cite:37]

This stack is the best fit for a local, parser-first, file-driven V1 with a clear path to future automation.
