# Changelog

All notable changes to the **Code Mentat** project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.1.0-dev] - 2026-08-19 (Re-audit #11 Remediation)

패키지 SemVer는 `0.1.0`이며 이 항목은 그 개발 스냅샷이다.

### Fixed
- **[SEC-F001] Canonical egress seal:** question, validation text digest, snapshot/ref, redaction/token 계수와 provider endpoint/model identity를 단일 digest로 결속하고 tamper matrix를 추가했다.
- **[IMP-F001] Baseline authority:** FR-013/FR-017 원문을 복구하고 provider activation을 `DR-FR-001`로 분리했다.
- **[DBG-F003] Incomplete snapshot:** 취소·한도 누락 scan을 `Incomplete`로 표시해 DB 저장과 로컬/클라우드 분석을 차단하고 repository switch 시 이전 scan을 취소한다.
- **[DBG-F003] Memory budget:** scan 전체 text preview를 8MiB로 제한하고 실제 100k/2GiB profile에 peak working set 계측을 추가했다.
- **[DBG-F002] Watcher:** `notify` OS event와 changed-path full hash로 전환해 16KiB tail edit를 검출하고 반복 전수 I/O를 제거했다.
- **[IMP-F004] Claim invariant:** Observed/Conflict empty evidence, duplicate/missing evidence와 invalid confidence를 Unknown으로 강등한다.
- **[SEC-F004] Gemini redirect:** secret-bearing Gemini client의 redirect를 거부하고 cross-origin key zero-leak fixture를 추가했다.
- **[IMP-F003] Global hotkey:** OS 등록·event thread·unregister lifecycle을 연결하고 창 숨김 대신 안전한 표시·포커스/Tier 1 fallback을 적용했다.
- **[IMP-F006] Audit roadmap:** 독립 감사 HOLD, Partial gate와 유효한 SEC-F007 risk record를 반영했다.
- **검증 후 AI 공급자 활성화:**
  - 제품 코드의 Gemini/OpenAI/OpenRouter/Local 모델 ID 프리셋과 스트리밍 fallback을 제거했다.
  - 공급자 API가 반환한 모델만 동적으로 표시하며 Gemini는 `generateContent` 지원 모델만 허용한다.
  - 외부 모델 ID의 길이·문자·경로 안전성을 검증하고 모델 검색/검증 응답 크기를 제한한다.
  - 선택 모델에 최소 실제 생성 프로브를 실행하고 `Draft → ModelsDiscovered → ModelVerified → Active` 순서를 강제한다.
  - Draft와 Active 프로필을 분리하고 설정 변경·오래된 비동기 결과가 기존 Active를 변조하지 못하게 했다.
  - 내장 로컬은 키 없이 선택 가능하지만 런타임/설치 모델 부재 시 `LOCAL_RUNTIME_UNAVAILABLE`로 실패 폐쇄한다.
- **네이티브 UI 렌더링 및 확장 복구:**
  - OFL NanumGothic 글꼴을 한글 폴백으로 내장해 한국어가 사각형 글리프로 표시되던 문제를 수정했다.
  - 핵심 UI 이모지 아이콘을 텍스트 조작으로 교체해 플랫폼별 글리프 의존성을 제거했다.
  - 설정, 질문, 오류 카드, 증거 인스펙터가 각각 580×300 및 660×480 뷰포트로 실제 확장되도록 상태와 창 크기를 동기화했다.
  - 저장소가 아직 열리지 않은 질문도 먼저 대화 카드를 열어 선행조건 오류를 표시한다.
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
