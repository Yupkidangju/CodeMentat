# Changelog

All notable changes to the **Code Mentat** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
