# D3D 재감사 보고서 (Turn 10 / Re-audit #8)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_9.md`
- 감사 대상 HEAD: `0e3b3e3d4f2a5d6ac87cbec00e95f81746949883`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

Watcher throttle/signature, deterministic sorted snapshot, repository budgets, profile/snapshot persistence, evidence-linked workflows, cloud result reset/간이 Claim normalization, Bearer/extended token redaction, user exclusion UI, adapter guard tests가 추가됐다. 테스트 38개, Strict Clippy, Windows locked release build는 통과한다.

그러나 formatter가 실패했고, user exclusion checkbox를 변경한 뒤 새 packet 재조립이 끝나기 전 기존 pending packet이 승인 가능한 상태로 남는다. 사용자가 UI에서 제외한 파일이 포함된 old packet을 전송할 수 있는 consent race이므로 `SEC-F011 Critical`로 판정한다. Watcher는 1초마다 전체 tree를 UI thread에서 동기 순회하고, session snapshot 복원은 매 open마다 새 repo UUID를 생성해 실제 이전 snapshot을 찾지 못한다. 전체 판정은 `HOLD`다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | **FAIL** — `egress.rs` formatting diff |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 38 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 38 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | High 2건 + unmaintained 2건, DEC-SEC-004 Accepted Risk 적용 |
| `git status --short --branch` | PASS — clean `master...origin/master` |

문서의 formatter PASS 선언은 현재 실행 결과와 불일치한다.

## 3. Finding 재판정표

| Finding | Re-audit #8 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline authority/ID 유지 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major) | exact sizes/theme/global hotkey 불일치 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | local evidence/reset 추가, 실제 conflict/cloud validation 미완료 |
| IMP-F005 | Needs Fix (Major, 부분 개선) | profile/snapshot DB 추가, stable repo/session 복원 실패 |
| IMP-F006 | Needs Fix (Major, 부분 개선) | Partial 정정 증가, 일부 Implemented 과대 표시 |
| DBG-F001 | Verified | UI blocking receive 제거 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | sorted single scan 추가, watcher/evidence completeness 미완료 |
| DBG-F003 | Needs Fix (Major, 부분 개선) | budgets 추가, cancellation/preflight/실규모 benchmark 미완료 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | Needs Fix (Major) | 38 tests지만 fmt 실패 및 wire/app E2E 부족 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | Verified | stream disconnect 처리 |
| DBG-F008 | Needs Fix (Major, 부분 개선) | 1초 throttle 추가, 전체 tree sync UI walk 유지 |
| SEC-F001 | Verified | approved request 유지 |
| SEC-F002 | Needs Fix (Major, 부분 개선) | token/UI exclusion 추가, generic entropy/relevance threshold 미완료 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | Verified | endpoint/key boundary 유지 |
| SEC-F005 | Verified | timeout/cancel/SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 유지 |
| SEC-F010 | Verified | Unicode assignment 유지 |
| SEC-F011 | 신규 **Hold (Critical)** | exclusion 변경 후 old packet 승인 가능 |

## 4. 상세 재감사

### [SEC-F011] 신규 — user exclusion 재조립 중 이전 packet을 승인할 수 있다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: consent TOCTOU, user exclusion
- Severity: Critical
- Status: Hold
- Evidence:
  - consent UI는 `pending_egress_packet`의 included refs를 checkbox로 표시한다.
  - checkbox 변경 후 `user_excluded_files`를 갱신하고 `handle_query()`를 다시 호출해 async reassembly를 시작한다.
  - 이때 기존 `pending_egress_packet`을 즉시 제거하거나 approve 버튼을 disable하지 않는다.
  - `handle_query()`도 old pending packet을 clear하지 않는다.
  - 재조립이 느리면 다음 frame에서 checkbox는 제외 상태지만 approve 버튼은 old packet에 대해 활성화된다.
  - 사용자가 승인하면 old packet의 `included_files`와 prompt가 그대로 ApprovedInferenceRequest로 전송된다.
- Expected: exclusion 변경 즉시 old packet은 invalidated되고 새 exclusion set으로 재조립된 packet만 승인 가능해야 한다.
- Actual: 표시 상태와 승인 payload가 일시적으로 달라진다.
- Impact: 사용자가 명시적으로 제외한 저장소 파일의 외부 전송 가능.
- Suggested Fix: toggle 시 `pending_egress_packet = None`, `consent_rebuilding = true`, approve disabled를 적용한다. generation/request ID를 packet assembly 결과에 넣어 stale result도 거부한다.
- Re-audit Method: delayed assembler fixture에서 exclude toggle 직후 approve 불가, old generation result 무시, final packet file absence를 검증한다.
- Owner: Coder / Security

### [DBG-F008] Re-audit #8 — throttle은 추가됐지만 synchronous full-tree walk가 UI에 남았다

- Pass: Debug
- Pattern: performance regression
- Area: watcher scheduling
- Severity: Major
- Status: Needs Fix
- Modified Files: `watcher.rs`, `app.rs`
- Evidence:
  - watcher는 최소 1초 간격으로 제한된다.
  - file count/total size/latest mtime signature로 추가/삭제를 일부 감지한다.
  - 하지만 `MentatApp::update()`에서 `check_for_changes()`를 직접 호출하며 내부에서 repository 전체를 동기 순회한다.
  - 큰 저장소에서 한 번의 walk가 1초 이상 걸리면 다음 frame에서 즉시 다시 walk할 수 있다.
  - watcher test는 즉시 두 번째 호출이 false인지 확인할 뿐 실제 변경 후 감지, 대형 tree latency를 검증하지 않는다.
- Expected: filesystem walk가 UI thread 밖의 bounded schedule에서 수행돼야 한다.
- Actual: poll 횟수는 줄었지만 한 번의 long task가 UI를 막는다.
- Impact: 주기적인 UI freeze와 높은 filesystem I/O.
- Suggested Fix: watcher를 background task/OS event backend로 이동하고 UI에는 결과 channel만 전달한다.
- Re-audit Method: 100k tree에서 watcher latency와 frame p95, actual add/delete/modify tests.
- Owner: Coder

### [DBG-F002] Re-audit #8 — deterministic single snapshot은 개선됐지만 watcher/evidence 경계가 남았다

- Pass: Debug
- Pattern: `DBG-002`
- Area: snapshot, stale, evidence
- Severity: Major
- Status: Needs Fix
- Evidence:
  - app은 one scan 결과로 snapshot을 생성하며 file records를 path 순으로 정렬해 digest한다.
  - watcher signature는 count/size/latest mtime을 사용한다.
  - 같은 count/size이며 latest mtime이 변하지 않는 변경, mtime rollback, watcher metadata 오류를 놓칠 수 있다.
  - EvidenceRef는 excerpt/path/requested range hash이며 snapshot FileRecord의 full hash와 직접 연결되지 않는다.
  - requested `line_end`가 실제 파일 길이를 넘을 때 actual clamped range를 기록하지 않는다.
- Expected: deterministic snapshot과 모든 evidence의 snapshot/full-file lineage 및 complete stale detection.
- Actual: single-scan/determinism은 개선됐지만 lineage/completeness가 부분적이다.
- Impact: 변경된 증거나 범위를 현재 snapshot evidence로 오인할 수 있다.
- Suggested Fix: watcher/file digest 연계, metadata error 처리, actual line range/full file hash 연결.
- Re-audit Method: same-size edit, mtime rollback, out-of-range evidence, file-hash change fixture.
- Owner: Coder

### [DBG-F003] Re-audit #8 — budget 상수는 추가됐지만 실제 큰 작업 취소가 없다

- Pass: Debug
- Pattern: `DBG-002`
- Area: resource limits
- Severity: Major
- Status: Needs Fix
- Evidence:
  - 100k files, 2GiB accumulated bytes, 10MiB direct read cap과 64KiB hash buffer가 있다.
  - limit은 break로 끝나며 omission/limit status가 없다.
  - oversized single file은 total budget 검사 전에 전체 hash한다.
  - scan cancellation token과 wall-clock budget이 없다.
  - benchmark test는 100 files/500ms이며 baseline 100k/2GiB를 검증하지 않는다.
- Expected: large workload의 cancel, explicit limit result, representative benchmark.
- Actual: memory bound는 개선됐지만 time/I/O/cancel acceptance가 닫히지 않았다.
- Impact: giant file/slow filesystem에서 장시간 점유 가능.
- Suggested Fix: metadata preflight, cancellation, ScanOutcome omissions, 100k/2GiB benchmark profile.
- Re-audit Method: giant single file, 100k files, mid-scan cancel, omission assertions.
- Owner: Coder

### [IMP-F004] Re-audit #8 — local evidence와 cloud reset은 개선됐지만 검증된 cloud contract가 아니다

- Pass: Implementation
- Pattern: `IMP-003`
- Area: evidence-based answers
- Severity: Major
- Status: Needs Fix
- Evidence:
  - local workflows가 EvidenceRef를 생성하고 `/risks`가 추가됐다.
  - 새 query 시작 시 이전 claims/evidence를 clear한다.
  - cloud Completed text의 markdown headings/bullets를 임의로 `Inferred` Claim으로 변환한다.
  - cloud claims의 evidence_ids는 항상 비어 있고 model citation을 snapshot과 검증하지 않는다.
  - `/conflicts`는 문서 헤더를 evidence로 붙일 뿐 문서 주장과 코드 동작을 비교하지 않는다.
- Expected: Claim schema normalization, citation validation, invalid citation 표시, 실제 양면 conflict evidence.
- Actual: 표시 구조는 개선됐지만 evidence validation이 없다.
- Impact: 모델의 근거 없는 bullet이 높은 신뢰도 Inferred claim으로 보일 수 있다.
- Suggested Fix: structured AnswerBundle parser/validator와 invalid citation handling, conflict fixture를 추가한다.
- Re-audit Method: invalid citation, unstructured response, doc-code conflict, claim evidence assertions.
- Owner: Coder

### [IMP-F005] Re-audit #8 — profile 저장은 작동하지만 snapshot 복원 키가 안정적이지 않다

- Pass: Implementation
- Pattern: `IMP-003`
- Area: persistence/restoration
- Severity: Major
- Status: Needs Fix
- Evidence:
  - BackendProfile은 key 제외 상태로 DB round-trip하며 app 시작 시 복원된다.
  - snapshot metadata save/load API와 tests가 있다.
  - `ReadOnlySession::open()`은 매번 새 repo UUID를 생성한다.
  - app은 새 profile을 `save_recent_repo()`한 뒤 새 UUID로 `load_latest_snapshot()`을 호출한다.
  - 이전 실행의 snapshot_history는 이전 repo UUID에 묶여 있어 재실행 시 찾을 수 없다.
  - 로드해도 즉시 새 scan을 시작해 snapshot을 덮는다.
- Expected: stable repository identity와 재실행 후 실제 session/snapshot 복원 소비.
- Actual: storage API는 있지만 cross-run lookup 경로가 무효다.
- Impact: snapshot history가 orphan되고 사용자 session 복원이 작동하지 않는다.
- Suggested Fix: canonical root에서 stable repo ID를 조회/재사용하고 restored snapshot을 indexing/stale UI 상태에 연결한다.
- Re-audit Method: process restart 후 동일 root ID/snapshot/profile/session 복원 E2E.
- Owner: Coder

### [SEC-F002] Re-audit #8 — exclusion UI와 token coverage는 개선됐지만 consent race와 일반 탐지가 남았다

- Pass: Security
- Pattern: `SEC-001`
- Area: least-data egress
- Severity: Major
- Status: Needs Fix
- Evidence:
  - Bearer, Anthropic, HuggingFace, Slack token patterns과 user exclusion API/UI가 추가됐다.
  - exact included file/line preview가 있다.
  - generic high-entropy/new provider credential 탐지는 없다.
  - score 0 file threshold/content retrieval이 없다.
  - exclusion UI의 old-packet race는 별도 Critical `SEC-F011`이다.
- Expected: unknown secret detection, relevance threshold, atomic exclusion consent.
- Actual: 알려진 패턴과 UI surface는 개선됐지만 정책 완결성이 없다.
- Impact: 신규 credential/무관 파일 전송 가능.
- Suggested Fix: entropy detector, score threshold/content retrieval, SEC-F011 atomic reassembly.
- Re-audit Method: random high-entropy, zero-score doc, exclusion race E2E.
- Owner: Coder / Security

### [DBG-F005] Re-audit #8 — 테스트가 늘었지만 품질 게이트와 실제 wire fixture가 부족하다

- Pass: Debug
- Pattern: `TEST-001`
- Area: quality/integration tests
- Severity: Major
- Status: Needs Fix
- Evidence:
  - 38 tests가 통과한다.
  - formatter는 `egress.rs`에서 실패한다.
  - watcher test는 throttle immediate false만 검사하고 변경 감지를 검사하지 않는다.
  - “benchmark”는 100 files/500ms로 baseline 100k/2GiB와 다르다.
  - adapter tests는 missing key, pre-cancelled token, invalid URL만 검사한다.
  - slow response header, cancel during send, split UTF-8/SSE, 401/429/5xx, app restart/exclusion race E2E가 없다.
- Expected: 모든 주장된 failure mode와 품질 명령 재현.
- Actual: unit guard coverage는 증가했지만 production wire/state coverage가 부족하고 fmt gate가 깨졌다.
- Impact: green tests가 주요 runtime 회귀를 놓치고 문서 PASS와 불일치한다.
- Suggested Fix: formatter 수정 및 mock HTTP/app E2E/representative benchmark 추가.
- Re-audit Method: 전체 gate와 failure-specific fixtures 재실행.
- Owner: Coder

### [IMP-F006] Re-audit #8 — Partial 정정은 진전됐지만 과대 상태가 남는다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: traceability status
- Severity: Major
- Status: Needs Fix
- Evidence:
  - 여러 항목이 Partial로 정정됐다.
  - FR-002는 전후 hash/permission/mtime/Git event 통합감사 없이 Implemented다.
  - FR-003은 실제 symlink/junction fixture 없이 Implemented다.
  - FR-009/010/011은 cloud evidence validation이 없는데 Implemented다.
  - FR-013은 selected headers/protocol persistence가 없는데 Implemented다.
  - FR-020은 threshold 설정이 없는데 Implemented다.
  - FR-021은 실제 conflict/결과 schema가 미완료인데 Implemented다.
  - FR-022 watcher completeness/reindex action이 없는데 Implemented다.
  - FR-025 external file export가 없는데 Implemented다.
  - NFR-013 evidence는 34 tests라고 적었지만 실제는 38이다.
  - 문서는 formatter PASS를 주장하지만 현재 FAIL이다.
- Expected: acceptance 전체와 현재 실행 증거가 일치하는 상태 표기.
- Actual: 정정은 계속되지만 일부 완료 주장과 수치가 부패한다.
- Impact: Phase/release 판단 과대평가.
- Suggested Fix: 해당 행을 Partial로 낮추고 자동 생성/검증 가능한 evidence source를 사용한다.
- Re-audit Method: Implemented 행 acceptance 및 명령 output 자동 대조.
- Owner: Architect / Coder

## 5. 상태 집계

- Verified: `IMP-F001`, `DBG-F001`, `DBG-F004`, `DBG-F006`, `DBG-F007`, `SEC-F001`, `SEC-F003`, `SEC-F004`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`
- Accepted Risk: `SEC-F007` (expiry 2026-11-30)
- 미해결: Critical 1, Major 9, Minor 1
- 신규: `SEC-F011`
- 전체 판정: **HOLD**

## 6. 다음 수정 우선순위

1. `SEC-F011`: exclusion toggle 시 old packet invalidation/generation guard.
2. `DBG-F005`: formatter 복구와 race/watcher/wire E2E.
3. `DBG-F008`: watcher background scheduling.
4. `DBG-F002/003`: watcher completeness, evidence lineage, cancel/benchmark.
5. `IMP-F004/005`: cloud validator와 stable repo/session restore.
6. `SEC-F002`, `IMP-F006`, `IMP-F003`, `IMP-F002`.

## 7. Final Decision

**HOLD**

user exclusion consent race가 Critical이며 formatter gate도 깨졌다. 이 두 항목과 남은 Major finding을 해소하기 전 PASS 불가다.

## 8. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_10.md`의 Re-audit #8 결과를 기준으로 수정하세요.
먼저 SEC-F011에서 exclusion toggle 즉시 old pending packet을 무효화하고 generation ID로 stale result를 거부하며 approve를 새 packet까지 disable하세요.
formatter를 복구한 뒤 watcher background scheduling, deterministic evidence/watcher completeness,
stable repo/session restore, cloud AnswerBundle validation, wire/app E2E를 처리하세요.
```
