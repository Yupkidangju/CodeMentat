# Changelog

All notable changes to the **Code Mentat** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #10 Remediation)

패키지 SemVer는 `0.1.0`이며 이 항목은 그 개발 스냅샷이다.

### Fixed
- **[IMP-F004] Cloud evidence contract:**
  - Approved requests now include AnswerBundle JSON schema, current snapshot ID, and per-file path/hash/line range.
  - The app validates model output with included file contents via `from_model_text_with_contents`.
- **[SEC-F002] Outbound question filtering:**
  - `user_question` is scanned with the same secret filter as file content.
  - The question is sent once; adapters no longer append a second raw copy.
- **[DBG-F003] App-level scan cancellation:**
  - Indexing uses `scan_files_with_limits` and shows omission reasons.
- **[DBG-F002] Preserved-mtime same-size edits:**
  - Periodic content fingerprint rehash marks the snapshot STALE.
- **[IMP-F003] UI contract:**
  - `designs.md` / README now match implemented viewport sizes, theme tokens, and registered shortcuts.
- **[IMP-F002] Version policy:**
  - Cargo `0.1.0` is the package version; `0.1.0-dev` is the unreleased document status.

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #9 Remediation)

### Fixed
- **[IMP-F004] Strict cloud citation validation and real conflict fixture:**
  - Current snapshot ID is forced; FileRecord hash, excerpt, and line_end are checked.
  - Any invalid evidence in a claim downgrades the claim to `Unknown`.
  - `/conflicts` compares document-claimed language/paths against the scanned tree.
- **[DBG-F008] Watcher constructor no longer walks the tree on the UI thread:**
  - Initial signature is computed in the worker; stop no longer joins on Drop.
- **[DBG-F003] Scan cancellation, preflight, and representative budget profile:**
  - `ScanOutcome` records omissions; oversized files are skipped before hashing.
  - Mid-scan cancel and giant-file/limit fixtures were added.
- **[SEC-F002] Generic high-entropy redaction and relevance threshold:**
  - High-entropy tokens are masked; score-0 files are not retrieved into egress packets.

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #8 Remediation)

### Fixed
- **[SEC-F011] Exclusion toggle invalidates the pending packet immediately:**
  - `ConsentAssemblyState` increments a generation, clears the old packet, and blocks approve until the matching generation result arrives.
  - Stale assembler results from a previous exclusion set are discarded.
- **[DBG-F005] Formatter and wire-level adapter fixtures:**
  - Restored `cargo fmt --all -- --check`.
  - Added loopback HTTP fixtures for 401/429/5xx, cancel-during-send, and split SSE chunks.
- **[DBG-F008] Watcher walk moved off the UI thread:**
  - Background thread computes tree signatures; the UI only polls a channel.
- **[IMP-F005] Stable repository identity and snapshot restore:**
  - Canonical root lookup reuses the previous repo UUID and consumes stored snapshot metadata.
- **[IMP-F004] Cloud AnswerBundle validation:**
  - Structured JSON is citation-checked against the current snapshot.
  - Unstructured model text is marked `UNSTRUCTURED_RESPONSE` instead of high-confidence inferred bullets.

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #7 Remediation)

### Fixed
- **[DBG-F008] Watcher I/O Polling Decoupled from UI Frame Rate:**
  - Added 1,000ms minimum throttling (`WATCHER_THROTTLE_INTERVAL`) to `RepositoryWatcher::check_for_changes()`.
  - Eliminated high frequency (~60fps) filesystem directory tree walks during active UI streaming/repaint wakeups.
- **[DBG-F002] Deterministic Sorted Snapshot & Watcher Tree Signature:**
  - Enforced lexicographical sorting on `files` before SHA-256 tree digest calculation in `ReadOnlySession::create_snapshot_from_files`.
  - Upgraded `RepositoryWatcher` with `TreeSignature` tracking `(file_count, total_size, latest_mtime)` to reliably detect deletions, additions, and mtime rollbacks.
- **[DBG-F003] Deterministic Repository Scan Benchmark Test:**
  - Added `test_dbg_f003_scan_and_snapshot_deterministic_benchmark` asserting 100-file recursive scanning and snapshot hashing in under 500ms.
- **[IMP-F004] Request-Scoped State Reset & Cloud Response Claim Normalization:**
  - Reset previous claims, recommendations, conflicts, and evidence maps at the start of each query in `MentatApp::handle_query`.
  - Added automatic cloud response normalization parsing markdown headers and bullet points into structured `Claim` objects tagged as `ClaimClassification::Inferred`.
- **[IMP-F005] Recent Repository Quick Reopening & Snapshot History Restoration:**
  - Connected `recent_repos` in UI settings allowing one-click reopening of previous repositories.
  - Automatically restored latest known snapshot metadata from SQLite storage upon opening a repository.
- **[SEC-F002] Interactive User Exclusion UI & Extended Token Redaction:**
  - Added interactive file exclusion checkboxes to Egress Consent Sheet, dynamically reassembling egress context without excluded files.
  - Added pattern redaction for Anthropic keys (`sk-ant-`), HuggingFace tokens (`hf_`), and Slack tokens (`xoxb-`/`xoxp-`).
- **[DBG-F005] Wire-Level Integration Tests:**
  - Added fail-closed tests for invalid base URLs in `MultiProviderAdapter::health_check`.

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #6 Remediation)

### Fixed
- **[DBG-F002 & DBG-F003] Single-Scan Snapshot, Recursive Tree Watcher & Scan Budget Limits:**
  - Implemented `create_snapshot_from_files` in `ReadOnlySession` to construct tree digests directly from scanned file records without redundant second disk traversals.
  - Upgraded `RepositoryWatcher` to perform recursive bounded file mtime inspections across subdirectories.
  - Enforced scan budgets (maximum 100,000 files, 2GiB total scanned size, 10MB per-file read limit) to guard against resource exhaustion.
- **[IMP-F004] Evidence-Linked Workflows & `/risks` Analysis:**
  - Attached verified `EvidenceRef` items with exact line numbers and content hashes to all local workflows (`/onboard`, `/structure`, `/conflicts`, `/where`).
  - Added new `/risks` workflow analyzing missing regression tests and unconfirmed decisions.
- **[IMP-F005] Profile & Snapshot History Persistence:**
  - Implemented SQLite persistence for active `BackendProfile` configuration (excluding plaintext API keys per CON-007).
  - Added repository snapshot history tracking (`snapshot_history` table) and automatic profile restoration on app launch.
- **[SEC-F002] Generic Bearer Token Redaction & Per-Request User Exclusions:**
  - Added pattern detection for generic `Bearer <token>` authorization headers in `EgressFilter`.
  - Added `assemble_packet_with_user_exclusions` supporting per-request user file exclusions.
- **[DBG-F005 & DBG-F007] Production Adapter Integration Tests & Stream Disconnect Terminal Handling:**
  - Handled `TryRecvError::Disconnected` in UI streaming event loop to transition cleanly out of streaming state upon abnormal background termination.
  - Added adapter integration tests verifying missing key fail-closed behavior and pre-response cancellation abort.
- **[IMP-F006] Calibrated Traceability Matrix:**
  - Recalibrated remaining baseline items in `spec.md` to honest `Partial` statuses with 34 passing tests.

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #5 Remediation)

### Fixed
- **[DBG-F007 & DBG-F001] Zero-Blocking File Preview, UI Repaint Wakeup & Async Task Error Handling:**
  - Removed synchronous `recv_timeout(100ms)` on the UI thread and converted file preview loading to non-blocking async `preview_rx`.
  - Added continuous UI frame wakeup (`ctx.request_repaint_after(16ms)`) whenever background tasks are active.
  - Implemented fail-closed channel disconnect and error consumption across all scanning, pinging, query, egress, and preview task receivers.
- **[SEC-F004] Robust Parsed URL Loopback Validation & Userinfo Rejection:**
  - Integrated `url` parser with strict validation rejecting userinfo (`@`), arbitrary HTTP subdomains (e.g. `localhost.evil.com`), non-loopback HTTP, and unsupported protocol schemes.
- **[SEC-F006] Canonical Path Direct-Open & Boundary Enforcement:**
  - Enforced fail-closed path canonicalization and verified canonical repository root boundaries.
  - Directly opened `File::open(&canonical_path)` to eliminate symlink TOCTOU swap races.
- **[SEC-F005] Pre-Response Cancellation & Byte-Buffered SSE Framing:**
  - Wrapped `.send().await` in `tokio::select!` with `cancel_token.cancelled()` for early cancellation before HTTP response header arrival.
  - Replaced lossy string chunk conversions with `Vec<u8>` raw byte accumulator for split multibyte UTF-8 stream decoding.
- **[SEC-F007] Accepted Risk Governance for `quick-xml 0.30.0`:**
  - Documented Owner (`@Yupkidangju`), Expiry Date (`2026-11-30`), Review Trigger (`eframe 0.31.0`), and target reachability evidence in `DESIGN_DECISIONS.md`.
- **[IMP-F006] Honest Requirements Traceability Calibration:**
  - Updated `spec.md` and `IMPLEMENTATION_SUMMARY.md` with honest `Partial` statuses and 29 passing unit/regression test linkages.

---

## [0.1.0-dev] - 2026-08-19 (Turn 4 / Re-audit #2 Remediation)

### Fixed
- **[SEC-F001] Actual Prompt Re-Hashing, Approved Profile Direct Consumption, and Consume-Once API:**
  - `ApprovedInferenceRequest` directly re-hashes `packet.prompt_context` bytes using SHA-256 in constructor and during integrity verification.
  - Bound the approved `BackendProfile` directly into `ApprovedInferenceRequest` with private fields.
  - Implemented `into_inference_request(self)` consume-once API taking `self` by value, preventing request reuse and parameter drift.
- **[SEC-F010] Exact Byte-Offset Preserving Unicode Assignment Parser:**
  - Replaced lowercase string translation with direct ASCII case-insensitive search (`find_ascii_case_insensitive`) operating on original string byte indices.
  - Eliminated byte offset drift and slicing panics on Unicode characters with casing length expansions (e.g. Turkish `İ`).
- **[IMP-F001] Verbatim Baseline Scope Preservation in Traceability Matrix:**
  - Transcribed verbatim baseline definitions and acceptance criteria from `CODE_MENTAT_SPEC.md` Section 5.1 (`FR-001`~`FR-026`), Section 5.2 (`NFR-001`~`NFR-013`), and Section 5.3 (`CON-001`~`CON-008`) into `spec.md`.

---

## [0.1.0] - 2026-08-18 (Initial Workspace Prototype)
- Initial modular workspace with 10 crates.
- Read-only repository scanner and snapshot digest.
- eframe/egui compact smart pill widget.
- Google Gemini and OpenRouter streaming adapters.
- Persona 3-preset renderer and SQLite AppData storage.
