# D3D 재감사 보고서 (Turn 11 / Re-audit #9)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_10.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `0e3b3e3d4f2a5d6ac87cbec00e95f81746949883`
- 감사 대상: 기준 commit 위의 **미커밋 working tree**
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

Consent generation guard, background watcher, stable repository ID, snapshot/profile persistence, cloud AnswerBundle normalizer, citation downgrade, wire-level adapter fixtures, watcher add/delete/modify tests가 추가됐다. `SEC-F011` old-packet approval race는 해소됐고 formatter·Strict Clippy·50개 tests·Windows locked release build가 통과한다.

그러나 cloud citation validator는 path와 시작 행만 확인하고 current snapshot ID, FileRecord content hash, excerpt, line_end를 검증하지 않는다. 존재하는 파일을 가리키는 가짜 citation이 Observed로 통과할 수 있다. Background watcher도 생성자에서 최초 전체 tree signature를 UI thread에서 동기 계산한다. Repository scan cancellation/대표 규모 benchmark, generic entropy, cloud conflict/evidence contract, exact UI/global shortcut이 남아 있다. 감사 대상이 미커밋 상태이므로 재현 가능한 릴리스 후보도 아니다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 50 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 50 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | High 2건 + unmaintained 2건, DEC-SEC-004 Accepted Risk 적용 |
| `git status --short --branch` | **DIRTY** — 수정 및 신규 파일 다수 |

## 3. Finding 재판정표

| Finding | Re-audit #9 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline authority/ID 유지 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major) | exact sizes/theme/global hotkey 불일치 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | cloud normalizer 추가, citation/conflict 검증 불완전 |
| IMP-F005 | **Verified** | stable repo ID, profile/snapshot DB, recent repo reopen/restore 연결 |
| IMP-F006 | Needs Fix (Major, 부분 개선) | 다수 Partial 정정, 일부 Implemented 과대 표시 |
| DBG-F001 | Verified | UI blocking receive 제거 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | sorted single snapshot/background watcher, evidence lineage 미완료 |
| DBG-F003 | Needs Fix (Major, 부분 개선) | budgets 추가, cancellation/preflight/실규모 benchmark 미완료 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | **Verified** | formatter/Clippy/build 및 wire-level tests 통과 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | Verified | async terminal 처리 유지 |
| DBG-F008 | Needs Fix (Major, 부분 개선) | background walk 추가, constructor initial full-tree sync 유지 |
| SEC-F001 | Verified | approved request 유지 |
| SEC-F002 | Needs Fix (Major, 부분 개선) | user exclusion/token coverage 추가, generic entropy/relevance threshold 미완료 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | Verified | endpoint/key boundary 유지 |
| SEC-F005 | Verified | timeout/cancel/SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 유지 |
| SEC-F010 | Verified | Unicode assignment 유지 |
| SEC-F011 | **Verified** | generation guard, old packet invalidation, approve disable |

## 4. 상세 재감사

### [SEC-F011] Re-audit #9 — exclusion consent race 해소

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: consent generation
- Severity: Critical
- Status: Verified
- Modified Files: `consent.rs`, `app.rs`
- Evidence:
  - exclusion 변경은 generation을 증가시키고 approvable pending packet을 즉시 제거한다.
  - assembly result에 generation이 포함되며 stale generation은 거부된다.
  - rebuilding 동안 `can_approve()`가 false이고 approve button이 disabled다.
  - 새 generation packet만 `take_approved_packet()`으로 소비된다.
  - delayed old packet 거부와 approve 차단 unit tests가 통과한다.
- Expected / Actual: exclusion set과 approved packet의 원자적 일치 — 충족.
- Suggested Fix: 없음.
- Re-audit Method: 현재 generation guard tests 유지.
- Owner: Auditor

### [IMP-F005] Re-audit #9 — stable repo/profile/snapshot 복원 경로 해소

- Pass: Implementation
- Pattern: `IMP-003`
- Area: persistence
- Severity: Major
- Status: Verified
- Modified Files: repository/session, storage, app
- Evidence:
  - canonical root lookup으로 이전 repository UUID를 재사용한다.
  - BackendProfile은 API key 제외 상태로 저장/복원된다.
  - snapshot metadata를 stable repo ID로 저장/로드한다.
  - restored snapshot digest가 새 single scan digest와 같으면 기존 snapshot ID를 재사용한다.
  - recent repository reopen UI가 연결됐다.
  - stable ID/profile/snapshot round-trip tests가 통과한다.
- Expected / Actual: recent repo, settings, snapshot metadata 복원 — 충족.
- Remaining Risk: 전체 대화 history는 active spec에서 Partial 후속 범위로 명시됐다.
- Suggested Fix: 없음.
- Re-audit Method: process restart E2E를 release gate에서 추가 권장.
- Owner: Auditor

### [DBG-F005] Re-audit #9 — 품질 게이트와 wire fixtures 해소

- Pass: Debug
- Pattern: `TEST-001`
- Area: quality/integration tests
- Severity: Major
- Status: Verified
- Modified Files: inference-openai tests, repository tests, analysis tests
- Evidence:
  - formatter, Strict Clippy, 50 tests, locked release build가 통과한다.
  - local loopback wire fixtures가 HTTP error mapping, split SSE chunks, cancel during send를 검증한다.
  - watcher add/delete/same-size modify, consent generation, cloud normalizer, stable repo ID tests가 추가됐다.
- Expected / Actual: 주요 신규 failure modes가 production-adjacent fixture로 고정됨 — 충족.
- Remaining Risk: 실제 GUI process restart E2E와 100k/2GiB benchmark는 개별 finding에 남긴다.
- Suggested Fix: 없음.
- Re-audit Method: current suite와 CI matrix 유지.
- Owner: Auditor

### [IMP-F004] Re-audit #9 — cloud normalizer는 추가됐지만 citation validity가 충분하지 않다

- Pass: Implementation
- Pattern: `IMP-003`
- Area: evidence-based cloud answers
- Severity: Major
- Status: Needs Fix
- Modified Files: `answer_bundle.rs`, `app.rs`, analysis lib
- Evidence:
  - JSON AnswerBundle parsing과 unstructured response의 Unknown 강등이 추가됐다.
  - missing path citation은 `[INVALID_CITATION]`과 Unknown으로 강등된다.
  - 하지만 model이 non-nil snapshot ID를 제공하면 current snapshot ID로 강제하지 않는다.
  - citation validation은 path 존재, line_start, 일부 line_count만 확인한다.
  - EvidenceRef content_hash를 FileRecord full content_hash와 비교하지 않고 excerpt가 실제 파일 내용인지 확인하지 않는다.
  - line_end가 파일 line count를 넘는지 검사하지 않는다.
  - claim이 valid+invalid evidence를 섞으면 `all()` 조건 때문에 Unknown으로 강등되지 않을 수 있다.
  - `/conflicts`는 실제 문서 주장과 코드 동작을 비교하지 않는다.
- Expected: snapshot ID, path, actual range, file hash/excerpt와 claim evidence set이 모두 검증돼야 한다.
- Actual: 명백한 missing-path만 거부하고 그럴듯한 가짜 citation은 통과한다.
- Impact: 존재하는 파일을 악용한 환각 인용이 Observed claim으로 표시될 수 있다.
- Suggested Fix: current snapshot ID 강제, FileRecord hash/actual excerpt/range 검증, any-invalid claim downgrade, conflict fixture를 추가한다.
- Re-audit Method: wrong snapshot/hash/excerpt/line_end, mixed evidence, doc-code conflict tests.
- Owner: Coder

### [DBG-F008] Re-audit #9 — background watcher는 구현됐지만 초기 UI full-tree walk가 남았다

- Pass: Debug
- Pattern: performance regression
- Area: watcher scheduling
- Severity: Major
- Status: Needs Fix
- Modified Files: `watcher.rs`, `app.rs`
- Evidence:
  - periodic tree walk는 background thread로 이동했고 UI는 channel만 poll한다.
  - add/delete/same-size modify tests가 통과한다.
  - 그러나 `RepositoryWatcher::new()`가 `compute_tree_signature()`를 동기 호출한다.
  - app은 `open_repository()` UI 경로에서 watcher를 생성하므로 큰 tree의 첫 전체 walk가 UI를 차단한다.
  - background drop/join도 최대 sleep interval slice 동안 호출 thread를 막을 수 있다.
- Expected: 최초 signature 생성과 stop/join도 UI thread를 장시간 막지 않아야 한다.
- Actual: 반복 walk는 해소됐지만 초기/종료 long task가 남는다.
- Impact: 대형 저장소 열기·전환 시 UI freeze.
- Suggested Fix: initial signature도 worker에서 생성하고 nonblocking stop/join 또는 short-lived cooperative cancellation을 사용한다.
- Re-audit Method: 100k tree watcher construction/open latency와 repo switch p95.
- Owner: Coder

### [DBG-F002] Re-audit #9 — snapshot determinism은 개선됐지만 citation/watcher lineage가 남았다

- Pass: Debug
- Pattern: `DBG-002`
- Area: snapshot/evidence/stale
- Severity: Major
- Status: Needs Fix
- Evidence:
  - one scan, sorted snapshot digest, background tree signature가 구현됐다.
  - watcher digest는 path/size/mtime을 포함하지만 file content hash는 포함하지 않는다.
  - preserved mtime+same-size content change는 감지하지 못한다.
  - metadata/walk error count는 digest에 포함되지만 오류 파일 자체를 식별/보고하지 않는다.
  - EvidenceRef lineage 검증은 `IMP-F004`의 hash/excerpt/range 문제와 연결된다.
- Expected: snapshot/evidence lineage와 complete stale detection.
- Actual: 정상 편집은 잘 감지하지만 adversarial/metadata edge가 부분적이다.
- Impact: 일부 변경 누락 또는 불명확한 stale 원인.
- Suggested Fix: selective content hash 또는 watcher event integration, error detail, citation lineage 강화.
- Re-audit Method: preserved-mtime same-size edit, metadata error, evidence hash mismatch tests.
- Owner: Coder

### [DBG-F003] Re-audit #9 — budget은 있으나 cancellation/대표 규모 증거가 없다

- Pass: Debug
- Pattern: `DBG-002`
- Area: resource budgets
- Severity: Major
- Status: Needs Fix
- Evidence:
  - file count/total bytes/direct read cap과 streaming hash buffer가 있다.
  - cancellation token/wall-clock budget과 explicit omission result가 없다.
  - oversized single file는 total budget 검사 전에 전체 hash한다.
  - benchmark test는 100 files/500ms이며 baseline 100k/2GiB를 검증하지 않는다.
- Expected: representative workload, cancel, limit reason.
- Actual: bounded memory/count는 구현됐지만 bounded time/cancel acceptance는 미완료다.
- Impact: giant file/slow filesystem에서 장시간 점유.
- Suggested Fix: metadata preflight, cancellation, ScanOutcome omissions, representative benchmark profile.
- Re-audit Method: 100k/2GiB, giant file, mid-scan cancel tests.
- Owner: Coder

### [SEC-F002] Re-audit #9 — user exclusion은 연결됐지만 일반 entropy/relevance가 남았다

- Pass: Security
- Pattern: `SEC-001`
- Area: least-data egress
- Severity: Major
- Status: Needs Fix
- Evidence:
  - consent UI toggle이 generation-guarded exclusion assembly에 연결된다.
  - Google/OpenAI/Anthropic/HuggingFace/Slack/GitHub/AWS/JWT/Bearer/PEM/assignment patterns를 처리한다.
  - generic high-entropy/new provider secret 탐지가 없다.
  - path relevance score 0인 문서가 여전히 포함될 수 있고 content retrieval threshold가 없다.
- Expected: unknown secret detector와 최소 relevance threshold.
- Actual: 알려진 패턴과 사용자 통제는 강해졌으나 미등록 형식/무관 문서 가능성이 남는다.
- Impact: 신규 credential 형식 또는 불필요한 파일 전송.
- Suggested Fix: entropy detector, zero-score exclusion/content retrieval threshold 추가.
- Re-audit Method: random high-entropy/new provider token/zero-score file tests.
- Owner: Coder / Security

### [IMP-F006] Re-audit #9 — honest Partial 표기는 개선됐지만 일부 완료 주장이 남는다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: traceability calibration
- Severity: Major
- Status: Needs Fix
- Evidence:
  - FR-002/003/006/008~011/013/015~017/020~026와 주요 NFR가 Partial로 정정됐다.
  - NFR-004는 citation lineage가 불완전한데 Implemented다.
  - NFR-005는 generic entropy가 없는데 Implemented다.
  - NFR-011은 구조화 작업 ID/단계/기간/절대경로 제거 증거가 부족한데 Implemented다.
  - CON-007은 API key가 serializable session String으로 남아 있지만 crash-report/serialization 경계를 Implemented로 본다.
- Expected: acceptance 전체가 닫힌 항목만 Implemented.
- Actual: 대부분 정정됐지만 보안/관찰성 일부가 과대다.
- Impact: Phase 상태 과대평가.
- Suggested Fix: 해당 행을 Partial로 정정하고 finding과 연결한다.
- Re-audit Method: Implemented 행 독립 재현.
- Owner: Architect / Coder

## 5. 상태 집계

- Verified: `IMP-F001`, `IMP-F005`, `DBG-F001`, `DBG-F004`, `DBG-F005`, `DBG-F006`, `DBG-F007`, `SEC-F001`, `SEC-F003`, `SEC-F004`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`, `SEC-F011`
- Accepted Risk: `SEC-F007` (expiry 2026-11-30)
- 미해결: Critical 0, Major 7, Minor 1
- 전체 판정: **HOLD**

## 6. 다음 수정 우선순위

1. `IMP-F004`: strict citation/snapshot/hash/excerpt validation과 실제 conflict.
2. `DBG-F008`: initial watcher construction background화.
3. `DBG-F003`: cancellation/preflight/representative benchmark.
4. `DBG-F002`: preserved-mtime content detection/evidence lineage.
5. `SEC-F002`: generic entropy/relevance threshold.
6. `IMP-F006`, `IMP-F003`, `IMP-F002`.

## 7. Final Decision

**HOLD**

Critical consent race는 해소됐고 품질 게이트가 통과하지만, evidence trust boundary와 대형 저장소/initial watcher 성능 등 7개 Major finding이 남아 있다. 또한 현재 감사 대상은 미커밋 working tree이므로 수정 후 commit 기준 재감사가 필요하다.

## 8. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_11.md`의 Re-audit #9 결과를 기준으로 수정하세요.
먼저 IMP-F004의 current snapshot/hash/excerpt/range citation 검증과 실제 conflict fixture를 구현하세요.
그다음 watcher initial walk를 background화하고 repository cancellation/representative benchmark,
generic entropy/relevance threshold를 처리하세요. traceability 상태를 동기화한 뒤 commit 기준 전체 게이트를 재실행하세요.
```
