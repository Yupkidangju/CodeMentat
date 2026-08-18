# D3D 재감사 보고서 (Turn 7 / Re-audit #5)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_6.md`, 상세 baseline `docs/audit/audit_report_5.md`
- 감사 대상 HEAD: `61bd7e2c3d820e70bb8065706ea40bf806c947ca`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

이번 감사 중 수정본이 병행 반영되어 중간 상태를 폐기하고 최종 clean HEAD의 파일 해시를 다시 고정했다. Git 프로젝트 경계와 `Cargo.lock` 추적, 3-platform CI 구성, UI 주요 작업의 비차단 channel 전환, Tier viewport resize, bounded file reading, symlink canonical guard, HTTP timeout, Gemini header key, redacted Debug, R/O 상태 연결이 추가됐다.

포맷, Strict Clippy, 26개 테스트, Windows locked release build는 통과한다. 그러나 새 비동기 polling은 background task 완료 시 repaint를 요청하지 않고 일부 오류/disconnect를 처리하지 않아 UI가 영구 대기할 수 있다. endpoint validation은 문자열 prefix 기반이라 `localhost.evil.com` 같은 비루프백 HTTP를 허용한다. 공급망 High 2건의 Accepted Risk도 owner와 만료일이 없어 감사 표준을 충족하지 않는다. 전체 판정은 계속 `HOLD`다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 26 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 26 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 경고 2건 |
| `git status --short --branch` | PASS — `master...origin/master`, clean |
| `git ls-files Cargo.lock` | PASS — lockfile tracked |

## 3. Finding 재판정표

| Finding | Re-audit #5 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline authority/ID 원문 보존 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major, 부분 개선) | viewport resize 추가, exact token/size와 global hotkey 불일치 유지 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | `/where` 추가, 실제 conflict/cloud evidence 검증 미완료 |
| IMP-F005 | Needs Fix (Major, 부분 개선) | CI matrix 추가, profile/session 복원 미완료 |
| IMP-F006 | Needs Fix (Major, 부분 개선) | 일부 Partial 정정, 다수 acceptance status 과대 표시 |
| DBG-F001 | Needs Fix (Major, 부분 개선) | 주요 작업 비차단화, preview 100ms block 및 async wake/error 문제 유지 |
| DBG-F002 | Needs Fix (Major) | snapshot double scan, shallow watcher, evidence 범위 문제 유지 |
| DBG-F003 | Needs Fix (Major, 부분 개선) | streaming hash/10MB cap 추가, total budget/file count/cancel 미구현 |
| DBG-F004 | Verified | semaphore permit 회귀 유지 |
| DBG-F005 | Needs Fix (Major) | 26 tests, production app/adapter failure coverage 제한 |
| DBG-F006 | **Verified** | Git root 정상화, clean origin, lockfile tracked, CI 존재 |
| DBG-F007 | 신규 Needs Fix (Major) | async polling에 wake/error/disconnect 상태 전이 누락 |
| SEC-F001 | Verified | immutable approved request 유지 |
| SEC-F002 | Needs Fix (Major, 부분 개선) | AWS/JWT/escape/query preview 추가, generic entropy/user exclusion 미완료 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | Needs Fix (Major, 부분 개선) | header/debug/TLS guard 추가, prefix bypass와 plaintext Serialize 유지 |
| SEC-F005 | Needs Fix (Major, 부분 개선) | hard timeout 추가, pre-response cancellation/SSE framing 미완료 |
| SEC-F006 | Needs Fix (Major, 부분 개선) | canonical guard 추가, TOCTOU/fail-open/error swallowing/test 미해소 |
| SEC-F007 | Needs Fix (Major) | High 2건 Accepted Risk 요건 미충족 |
| SEC-F008 | **Verified** | R/O badge가 session boundary 상태를 사용 |
| SEC-F009 | Verified | Unicode token scanner 유지 |
| SEC-F010 | Verified | Unicode assignment offset 유지 |

## 4. 상세 재감사

### [DBG-F006] Re-audit #5 — Git/lockfile/CI 경계 해소

- Pass: Debug
- Pattern: `BUILD-001`, `DEP-001`
- Area: repository scope, reproducibility
- Severity: Major
- Status: Verified
- Modified Files: `.gitignore`, `.github/workflows/ci.yml`, Git metadata
- Evidence:
  - Git top-level이 `C:/LocalDev/rust/CodeMentat`다.
  - branch는 `master...origin/master`, worktree는 clean이다.
  - `Cargo.lock`이 tracked 상태다.
  - Ubuntu/Windows/macOS matrix에서 fmt, Clippy, tests, locked release build를 실행하는 workflow가 존재한다.
- Expected / Actual: 프로젝트 범위 Git과 tracked lockfile — 일치.
- Remaining Risk: 실제 CI run 결과는 이번 로컬 감사에서 확인하지 않았다.
- Suggested Fix: 없음.
- Re-audit Method: PR/commit의 CI run 결과를 release gate에서 확인한다.
- Owner: Auditor

### [SEC-F008] Re-audit #5 — Read-Only badge 상태 연결 해소

- Pass: Security
- Pattern: `SEC-005`
- Area: security-state UI
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-app/src/app.rs`, `widgets/pill_bar.rs`
- Evidence:
  - `PillBar::new`이 외부 `is_read_only` 상태를 입력받는다.
  - app은 격리 검증과 `ReadOnlySession::open` 후 `session.is_some()`을 badge 상태로 전달한다.
- Expected / Actual: session boundary 확립 전 고정 true를 표시하지 않음 — 일치.
- Suggested Fix: 없음.
- Re-audit Method: no-session/invalid-root/valid-session UI 상태 테스트를 후속 추가한다.
- Owner: Auditor

### [DBG-F001] Re-audit #5 — 주요 UI 작업은 비차단화됐으나 완전히 닫히지 않았다

- Pass: Debug
- Pattern: `DBG-001`
- Area: UI responsiveness
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-app/src/app.rs`
- Evidence:
  - scan, ping, local workflow, egress assembly는 channel과 `try_recv()` polling으로 전환됐다.
  - Tier transition은 `ViewportCommand::InnerSize`를 사용한다.
  - `load_file_preview()`는 UI 호출 경로에서 여전히 최대 100ms `recv_timeout`으로 대기한다.
  - spec의 NFR-002는 “UI 스레드를 막지 않음”을 요구하므로 100ms blocking도 계약 불일치다.
- Expected: UI update 경로에 blocking receive가 없어야 한다.
- Actual: 대부분 개선됐지만 preview 한 경로가 남았다.
- Impact: 파일 선택 시 frame long-task와 입력 지연이 발생할 수 있다.
- Suggested Fix: preview도 receiver/state polling으로 전환하고 frame latency를 측정한다.
- Re-audit Method: 모든 `recv_timeout` 검색 0건과 p95 frame/input latency 검증.
- Owner: Coder

### [DBG-F007] 신규 — 비동기 channel 완료가 UI를 깨우지 않고 오류 상태가 소실된다

- Pass: Debug
- Pattern: `DBG-001`, `TEST-001`
- Area: async UI state machine
- Severity: Major
- Status: Needs Fix
- Evidence:
  - scan/ping/local/egress background task는 결과를 channel로 보내지만 task 완료 시 `egui::Context::request_repaint()`를 호출하지 않는다.
  - `update()`의 명시적 `ctx.request_repaint()`는 streaming 중에만 호출된다.
  - 입력 이벤트가 없으면 완료 결과가 receiver에 남고 UI가 갱신되지 않을 수 있다.
  - scan polling은 `(Ok(files), Ok(snapshot))`만 처리한다. `Ok((Err, _))` 또는 `Ok((_, Err))` 메시지는 소비한 뒤 receiver를 남겨 “인덱싱 중” 상태가 영구화된다.
  - local workflow도 `Ok(Ok(bundle))`만 처리해 error/disconnect 상태를 남긴다.
- Expected: task 완료가 UI를 깨우고 success/error/disconnected가 모두 terminal state로 전이해야 한다.
- Actual: success 일부만 처리하고 wakeup 및 실패 전이가 누락됐다.
- Impact: 저장소 열기나 로컬 분석이 사용자 입력 전까지 멈춰 보이거나 영구 대기 상태가 될 수 있다.
- Suggested Fix: task에 cloned Context를 전달해 완료 후 repaint하거나 pending task 동안 `request_repaint_after`를 사용한다. `TryRecvError::Disconnected`와 inner Err를 모두 상태로 소비한다.
- Re-audit Method: 입력 없는 task 완료, scan Err, local Err, sender panic/disconnect fixture를 검증한다.
- Owner: Coder

### [DBG-F003] Re-audit #5 — 메모리 상한은 개선됐지만 repository budget은 미완료

- Pass: Debug
- Pattern: `DBG-002`
- Area: resource limits
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-repository/src/scanner.rs`, `session.rs`
- Evidence:
  - scanner가 64KiB chunk로 전체 hash를 계산하고 text sample은 2MiB로 제한한다.
  - direct content read는 10MiB에서 거부한다.
  - 그러나 scan은 파일 전체를 끝까지 hash하며 개별 hash bytes, 총 bytes, 파일 수, wall-clock, cancellation budget이 없다.
  - baseline 100,000파일/2GiB benchmark와 메모리/시간 상한 결과가 없다.
- Expected: file/byte/count/time/cancel budget과 결정적 benchmark.
- Actual: 메모리 peak는 낮아졌지만 전체 I/O/시간은 무제한이다.
- Impact: 대형/특수 저장소가 장시간 CPU·디스크를 점유할 수 있다.
- Suggested Fix: ScanBudget과 cancellation을 도입하고 limit 초과 사유를 FileRecord/summary에 기록한다.
- Re-audit Method: giant file, 100k files, 2GiB, cancellation benchmark.
- Owner: Coder

### [SEC-F002] Re-audit #5 — egress 탐지와 preview는 개선됐지만 일반 secret/user policy가 남았다

- Pass: Security
- Pattern: `SEC-001`
- Area: content filtering, least-data
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-analysis/src/egress.rs`, `crates/mentat-app/src/app.rs`
- Evidence:
  - AWS Access Key, JWT, escaped quoted assignment 회귀 테스트가 추가됐다.
  - 파일 경로 keyword scoring과 included file/line preview가 추가됐다.
  - 그러나 일반 고엔트로피 문자열, 임의 bearer/새 provider credential 탐지가 없다.
  - 사용자 per-request 제외 정책이 없다.
  - relevance는 파일 경로 keyword만 사용하며 콘텐츠 관련성은 평가하지 않는다. 점수 0인 문서도 최대 8개까지 포함된다.
- Expected: content-aware 최소 문맥과 사용자 제외가 secret scanner와 함께 동작해야 한다.
- Actual: 알려진 패턴과 preview는 개선됐지만 일반/미등록 비밀과 무관 문서 전송 가능성이 남는다.
- Impact: 사용자가 보지 못한 새로운 credential 형식 또는 무관 소스가 전송될 수 있다.
- Suggested Fix: generic entropy/credential detector, per-request exclude, score threshold 또는 content retrieval을 추가한다.
- Re-audit Method: random bearer/new provider key, zero-score files, user exclude fixture를 검증한다.
- Owner: Coder / Security

### [SEC-F004] Re-audit #5 — key transport는 개선됐지만 URL allowlist 우회가 가능하다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: credential handling, endpoint validation
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-inference/src/types.rs`, inference adapters
- Evidence:
  - BackendProfile Debug는 API key를 redacted 표시한다.
  - Gemini는 `x-goog-api-key` header를 사용한다.
  - adapter가 endpoint validation을 호출한다.
  - 그러나 `validate_url()`은 문자열 `starts_with("http://localhost")` 또는 `starts_with("http://127.0.0.1")`를 사용한다.
  - `http://localhost.evil.com`, `http://localhost@evil.com`, `http://127.0.0.1.evil.com`이 허용되어 평문 API key와 repository context를 외부로 보낼 수 있다.
  - BackendProfile은 여전히 `Serialize/Deserialize` 가능한 plaintext `api_key`를 소유한다.
- Expected: parsed URL의 scheme/host/port를 검증하고 비밀은 secret reference로 분리해야 한다.
- Actual: log redaction과 header는 개선됐지만 transport boundary를 우회할 수 있다.
- Impact: 잘못되거나 악의적인 endpoint 값으로 credential/code 유출 가능.
- Suggested Fix: URL parser로 정확한 loopback host/IP를 판정하고 userinfo를 거부한다. profile에는 secret_ref만 저장한다.
- Re-audit Method: localhost subdomain/userinfo/IPv4 suffix/IPv6 loopback/non-loopback HTTP table test.
- Owner: Coder / Security

### [SEC-F005] Re-audit #5 — timeout은 추가됐지만 요청 전구간 cancellation은 미완료

- Pass: Security
- Pattern: `SEC-002`
- Area: request lifecycle
- Severity: Major
- Status: Needs Fix
- Modified Files: inference adapters
- Evidence:
  - request timeout은 `timeout_secs.clamp(5, 300)`으로 적용된다.
  - cancellation token은 response body stream loop에서 확인한다.
  - DNS/connect/send/response-header를 기다리는 `.send().await`는 cancellation `select!` 밖에 있다.
  - OpenAI SSE는 raw chunk별 `from_utf8` 실패 시 chunk를 버리고, Gemini는 `from_utf8_lossy`로 split multibyte를 손상시킬 수 있다.
- Expected: connect/send/header/body 전체 cancellation과 byte-buffered SSE framing.
- Actual: hard timeout은 생겼지만 즉시 Esc cancellation과 UTF-8 무결성이 전구간에 적용되지 않는다.
- Impact: 취소 지연과 스트리밍 텍스트 손실 가능.
- Suggested Fix: `.send()`도 cancellation select로 감싸고 bytes accumulator에서 complete UTF-8/SSE frames를 파싱한다.
- Re-audit Method: cancel-before-response, split multibyte, split SSE line fixture.
- Owner: Coder

### [SEC-F006] Re-audit #5 — symlink guard는 추가됐지만 TOCTOU와 failure handling이 남았다

- Pass: Security
- Pattern: `SEC-004`
- Area: path boundary
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-repository/src/scanner.rs`
- Evidence:
  - scanner가 full path와 root를 canonicalize하여 외부 target을 거부한다.
  - 하지만 canonicalization 오류는 중첩 `if let`에서 무시되고 원래 `full_path`를 계속 연다.
  - canonical path를 검증한 뒤 실제 open은 canonical path가 아니라 원래 path이므로 symlink swap race가 남는다.
  - `scan_files()`는 `inspect_file()`의 `ExternalPathBlocked`를 포함한 모든 오류를 조용히 버린다.
  - symlink/junction 회귀 테스트가 없다.
- Expected: fail-closed canonical path를 직접 open하고 boundary violation을 관찰 가능한 결과로 남겨야 한다.
- Actual: 일반 external symlink는 차단되지만 오류/race 경계가 봉인되지 않았다.
- Impact: 경쟁 조건이나 플랫폼 경로 특이점에서 root 외부 접근 가능성이 남는다.
- Suggested Fix: canonical root를 session에서 재사용하고 canonical file handle/path를 직접 열며 security error를 전파/기록한다.
- Re-audit Method: file symlink, directory symlink, junction, swap race, canonicalization failure tests.
- Owner: Coder / Security

### [SEC-F007] Re-audit #5 — Accepted Risk 문서가 감사 요건을 충족하지 않는다

- Pass: Security
- Pattern: `SEC-006`, `DEP-001`
- Area: supply chain accepted risk
- Severity: Major
- Status: Needs Fix
- Modified Files: `DESIGN_DECISIONS.md`
- Evidence:
  - Windows target에서 quick-xml이 비도달이라는 분석과 upgrade trigger는 기록됐다.
  - 그러나 제품은 Linux를 지원/CI 빌드하며 vulnerable path는 Linux accessibility dependency에 포함된다.
  - Accepted Risk에 책임자(owner), 만료일, 정기 재검토 시점이 없다.
  - `cargo audit`은 여전히 High 2건으로 exit 1이다.
- Expected: owner, 구체 만료일, 영향 범위, 재검토 조건, target별 reachability evidence.
- Actual: 이유와 완화 방향만 있고 공식 위험 수용 gate가 닫히지 않았다.
- Impact: High finding이 무기한 방치될 수 있다.
- Suggested Fix: Linux shipment 범위를 명시하고 owner/expiry/review date를 기록하거나 dependency를 갱신한다.
- Re-audit Method: Accepted Risk 필드와 Linux dependency/runtime evidence를 확인한다.
- Owner: Human / Security

### [IMP-F006] Re-audit #5 — traceability 상태가 개선됐지만 일부 완료 주장은 여전히 과대하다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: completion evidence
- Severity: Major
- Status: Needs Fix
- Modified Files: `spec.md`, `IMPLEMENTATION_SUMMARY.md`
- Evidence:
  - FR-006/NFR-003/FR-023/024/026/NFR-012는 Partial로 정정됐다.
  - 그러나 NFR-002는 preview blocking과 DBG-F007이 남았는데 Implemented다.
  - NFR-008은 pre-response cancellation이 없는데 Implemented다.
  - NFR-009는 DB migration만으로 backup/reset/손상 복구를 Implemented로 표시한다.
  - NFR-013은 key store/time/watcher failure seam이 부족하지만 Implemented다.
  - FR-015는 variant 존재만으로 provider별 안정 오류/복구 지침을 Implemented로 표시한다.
  - FR-016은 generic entropy/user exclude가 없지만 Implemented다.
  - 문서의 테스트 개수는 24였으나 실제는 26으로 다시 drift했다.
- Expected: acceptance criterion 전체와 failure-specific test가 일치할 때만 Implemented.
- Actual: 상태 정정은 진전됐지만 타입/부분 경로를 완료 증거로 사용하는 행이 남는다.
- Impact: 남은 Major finding이 추적표에서 숨겨진다.
- Suggested Fix: 관련 행을 Partial로 낮추고 본 보고서 finding과 정확히 연결한다.
- Re-audit Method: 각 Implemented 행의 acceptance criterion을 독립 재현한다.
- Owner: Architect / Coder

## 5. 변경되지 않았거나 일부만 개선된 기존 Finding

- `IMP-F002` Minor: 문서 prerelease와 Cargo package version 불일치.
- `IMP-F003` Major: viewport transition은 추가됐으나 exact sizes/theme/global shortcut 불일치.
- `IMP-F004` Major: `/where`는 추가됐으나 evidence 없는 경로 나열, 실제 conflict/cloud validation 미완료.
- `IMP-F005` Major: CI config는 추가됐으나 profile/session 복원 미완료.
- `DBG-F002` Major: snapshot double scan, shallow watcher, evidence exactness 미완료.
- `DBG-F005` Major: 26 tests지만 production app/network failure coverage 제한.

## 6. 상태 집계

- Verified: `IMP-F001`, `DBG-F004`, `DBG-F006`, `SEC-F001`, `SEC-F003`, `SEC-F008`, `SEC-F009`, `SEC-F010`
- 미해결: Critical 0, Major 14, Minor 1
- 신규: `DBG-F007`
- 전체 판정: **HOLD**

## 7. 다음 수정 우선순위

1. `DBG-F007`, `DBG-F001`: async wake/error state와 preview blocking 제거.
2. `SEC-F004`, `SEC-F006`: parsed URL allowlist와 canonical path direct-open.
3. `SEC-F005`: pre-response cancellation과 byte-buffered SSE.
4. `IMP-F006`: 남은 과대 상태를 Partial로 정정.
5. `SEC-F002`, `SEC-F007`, `DBG-F002/003/005`, `IMP-F003~005`.

## 8. Final Decision

**HOLD**

Git/CI, viewport, bounded reading, timeout, badge 등은 의미 있게 개선됐지만 새 async state 회귀와 URL allowlist 우회, symlink TOCTOU, 공급망 risk 및 14개 Major finding이 남아 있다.

## 9. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_7.md`의 Re-audit #5 결과를 기준으로 수정하세요.
먼저 DBG-F007/DBG-F001의 repaint·error·disconnect 상태와 preview blocking을 해결하세요.
그다음 SEC-F004의 parsed URL loopback 검증, SEC-F006의 canonical path direct-open,
SEC-F005의 pre-response cancellation/SSE byte buffering을 처리하세요.
IMP-F006의 남은 과대 상태를 Partial로 정정하고 전체 품질 게이트와 cargo audit를 재실행하세요.
```
