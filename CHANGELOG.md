# Changelog

All notable changes to the **Code Mentat** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
