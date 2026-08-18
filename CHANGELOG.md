# Changelog

All notable changes to the **Code Mentat** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
