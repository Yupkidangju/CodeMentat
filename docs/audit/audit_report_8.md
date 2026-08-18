# D3D 재감사 보고서 (Turn 8 / Re-audit #6)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_7.md`
- 감사 대상 HEAD: `4823d5871080e79990be1e966477fef26d7795c6`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

Re-audit #5의 주요 기술 경계가 의미 있게 개선됐다. UI의 모든 파일 preview와 background 작업이 비차단 channel polling으로 전환됐고 repaint/error/disconnect 처리가 추가됐다. endpoint는 URL parser로 정확한 loopback/HTTPS를 검증하며 Gemini key는 header로 전달된다. HTTP send 이전 cancellation과 request timeout, byte-buffered SSE parsing이 추가됐고 scanner는 fail-closed canonical path를 직접 연다. 공급망 High 2건은 owner·만료일·재검토 조건이 있는 Accepted Risk로 문서화됐다.

포맷, Strict Clippy, 29개 테스트, Windows locked release build는 통과한다. 그러나 traceability의 여러 Implemented 판정, repository snapshot/watcher/evidence 일관성, 전체 resource budget, 실제 cloud evidence normalization, profile/session 복원, 실제 adapter failure tests가 남아 있어 전체 판정은 `HOLD`다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 29 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 29 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | High 2건 + unmaintained 2건, DEC-SEC-004 Accepted Risk 적용 |
| `git status --short --branch` | PASS — clean `master...origin/master` |

## 3. Finding 재판정표

| Finding | Re-audit #6 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline authority/ID 원문 유지 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major, 부분 개선) | resize 구현, exact design token/global hotkey 불일치 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | `/where` 추가, conflict/cloud evidence 검증 미완료 |
| IMP-F005 | Needs Fix (Major, 부분 개선) | CI 구성, profile/session 복원 미완료 |
| IMP-F006 | Needs Fix (Major, 부분 개선) | Partial 정정 증가, 일부 Implemented 과대 표시 |
| DBG-F001 | **Verified** | preview 포함 UI blocking receive 0건 |
| DBG-F002 | Needs Fix (Major) | double scan/shallow watcher/evidence exactness 유지 |
| DBG-F003 | Needs Fix (Major, 부분 개선) | bounded memory/read 추가, total budget/count/cancel 미구현 |
| DBG-F004 | Verified | semaphore permit 반환 유지 |
| DBG-F005 | Needs Fix (Major) | 29 tests, production adapter/app failure coverage 제한 |
| DBG-F006 | Verified | Git/lockfile/CI 경계 유지 |
| DBG-F007 | Needs Fix (Minor, 부분 개선) | task wake/error fixed, stream sender disconnect terminal 처리 누락 |
| SEC-F001 | Verified | approved request consume-once 유지 |
| SEC-F002 | Needs Fix (Major, 부분 개선) | AWS/JWT/escaped/query preview 추가, generic entropy/user exclusion 미완료 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | **Verified** | parsed URL exact loopback, userinfo 거부, header key, redacted Debug |
| SEC-F005 | **Verified** | hard timeout, pre-response cancellation, byte-buffered SSE 적용 |
| SEC-F006 | **Verified** | fail-closed canonical path direct-open 적용 |
| SEC-F007 | **Accepted Risk** | owner/expiry/review 조건과 Windows reachability 기록 |
| SEC-F008 | Verified | R/O badge session 상태 연결 |
| SEC-F009 | Verified | Unicode token range 유지 |
| SEC-F010 | Verified | Unicode assignment offset 유지 |

## 4. 상세 재감사

### [DBG-F001] Re-audit #6 — UI blocking receive 해소

- Pass: Debug
- Pattern: `DBG-001`
- Area: UI responsiveness
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-app/src/app.rs`
- Evidence:
  - scan, ping, local workflow, egress, preview가 모두 receiver state로 관리된다.
  - `recv_timeout`이 app UI 경로에서 제거됐다.
  - pending task 동안 `request_repaint_after(16ms)`가 적용된다.
- Expected / Actual: UI update 경로의 blocking receive 0건 — 일치.
- Remaining Risk: p95 100ms 실측 benchmark는 NFR 후속 gate에 남는다.
- Suggested Fix: 없음.
- Re-audit Method: frame/input latency benchmark를 release gate에서 추가한다.
- Owner: Auditor

### [DBG-F007] Re-audit #6 — task wake/error는 해소됐으나 stream disconnect가 남았다

- Pass: Debug
- Pattern: `DBG-001`, `TEST-001`
- Area: async UI state machine
- Severity: Minor (Major에서 하향)
- Status: Needs Fix
- Modified Files: `crates/mentat-app/src/app.rs`
- Evidence:
  - pending task 동안 16ms repaint가 예약된다.
  - scan/ping/local/egress/preview의 success, inner Err, Disconnected, Empty가 모두 처리된다.
  - streaming receiver는 `while let Ok(event) = rx.try_recv()`만 사용한다.
  - backend task가 최종 이벤트 없이 panic/종료하면 sender가 disconnect되지만 `is_streaming`과 receiver가 남아 지속 repaint한다.
- Expected: 모든 async receiver가 success/error/disconnected terminal state를 가진다.
- Actual: 일반 작업은 닫혔지만 stream disconnect 한 경로가 남았다.
- Impact: 비정상 backend 종료 시 UI가 무한 streaming 상태가 될 수 있다.
- Suggested Fix: stream `TryRecvError::Disconnected`를 Failed terminal state로 소비한다.
- Re-audit Method: sender drop without terminal event fixture를 검증한다.
- Owner: Coder

### [SEC-F004] Re-audit #6 — endpoint/key 노출 주요 경계 해소

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: credentials, endpoint validation
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-inference/src/types.rs`, inference adapters
- Evidence:
  - `url::Url`로 scheme, exact host, userinfo를 검증한다.
  - `localhost.evil.com`, userinfo, loopback suffix, remote HTTP, FTP 거부 테스트가 통과한다.
  - Gemini key가 `x-goog-api-key` header로 이동했다.
  - BackendProfile Debug는 key 원문을 출력하지 않는다.
- Expected / Actual: HTTPS 또는 exact loopback HTTP만 허용하고 로그 표면에서 key redaction — 일치.
- Remaining Risk: BackendProfile은 session-memory `String`과 Serialize를 사용하므로 장기적으로 secret_ref/keychain 경계가 권장된다. 현재 storage schema는 key를 저장하지 않는다.
- Suggested Fix: 후속 hardening으로 `#[serde(skip)]` 또는 secret wrapper 적용.
- Re-audit Method: URL table test와 DB/log key absence 유지.
- Owner: Auditor

### [SEC-F005] Re-audit #6 — timeout/cancellation/SSE byte framing 해소

- Pass: Security
- Pattern: `SEC-002`
- Area: HTTP request lifecycle
- Severity: Major
- Status: Verified
- Modified Files: `openai_adapter.rs`, `gemini_adapter.rs`
- Evidence:
  - `timeout_secs`가 5~300초로 clamp되어 request timeout에 적용된다.
  - `.send()` future가 cancellation token과 `tokio::select!`로 경쟁한다.
  - response stream은 byte buffer에 누적한 뒤 newline 단위로 UTF-8 변환하여 chunk 경계의 multibyte 손실을 방지한다.
- Expected / Actual: pre-response cancel, hard timeout, split UTF-8/SSE framing — 코드 경계 일치.
- Remaining Risk: 실제 slow server/split chunk adapter fixture가 아직 없다(`DBG-F005`).
- Suggested Fix: 없음.
- Re-audit Method: adapter integration fixture를 추가해 현재 동작을 잠근다.
- Owner: Auditor

### [SEC-F006] Re-audit #6 — canonical path direct-open 해소

- Pass: Security
- Pattern: `SEC-004`
- Area: symlink/path boundary
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-repository/src/scanner.rs`, `tests.rs`
- Evidence:
  - root/file canonicalization 실패가 오류로 반환된다.
  - canonical file이 root 밖이면 `ExternalPathBlocked`를 반환한다.
  - metadata와 `File::open`이 검증된 canonical path를 직접 사용한다.
  - inside-root와 traversal 회귀 테스트가 통과한다.
- Expected / Actual: 검증한 canonical path와 실제 open target 동일 — 일치.
- Remaining Risk: 플랫폼별 symlink/junction 실fixture는 후속 보강 권장.
- Suggested Fix: 없음.
- Re-audit Method: Linux symlink/Windows junction CI fixture 추가.
- Owner: Auditor

### [SEC-F007] Re-audit #6 — 공급망 finding을 조건부 Accepted Risk로 전환

- Pass: Security
- Pattern: `SEC-006`, `DEP-001`
- Area: supply chain
- Severity: Major
- Status: Accepted Risk
- Modified Files: `DESIGN_DECISIONS.md`
- Evidence:
  - `quick-xml 0.30.0` High 2건은 계속 검출된다.
  - owner `@Yupkidangju`, expiry `2026-11-30`, eframe/accesskit patch release 재검토 조건이 기록됐다.
  - current Windows shipment에서 target-specific Linux AT-SPI 경로라는 reachability 분석이 있다.
- Expected / Actual: 위험, 영향 범위, owner, 만료일, 재검토 조건 — 충족.
- Impact: Windows current target에서는 조건부 수용. Linux shipment 전 별도 재감사 필요.
- Suggested Fix: Linux 배포 전 upgrade 또는 runtime reachability 증거 갱신.
- Re-audit Method: expiry/trigger 도달 시 `cargo audit`와 Linux 경로 재감사.
- Owner: Security Lead

### [SEC-F002] Re-audit #6 — egress는 개선됐지만 일반 secret/user exclusion이 남았다

- Pass: Security
- Pattern: `SEC-001`
- Area: least-data egress
- Severity: Major
- Status: Needs Fix
- Evidence:
  - PEM, Google/OpenAI/GitHub, AWS, JWT, escaped assignment redaction과 exact file/line preview가 있다.
  - 질문 keyword로 file path relevance를 계산한다.
  - 그러나 generic high-entropy/새 provider bearer 탐지가 없다.
  - 사용자 per-request exclude가 없다.
  - content relevance가 아니라 path만 점수화하며 0점 문서도 포함할 수 있다.
- Expected: scanner, query relevance, user exclusion이 함께 최소 전송 범위를 확정한다.
- Actual: known pattern/preview는 강해졌지만 일반 비밀과 최소화 정책은 부분 구현이다.
- Impact: 신규 credential 형식 또는 무관 파일 전송 가능.
- Suggested Fix: entropy/credential detector, user exclude, zero-score threshold/content retrieval 추가.
- Re-audit Method: random bearer, zero-score doc, per-request exclusion fixture.
- Owner: Coder / Security

### [IMP-F006] Re-audit #6 — 상태 정정은 진전됐지만 과대 판정이 남는다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: traceability status
- Severity: Major
- Status: Needs Fix
- Evidence:
  - FR-006/008/015/016/023/024/026, NFR-002/003/009/010/012가 Partial로 정정됐다.
  - 그러나 FR-002는 전후 hash/permission/mtime/Git event 3-pass 테스트 없이 Implemented다.
  - FR-003의 테스트는 실제 symlink/junction이 아니라 traversal/internal file만 검사한다.
  - FR-009/010은 cloud raw text가 Claim/Evidence로 정규화되지 않는데 Implemented다.
  - FR-011의 local structure/where claims는 evidence_ids가 비어 있고 클릭 가능한 exact evidence가 없는데 Implemented다.
  - FR-013은 backend profile 저장이 없는데 Implemented다.
  - FR-017은 fake/OpenAI가 동일 conformance suite를 통과하지 않는데 Implemented다.
  - FR-020은 announcer threshold 설정이 없는데 Implemented다.
  - FR-021은 위험/미확정 workflow가 없는데 Implemented다.
  - FR-022 watcher는 root 직계 mtime만 확인하고 reindex action이 없는데 Implemented다.
  - FR-025는 외부 파일 저장 없이 clipboard만 제공하는데 Implemented다.
  - CON-008은 capability detection/response validation이 아닌 health check만 있어 Implemented 근거가 부족하다.
- Expected: baseline acceptance 전체가 닫힌 항목만 Implemented.
- Actual: status 정정은 개선됐지만 다수 부분 구현이 남는다.
- Impact: Phase completion과 release 판단이 과대평가된다.
- Suggested Fix: 해당 행을 Partial로 낮추고 정확한 finding/test와 연결한다.
- Re-audit Method: Implemented 행별 acceptance criterion을 독립 재현한다.
- Owner: Architect / Coder

## 5. 변경되지 않은 주요 Finding

- `IMP-F002` Minor: prerelease 문서와 Cargo version 불일치.
- `IMP-F003` Major: viewport 기능은 있으나 exact sizes/theme/global shortcut 불일치.
- `IMP-F004` Major: cloud AnswerBundle/evidence normalization과 실제 conflict 분석 미완료.
- `IMP-F005` Major: CI 구성은 있으나 profile/session 복원 미완료.
- `DBG-F002` Major: file list/snapshot double scan, shallow watcher, evidence 범위 무결성 미완료.
- `DBG-F003` Major: 전체 byte/file/time/cancellation budget 및 100k/2GiB benchmark 미완료.
- `DBG-F005` Major: 실제 adapter slow/cancel/SSE split와 app state integration tests 부족.

## 6. 상태 집계

- Verified: `IMP-F001`, `DBG-F001`, `DBG-F004`, `DBG-F006`, `SEC-F001`, `SEC-F003`, `SEC-F004`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`
- Accepted Risk: `SEC-F007` (expiry 2026-11-30)
- 미해결: Critical 0, Major 8, Minor 2
- 전체 판정: **HOLD**

## 7. 다음 수정 우선순위

1. `IMP-F006`: 미완료 baseline 행을 Partial로 정정.
2. `DBG-F002`, `DBG-F003`: single-scan snapshot, recursive watcher, repository budgets/benchmark.
3. `IMP-F004`, `IMP-F005`: cloud evidence normalization과 profile/session 복원.
4. `SEC-F002`: generic entropy/user exclude/content threshold.
5. `DBG-F005`, `IMP-F003`: production integration tests와 UI 계약.
6. `DBG-F007` stream disconnect terminal 처리 및 `IMP-F002` version sync.

## 8. Final Decision

**HOLD**

보안·비동기 핵심 경계는 크게 개선됐지만 8개 Major finding이 수정 또는 별도 Accepted Risk로 닫히지 않았다. `SEC-F007`은 Windows current target에 한해 조건부 수용하며 Linux shipment 전 재감사가 필요하다.

## 9. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_8.md`의 Re-audit #6 결과를 기준으로 수정하세요.
먼저 IMP-F006에서 미완료 acceptance 항목을 Partial로 정정하세요.
다음으로 DBG-F002/003의 single-scan snapshot, recursive watcher, repository budget을 처리하고,
IMP-F004/005의 cloud evidence normalization과 profile/session 복원을 구현하세요.
SEC-F002와 실제 adapter/app integration tests를 보강한 뒤 전체 게이트를 재실행하세요.
```
