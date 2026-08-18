# D3D 재감사 보고서 (Turn 9 / Re-audit #7)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_8.md`
- 감사 대상 HEAD: `6b14a046833964b88e964e456b8a94dd0e2d813a`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

Single-scan snapshot, repository file/count/byte budgets, recursive watcher, backend profile/snapshot metadata persistence, evidence-linked local workflows, `/risks`, Bearer redaction, user exclusion API, stream disconnect terminal 처리, adapter missing-key/cancel tests가 추가됐다. 포맷, Strict Clippy, 34개 테스트, Windows locked release build는 통과한다.

그러나 watcher는 UI `update()`마다 재귀 순회하며 streaming/pending task 중 16ms repaint와 결합되어 초당 최대 약 60회 × 2,000개 entry metadata 검사를 수행할 수 있다. watcher는 삭제와 mtime 역행, 2,000개 이후/깊이 10 이후 변경을 놓친다. Cloud 답변의 Claim/Evidence 정규화, session snapshot 복원, generic entropy 및 UI user exclusion, 실제 adapter SSE fixture, UI 디자인 계약도 남아 있다. 전체 판정은 `HOLD`다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 34 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 34 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | High 2건 + unmaintained 2건, DEC-SEC-004 Accepted Risk 적용 |
| `git status --short --branch` | PASS — clean `master...origin/master` |

## 3. Finding 재판정표

| Finding | Re-audit #7 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline authority/ID 유지 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major) | resize 구현, exact sizes/theme/global hotkey 불일치 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | local evidence 추가, 실제 conflict/cloud normalization 미완료 |
| IMP-F005 | Needs Fix (Major, 부분 개선) | profile/snapshot 저장 추가, session snapshot 복원 미사용 |
| IMP-F006 | Needs Fix (Major, 부분 개선) | Partial 정정 증가, 일부 Implemented 과대 표시 |
| DBG-F001 | Verified | UI blocking receive 제거 유지 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | single scan 적용, watcher completeness/digest determinism 미완료 |
| DBG-F003 | Needs Fix (Major, 부분 개선) | count/byte/file cap 추가, cancellation/benchmark/oversize preflight 미완료 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | Needs Fix (Major, 부분 개선) | 34 tests, real slow server/SSE split/app integration 부족 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | **Verified** | stream disconnect 포함 terminal state 처리 |
| DBG-F008 | 신규 Needs Fix (Major) | recursive watcher를 UI frame마다 실행 |
| SEC-F001 | Verified | approved request 유지 |
| SEC-F002 | Needs Fix (Major, 부분 개선) | Bearer/user-exclusion API 추가, generic entropy/UI exclude 미완료 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | Verified | parsed URL/header/redacted Debug 유지 |
| SEC-F005 | Verified | timeout/pre-response cancel/byte SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review 조건 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 유지 |
| SEC-F010 | Verified | Unicode assignment 유지 |

## 4. 상세 재감사

### [DBG-F007] Re-audit #7 — stream disconnect terminal 처리 해소

- Pass: Debug
- Pattern: `DBG-001`, `TEST-001`
- Area: async state machine
- Severity: Minor
- Status: Verified
- Modified Files: `crates/mentat-app/src/app.rs`
- Evidence:
  - stream receiver가 `Empty`와 `Disconnected`를 구분한다.
  - disconnect 시 status를 갱신하고 `is_streaming`, cancellation token, receiver를 terminal cleanup한다.
- Expected / Actual: 최종 이벤트 없는 sender drop도 terminal state — 일치.
- Suggested Fix: 없음.
- Re-audit Method: sender-drop fixture 추가 권장.
- Owner: Auditor

### [DBG-F002] Re-audit #7 — single scan은 해소됐지만 watcher/snapshot completeness가 남았다

- Pass: Debug
- Pattern: `DBG-002`
- Area: snapshot consistency, stale detection
- Severity: Major
- Status: Needs Fix
- Modified Files: `session.rs`, `watcher.rs`, `app.rs`
- Evidence:
  - app은 한 번의 `scan_files()` 결과로 `create_snapshot_from_files()`를 호출한다.
  - snapshot history가 DB에 저장된다.
  - watcher는 recursive이지만 max depth 10, max 2,000 entries에서 중단한다.
  - watcher는 `current_mtime > last_known_mtime`만 비교해 삭제, mtime 역행, 동일 mtime 내용 변경을 놓친다.
  - snapshot digest는 walker가 반환한 file 순서를 정렬하지 않고 hash한다.
  - EvidenceRef는 excerpt 범위 hash이며 실제 clamped `line_end`를 기록하지 않는다.
- Expected: 같은 파일 집합의 결정적 digest와 모든 변경에 대한 STALE 전이.
- Actual: double scan은 제거됐지만 큰/깊은 저장소와 삭제/순서 경계가 미완료다.
- Impact: 변경 누락 또는 동일 tree의 비결정적 digest 가능.
- Suggested Fix: sorted file records, tree signature/file count 비교, deletion detection, full-file hash 연결, 실제 line range 기록.
- Re-audit Method: 2,001파일, depth 11, delete, mtime rollback, repeat snapshot fixture.
- Owner: Coder

### [DBG-F003] Re-audit #7 — repository budgets는 추가됐지만 종료/취소 증거가 부족하다

- Pass: Debug
- Pattern: `DBG-002`
- Area: resource budgets
- Severity: Major
- Status: Needs Fix
- Modified Files: `session.rs`, `scanner.rs`
- Evidence:
  - max 100,000 records, 2GiB accumulated bytes, 10MiB direct read cap이 있다.
  - scanner hash는 64KiB chunk와 2MiB sample을 사용한다.
  - limit 도달 시 단순 break하며 omission reason/limit status를 반환하지 않는다.
  - 매우 큰 단일 파일은 accumulated budget 검사 전에 전체 hash된다.
  - cancellation token과 wall-clock budget이 없다.
  - baseline 100k/2GiB benchmark 결과가 없다.
- Expected: bounded work, cancel, 명시적 omission, 결정적 benchmark.
- Actual: memory/count cap은 진전됐으나 작업 시간과 사용자 취소가 닫히지 않았다.
- Impact: giant file 또는 느린 filesystem에서 장시간 점유 가능.
- Suggested Fix: metadata preflight, ScanBudget result/omission, cancellation, benchmark를 추가한다.
- Re-audit Method: >2GiB single file, 100k files, cancel mid-scan, limit reason fixture.
- Owner: Coder

### [DBG-F008] 신규 — recursive watcher가 UI frame마다 filesystem을 순회한다

- Pass: Debug
- Pattern: `DBG-001`, performance regression
- Area: watcher scheduling
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `MentatApp::update()`가 호출될 때마다 `watcher.check_for_changes()`를 실행한다.
  - pending task/streaming 중 `request_repaint_after(16ms)`로 약 60fps update가 예약된다.
  - watcher 한 번은 최대 2,000 entry metadata와 depth 10 walk를 수행한다.
  - 최악에는 streaming 중 초당 약 120,000회 metadata 검사와 반복 디렉터리 순회가 발생한다.
- Expected: watcher I/O가 UI frame rate와 분리된 제한 주기 또는 OS event backend에서 동작해야 한다.
- Actual: frame loop가 filesystem polling rate를 결정한다.
- Impact: UI 지연, 높은 CPU/디스크 사용, 대형 저장소 성능 악화.
- Suggested Fix: `Instant` 기반 500ms~2s throttle, background watcher task, 또는 OS notify event를 사용한다.
- Re-audit Method: 2,000파일 저장소에서 idle/streaming stat-call rate와 frame p95를 측정한다.
- Owner: Coder

### [IMP-F004] Re-audit #7 — local evidence는 개선됐지만 conflict/cloud contract는 미완료

- Pass: Implementation
- Pattern: `IMP-003`
- Area: evidence workflows
- Severity: Major
- Status: Needs Fix
- Modified Files: `semantic_kernel.rs`, `app.rs`
- Evidence:
  - `/onboard`, `/structure`, `/where`, `/risks` claims에 EvidenceRef가 연결된다.
  - `/conflicts`는 문서 헤더 evidence를 만들지만 실제 문서 주장과 코드 동작을 비교하지 않고 “모니터링 중”이라고만 보고한다.
  - cloud streaming 완료는 raw `full_text`를 `answer_preview`에 저장하고 Claim/Evidence validation을 거치지 않는다.
  - 이전 local claims/evidence가 cloud 질문 시작 시 초기화되지 않는다.
- Expected: local conflict는 양쪽 evidence를 비교하고 cloud 답변은 AnswerBundle로 검증하며 현재 질문의 evidence만 표시해야 한다.
- Actual: local evidence surface는 개선됐지만 핵심 자연어/충돌 계약이 분리돼 있다.
- Impact: 새 cloud 답변 옆에 이전 질문 evidence가 남거나 모델 주장이 검증된 것처럼 보일 수 있다.
- Suggested Fix: cloud normalizer/validator와 request-scoped result reset을 구현하고 conflict fixture를 추가한다.
- Re-audit Method: invalid citation, previous-claim reset, doc-vs-code conflict fixture.
- Owner: Coder

### [IMP-F005] Re-audit #7 — profile persistence는 해소됐지만 session 복원은 부분 구현이다

- Pass: Implementation
- Pattern: `IMP-003`
- Area: persistence
- Severity: Major
- Status: Needs Fix
- Modified Files: storage crate, app
- Evidence:
  - BackendProfile은 API key를 제외하고 SQLite에 저장/로드되며 app 시작 시 복원된다.
  - snapshot metadata table과 save/load API가 있고 scan 완료 시 저장된다.
  - app은 `load_latest_snapshot()`을 호출하지 않고 repository session/answer history를 복원하지 않는다.
  - recent repository 목록은 읽지만 UI에서 최근 항목을 선택해 reopen하는 흐름이 없다.
- Expected: 최근 repo/session/settings/index metadata가 재실행 후 사용자 흐름으로 복원된다.
- Actual: profile과 metadata 저장은 구현됐으나 실제 session 복원 소비 경로가 없다.
- Impact: 재실행 후 이전 분석 상태/스냅샷을 사용할 수 없다.
- Suggested Fix: recent repo picker와 latest snapshot/session restoration state를 app에 연결한다.
- Re-audit Method: 저장→프로세스 재시작→repo/session/profile 복원 E2E.
- Owner: Coder

### [SEC-F002] Re-audit #7 — Bearer와 exclusion API는 추가됐지만 UI 정책이 연결되지 않았다

- Pass: Security
- Pattern: `SEC-001`
- Area: least-data egress
- Severity: Major
- Status: Needs Fix
- Modified Files: `egress.rs`
- Evidence:
  - generic `Bearer <token>` redaction test가 추가됐다.
  - `assemble_packet_with_user_exclusions()` API가 있다.
  - app은 기본 `assemble_packet()`만 호출하고 사용자가 preview에서 파일을 제외할 컨트롤을 제공하지 않는다.
  - generic entropy/new provider token 탐지는 없다.
  - path relevance score가 0인 문서도 포함될 수 있다.
- Expected: user exclusion이 실제 consent UI에 연결되고 unknown secrets/zero relevance가 전송되지 않아야 한다.
- Actual: library capability는 생겼지만 제품 호출 경로에 연결되지 않았다.
- Impact: 사용자가 전송 직전 원치 않는 파일을 제거할 수 없다.
- Suggested Fix: consent UI file toggles를 exclusion API에 연결하고 entropy/score threshold를 추가한다.
- Re-audit Method: UI에서 제외→재조립→approved packet file list 불포함 E2E.
- Owner: Coder / Security

### [DBG-F005] Re-audit #7 — adapter tests는 증가했지만 wire-level 실패모드는 미검증

- Pass: Debug
- Pattern: `TEST-001`
- Area: integration tests
- Severity: Major
- Status: Needs Fix
- Modified Files: inference-openai tests, storage tests
- Evidence:
  - missing key fail-closed와 pre-cancelled token이 검증된다.
  - storage profile/snapshot round-trip 테스트가 추가됐다.
  - adapter 테스트는 실제 지연 HTTP server, response-header 대기 중 cancel, split UTF-8, split SSE line, malformed JSON, 401/429/5xx mapping을 실행하지 않는다.
  - app 테스트는 실제 channel wake/error, stream disconnect, persistence restart를 실행하지 않는다.
- Expected: 구현한 실패 경계를 wire/state level fixture로 고정한다.
- Actual: constructor/early guard 테스트가 대부분이다.
- Impact: network framing과 lifecycle 회귀를 green suite가 놓칠 수 있다.
- Suggested Fix: local mock HTTP server와 app state reducer/integration fixture를 추가한다.
- Re-audit Method: slow header/cancel, chunk split, error status, sender drop, restart E2E.
- Owner: Coder

### [IMP-F006] Re-audit #7 — 상태 정정은 진전됐지만 일부 완료 주장이 남는다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: traceability calibration
- Severity: Major
- Status: Needs Fix
- Evidence:
  - FR-006/008/015/016/023/024/026, NFR-002/003/009/010/012가 Partial로 정정됐다.
  - FR-002는 전후 파일/hash/permission/mtime/Git event 통합감사 없이 Implemented다.
  - FR-003은 실제 symlink/junction fixture 없이 Implemented다.
  - FR-009/010은 cloud AnswerBundle normalization이 없는데 Implemented다.
  - FR-011은 EvidenceRef가 snapshot file hash/STALE 해석을 완전히 보장하지 않는데 Implemented다.
  - FR-013은 저장 profile에 선택 header/protocol이 없는데 Implemented다.
  - FR-017은 fake/OpenAI 동일 conformance suite가 없는데 Implemented다.
  - FR-020은 threshold 설정이 없는데 Implemented다.
  - FR-021은 local workflow 결과 schema/실제 conflict가 미완료인데 Implemented다.
  - FR-022 watcher completeness/reindex가 없는데 Implemented다.
  - FR-025는 external file export 없이 clipboard만 있어 Implemented다.
  - CON-008은 capability detection이 아닌 health check만 있어 Implemented다.
- Expected: baseline acceptance 전체가 닫힌 항목만 Implemented.
- Actual: 문서 정합성은 개선됐지만 다수 partial surface가 남는다.
- Impact: release/Phase 상태 과대평가.
- Suggested Fix: 해당 행을 Partial로 낮추고 정확한 finding/test와 연결한다.
- Re-audit Method: Implemented 행 acceptance 독립 재현.
- Owner: Architect / Coder

## 5. 상태 집계

- Verified: `IMP-F001`, `DBG-F001`, `DBG-F004`, `DBG-F006`, `DBG-F007`, `SEC-F001`, `SEC-F003`, `SEC-F004`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`
- Accepted Risk: `SEC-F007` (expiry 2026-11-30)
- 미해결: Critical 0, Major 9, Minor 1
- 신규: `DBG-F008`
- 전체 판정: **HOLD**

## 6. 다음 수정 우선순위

1. `DBG-F008`: watcher throttling/background scheduling.
2. `IMP-F006`: remaining partial items status correction.
3. `DBG-F002/003`: watcher completeness, deterministic snapshot, cancellation/benchmark.
4. `IMP-F004/005`: cloud evidence normalization, session restoration.
5. `SEC-F002`: UI exclusion/entropy/relevance threshold.
6. `DBG-F005`, `IMP-F003`, `IMP-F002`.

## 7. Final Decision

**HOLD**

기술적 경계는 계속 개선되고 있으나 watcher 성능 회귀와 9개 Major finding이 남아 있다. 공급망 High 2건은 Windows current target에 한해 조건부 Accepted Risk이며 Linux shipment 전 재감사가 필요하다.

## 8. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_9.md`의 Re-audit #7 결과를 기준으로 수정하세요.
먼저 DBG-F008의 watcher polling을 UI frame rate와 분리하세요.
IMP-F006의 남은 과대 상태를 Partial로 정정하고, DBG-F002/003의 watcher completeness와
repository cancellation/benchmark를 처리하세요. 이후 cloud evidence/session restore, UI exclusion,
wire-level adapter/app integration tests를 보강하고 전체 품질 게이트를 재실행하세요.
```
