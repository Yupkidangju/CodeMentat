# D3D 재감사 보고서 (Turn 12 / Re-audit #10)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_11.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `0e3b3e3d4f2a5d6ac87cbec00e95f81746949883`
- 감사 대상: 기준 commit 위의 **미커밋 working tree**
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

이전 감사 이후 cloud citation의 snapshot/hash/excerpt/range 검증, mixed-invalid 강등, 실제 문서-언어/경로 충돌 fixture, watcher 최초 walk의 background 이동, scan cancellation/preflight/omission API, 고엔트로피 마스킹, 최소 relevance threshold, 명세 상태 보정이 추가됐다. 이에 따라 `DBG-F008`과 `IMP-F006`을 Verified로 전환한다. Formatter, strict Clippy, 58개 tests, Windows locked release build도 통과한다.

다만 검증기 앞단의 cloud 요청 계약에는 AnswerBundle JSON 스키마, 현재 snapshot ID, 파일 content hash가 제공되지 않는다. 따라서 모델은 정상 인용을 구성할 수 없고 앱은 `from_model_text()`의 preview-only 경로를 사용한다. 저장소 scan cancellation API는 앱에서 사용되지 않으며 omission 결과도 버려진다. Watcher는 content hash가 아닌 path/size/mtime만 서명하고, outbound user question은 민감정보 필터를 거치지 않은 채 prompt context와 별도 user message에 중복 포함된다. UI 명세와 실제 크기·색상·단축키도 여전히 어긋난다.

감사 대상은 아직 미커밋 상태이므로 재현 가능한 릴리스 후보로 볼 수 없다.

## 2. 4단계 감사 결과

### 2.1 정합성

- `spec.md`의 미완료 baseline 항목은 `Partial`로 보정되어 이전 `IMP-F006` 과대 표기가 해소됐다.
- `designs.md`의 Tier 크기·색상 토큰과 실제 UI 구현은 일치하지 않는다.
- README가 약속한 글로벌 단축키는 dependency만 있고 등록·처리 코드가 없다.
- 활성 문서 버전 `0.1.0-dev`와 Cargo workspace 버전 `0.1.0`이 다르다.

### 2.2 위험요소

- Cloud evidence validator 자체는 fail-closed로 강화됐으나 모델이 필요한 citation metadata를 받을 수 없어 정상 evidence 흐름이 닫히지 않는다.
- 사용자 질문에 포함된 credential은 egress secret scanner를 통과하지 않는다.
- scan cancellation과 omission이 라이브 앱에서 사용자 제어·표시로 연결되지 않는다.
- `cargo audit`의 High 2건은 `DEC-SEC-004`의 owner/expiry/review trigger가 있는 Accepted Risk다.

### 2.3 아키텍처

- Consent generation guard, sealed request, background watcher, stable repository identity, persistence 경계는 유지된다.
- Cloud output trust boundary는 parser/validator가 생겼지만 egress prompt contract와 연결되지 않아 end-to-end 계약이 끊겨 있다.
- Repository scan의 richer `ScanOutcome`이 기존 `RepositoryReader::scan_files() -> Vec<FileRecord>` 경계에서 소실된다.
- Polling watcher는 UI thread를 막지 않지만 1초마다 전체 tree metadata walk를 수행하고 content-preserving metadata edge를 식별하지 못한다.

### 2.4 로드맵

1. Cloud request에 명시적 AnswerBundle schema, current snapshot ID, 파일별 path/range/content hash를 제공하고 실제 앱 경로 E2E를 고정한다.
2. App scan 경로를 cancellable `ScanOutcome`으로 연결해 취소, omission, 제한 사유를 UI에 표시한다.
3. Outbound question을 포함한 최종 요청 전체에 secret filter를 적용하고 중복 전송을 제거한다.
4. Watcher의 preserved-mtime same-size 변경과 대형 tree 지속 부하를 보완한다.
5. UI 명세 또는 구현을 하나의 기준으로 동기화하고 글로벌 단축키를 실제 등록한다.

## 3. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 58 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 58 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `git diff --check` | PASS — whitespace error 없음, LF→CRLF 경고 존재 |
| `cargo audit --file Cargo.lock` | FAIL — `quick-xml 0.30.0` High 2건, unmaintained 2건 |
| `git status --short --branch` | **DIRTY** — 수정 및 신규 파일 다수 |

`cargo audit` 상세:

- `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`: `quick-xml 0.30.0`, CVSS 7.5 High.
- `RUSTSEC-2024-0436`: `paste 1.0.15` unmaintained.
- `RUSTSEC-2026-0192`: `ttf-parser 0.25.1` unmaintained.
- `DEC-SEC-004`: Windows 대상에서 해당 XML 경로가 조건부 비도달이라는 근거, owner `@Yupkidangju`, expiry `2026-11-30`, upstream update trigger가 기록되어 있다.

## 4. Finding 재판정표

| Finding | Re-audit #10 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline authority/ID 유지 |
| IMP-F002 | Needs Fix (Minor) | 활성 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major) | Tier 크기·theme token·global hotkey 불일치 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | strict validator는 추가됐으나 cloud input/output 계약과 앱 full-content 검증 연결 미완료 |
| IMP-F005 | Verified | stable repo ID, profile/snapshot/recent repo 복원 유지 |
| IMP-F006 | **Verified** | 미충족 baseline을 Partial로 보정하고 테스트 수를 실행 증거로 전환 |
| DBG-F001 | Verified | UI blocking receive 제거 유지 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | deterministic snapshot은 유지되나 watcher content lineage 미완료 |
| DBG-F003 | Needs Fix (Major, 부분 개선) | cancellable API/preflight 추가, 앱 연결·대표 규모 증거 미완료 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | Verified | formatter/Clippy/build/wire tests 통과 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | Verified | async terminal 처리 유지 |
| DBG-F008 | **Verified** | constructor와 UI poll에서 tree walk 제거, worker 최초 signature test 통과 |
| SEC-F001 | Verified | approved request binding/consume-once 유지 |
| SEC-F002 | Needs Fix (Major, 부분 개선) | entropy/relevance 추가, outbound user question 필터 누락 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | Verified | endpoint/key boundary 유지 |
| SEC-F005 | Verified | timeout/cancel/SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review trigger 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 처리 유지 |
| SEC-F010 | Verified | Unicode assignment 처리 유지 |
| SEC-F011 | Verified | generation guard와 stale packet 거부 유지 |

## 5. 상세 재감사

### [IMP-F004] Strict citation validator가 실제 cloud 계약과 연결되지 않았다

- Pass: Implementation / Architecture
- Pattern: `IMP-003`, evidence trust boundary
- Area: cloud AnswerBundle
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `answer_bundle.rs`는 current snapshot, file content hash, range, excerpt, mixed-invalid evidence를 검사하고 실패 시 claim을 `Unknown`으로 강등한다.
  - 그러나 `egress.rs:159` system contract에는 AnswerBundle JSON schema와 `PROPOSED/UNKNOWN` 계약이 없고, `egress.rs:713` 파일 context에는 path와 본문만 있으며 snapshot ID와 content hash가 없다.
  - 모델은 validator가 요구하는 `EvidenceRef.content_hash`와 snapshot ID를 알 수 없다.
  - 앱은 `app.rs:618`에서 `from_model_text()`를 호출하며, 이 경로는 `answer_bundle.rs:21`의 빈 `file_texts`를 사용해 preview 범위만 검증한다.
  - `/conflicts` fixture는 실제 문서 언어·경로 불일치를 검출하지만 단일 키워드 휴리스틱이며 cloud contract 단절을 해소하지 않는다.
- Expected: 모델이 현재 snapshot과 허용 파일별 citation metadata를 받아 정해진 JSON schema로 응답하고, 앱이 그 응답을 실제 파일 범위와 검증한다.
- Actual: validator는 강화됐으나 정상 모델이 유효 citation을 생성할 입력·출력 계약이 없다.
- Impact: 정상 cloud 응답도 대부분 `UNSTRUCTURED_RESPONSE` 또는 `INVALID_CITATION`으로 강등되어 근거 기반 자연어 질의의 핵심 수용 기준을 충족하지 못한다.
- Suggested Fix: approved packet에 snapshot ID, 파일별 content hash, 허용 line range와 JSON schema를 포함하고 structured output을 요구한다. 앱은 included files의 실제 내용을 읽어 `from_model_text_with_contents()`로 검증한다.
- Re-audit Method: 실제 adapter loopback fixture에서 valid JSON, wrong snapshot/hash/excerpt/range, mixed evidence를 end-to-end 검증한다.
- Owner: Coder

### [SEC-F002] 파일 필터는 강화됐지만 사용자 질문은 최종 egress 필터를 우회한다

- Pass: Security
- Pattern: `SEC-001`, least-data egress
- Area: outbound request
- Severity: Major
- Status: Needs Fix
- Evidence:
  - 알려진 credential pattern, assignment, PEM, generic high-entropy token 마스킹이 구현됐다.
  - relevance score가 3 미만인 파일은 content retrieval 전에 제외되고 user exclusion generation guard도 유지된다.
  - 반면 `egress.rs:729`는 `user_question`을 그대로 `prompt_context`에 추가한다.
  - `ApprovedInferenceRequest::into_inference_request()`는 동일 질문을 별도 `user_question` 필드로도 전달해 adapter 요청에 중복 포함한다.
  - 사용자 질문에 credential이 들어간 경우 어느 경로도 `scan_and_redact_secrets()`를 거치지 않는다.
- Expected: 실제 외부로 나가는 모든 텍스트가 동일한 secret policy를 통과하고 consent preview가 최종 payload와 일치해야 한다.
- Actual: repository file content만 필터링되고 user question은 raw 상태로 두 번 포함될 수 있다.
- Impact: 실수로 질문에 붙여 넣은 API key, JWT, private token이 외부 공급자에 전송될 수 있다.
- Suggested Fix: final outbound message 전체에 필터를 적용하고 user question을 단일 canonical 위치에만 포함한다. 질문 redaction/receipt 일치 회귀 테스트를 추가한다.
- Re-audit Method: 질문에 known secret, unknown high-entropy token, Unicode 인접 token을 넣은 wire-level zero-leak test.
- Owner: Coder / Security

### [DBG-F003] Cancellable scan API가 앱 경로에서 사용되지 않는다

- Pass: Debug / Performance
- Pattern: `DBG-002`
- Area: repository scan
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `scan_files_with_limits()`와 `ScanOutcome`이 cancellation, giant-file preflight, omission reason을 제공한다.
  - 기본 `scan_files()`는 내부에서 새 `CancellationToken`을 생성하고 `ScanOutcome.files`만 반환해 취소 handle과 omissions를 버린다.
  - 앱 `app.rs:223`은 이 기본 `scan_files()`를 호출하므로 실행 중 scan을 취소할 UI/API가 없다.
  - `test_dbg_f003_representative_budget_profile`은 80개 파일과 `max_files=25` 축소 fixture이며 baseline의 100,000파일/2GiB 메모리·시간 profile은 측정하지 않는다.
- Expected: 실제 앱 indexing 작업을 사용자가 취소할 수 있고 한도 초과·누락 사유를 확인할 수 있으며 대표 규모의 자원 상한 증거가 있어야 한다.
- Actual: low-level API/test는 생겼지만 product path와 representative acceptance evidence가 없다.
- Impact: 대형/느린 저장소에서 작업 종료와 누락 설명을 사용자가 제어할 수 없다.
- Suggested Fix: app이 보유하는 scan cancellation token과 `Receiver<Result<ScanOutcome, _>>`를 연결하고 취소/omission UI를 제공한다. 별도 ignored/benchmark profile로 100k/2GiB 조건을 계측한다.
- Re-audit Method: app-level mid-scan cancel, omitted-file reason UI, representative benchmark artifact.
- Owner: Coder

### [DBG-F002] Watcher signature가 content-preserving metadata 변경을 놓친다

- Pass: Debug / Performance
- Pattern: `DBG-002`
- Area: stale detection
- Severity: Major
- Status: Needs Fix
- Evidence:
  - repository snapshot은 sorted path와 full content hash로 deterministic하게 생성된다.
  - watcher worker signature는 `watcher.rs:181-185`에서 path, size, mtime만 해시하고 파일 content hash는 포함하지 않는다.
  - 주석은 same-size edit와 mtime rollback을 감지한다고 표현하지만, 동일 크기 내용을 쓴 뒤 mtime을 원래 값으로 복원하면 signature가 동일하다.
  - 1초마다 전체 tree metadata walk를 수행하므로 100k tree에서 지속 I/O 비용도 측정되지 않았다.
- Expected: 기존 답변의 snapshot이 변한 모든 의미 있는 파일 변경에서 `STALE`로 전환되거나, 명시된 제한과 보완 재검증 경로가 있어야 한다.
- Actual: 일반 편집은 감지하지만 preserved-mtime same-size 변경은 누락할 수 있다.
- Impact: 변경된 코드를 기존 evidence가 최신인 것처럼 표시할 수 있다.
- Suggested Fix: OS watcher event와 selective content hash를 조합하거나, metadata가 같아도 periodic verified rehash를 수행한다. preserved-mtime fixture와 100k-tree I/O profile을 추가한다.
- Re-audit Method: same-size write 후 mtime 복원, rapid replace, metadata error, large-tree sustained polling test.
- Owner: Coder

### [IMP-F003] UI 구현이 디자인 문서와 README 계약을 따르지 않는다

- Pass: Implementation / Consistency
- Pattern: `IMP-003`
- Area: UI/UX
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `designs.md`는 Tier 1/2/3을 `580x48`, `580x280`, `640x460`으로 정의한다.
  - 실제 `app.rs:162-164`는 `580x52`, `580x300`, `660x480`이고 초기 `main.rs:18`은 `540x52`다.
  - 디자인의 `BG_BASE #0F172A`, `STATUS_CONFLICT #EF4444`와 실제 theme의 `(18,21,26)`, amber `(245,158,11)`가 다르다.
  - README는 `Alt+Space`, `Ctrl+Alt+M`, `Ctrl+K`, `Ctrl+P`를 약속하지만 `global-hotkey` dependency 외에 등록/처리 코드가 없다.
- Expected: 구현과 승인된 design/README interaction contract가 일치한다.
- Actual: layout, token, shortcuts가 서로 다른 기준을 사용한다.
- Impact: 구현 기준 문서로 재현할 수 없고 사용자 약속이 동작하지 않는다.
- Suggested Fix: 문서가 의도한 값이면 구현을 맞추고, 구현이 최종 결정이면 먼저 designs/README를 변경 근거와 함께 갱신한다. 글로벌 단축키는 실제 등록·해제 lifecycle test를 추가한다.
- Re-audit Method: viewport/theme token unit test, Windows global hotkey smoke test, 문서 대조.
- Owner: Coder / Designer

### [IMP-F002] 활성 문서와 Cargo 버전이 다르다

- Pass: Implementation / Consistency
- Pattern: `IMP-004`
- Area: version metadata
- Severity: Minor
- Status: Needs Fix
- Evidence: `spec.md`와 `IMPLEMENTATION_SUMMARY.md`는 `0.1.0-dev`, workspace `Cargo.toml`은 `0.1.0`이다.
- Expected: 현재 개발/릴리스 상태를 하나의 버전 정책으로 설명한다.
- Actual: pre-release 표기와 package version이 다르며 대응 관계가 명시되지 않았다.
- Impact: 산출물·문서·CHANGELOG 추적 혼선.
- Suggested Fix: Cargo SemVer prerelease를 사용하거나 문서에 package version과 문서 상태의 차이를 명시한다.
- Re-audit Method: workspace/package/doc/changelog version matrix 대조.
- Owner: Coder / Release

## 6. 이번에 해소된 Finding

### [DBG-F008] Initial watcher UI blocking — Verified

- `RepositoryWatcher::new()`는 tree walk를 하지 않는다.
- 최초 signature와 이후 full walk는 worker에서 계산한다.
- UI `poll_changes()`는 channel `try_recv()`만 수행한다.
- constructor/background nonblocking 및 add/delete/modify tests가 통과한다.
- Worker detach와 지속 full-tree polling 비용은 `DBG-F002`의 remaining risk로 추적한다.

### [IMP-F006] Traceability 과대 표기 — Verified

- 이전에 과대 표기됐던 NFR-004, NFR-005, NFR-011, CON-007을 `Partial`로 보정했다.
- FR/NFR/CON 표는 실제 코드와 테스트 근거에 맞춰 완료/부분 상태를 구분한다.
- 테스트 개수는 문서에 고정 숫자로 진실화하지 않고 `cargo test --workspace --locked` 실행 증거를 따르도록 정정했다.

## 7. 상태 집계

- Verified: 18건
  - `IMP-F001`, `IMP-F005`, `IMP-F006`
  - `DBG-F001`, `DBG-F004`, `DBG-F005`, `DBG-F006`, `DBG-F007`, `DBG-F008`
  - `SEC-F001`, `SEC-F003`, `SEC-F004`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`, `SEC-F011`
- Accepted Risk: 1건 — `SEC-F007` (expiry 2026-11-30)
- 미해결: Critical 0, Major 5, Minor 1
- 전체 판정: **HOLD**

## 8. Final Decision

**HOLD**

이번 보완으로 initial watcher UI freeze와 요구사항 상태 과대 표기는 해소됐고 모든 코드 품질 게이트가 통과한다. 그러나 cloud evidence end-to-end 계약, outbound 질문 secret filtering, 앱 scan cancellation, stale detection 완전성, UI 문서 정합성이라는 5개 Major finding이 남아 있다. 또한 현재 감사 대상은 미커밋 working tree이므로 수정 완료 후 commit 기준 재감사가 필요하다.

## 9. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_12.md`의 Re-audit #10 결과를 기준으로 수정하세요.
우선 IMP-F004에서 cloud request에 AnswerBundle schema/snapshot/file hash/range를 제공하고 앱 full-content 검증 E2E를 닫으세요.
다음으로 SEC-F002의 outbound user question 전체 필터와 중복 제거, DBG-F003의 app-level scan cancellation/omission UI,
DBG-F002의 preserved-mtime same-size stale detection을 처리하세요. IMP-F003 문서-UI 계약과 IMP-F002 버전을 동기화한 뒤,
commit 기준으로 fmt, strict clippy, workspace tests, locked release build, cargo audit를 재실행하세요.
```
