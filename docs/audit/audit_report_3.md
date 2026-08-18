# D3D 재감사 보고서 (Turn 3 / Re-audit #1)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_2.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

Turn 2의 Critical 3건 중 `SEC-F003`은 해소되었고 `SEC-F001`은 fail-open/TOCTOU 핵심 경로가 수정되었다. `DBG-F004` semaphore permit 누수도 해소되었다. 포맷, Strict Clippy, 전체 테스트, Windows release build는 통과한다.

그러나 `SEC-F002`의 새 redactor는 개인키 헤더만 마스킹하고 본문을 남기며, 다중 secret과 UTF-8 경계도 안전하게 처리하지 못한다. 공급망 High 취약점 2건과 나머지 Major finding도 유지된다. 따라서 Critical 1건을 포함한 미해결 finding 때문에 전체 판정은 계속 `HOLD`다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — **16 passed**, 0 failed |
| `cargo test --workspace --locked -- --list` | 16 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 경고 2건 |
| `git rev-parse --show-toplevel` | FAIL — 프로젝트가 아닌 `C:/` |

문서의 `15 tests passed` 주장은 실제 16개와 다르다.

## 3. 기존 Finding 재판정표

| Finding | 이전 | 재감사 결과 | 상태 |
|---|---|---|---|
| IMP-F001 | Major | authority ADR은 추가됐지만 기존 FR/NFR/CON ID의 의미를 재사용하고 전체 매핑을 제공하지 않음 | Needs Fix |
| IMP-F002 | Major | 1.0.0 릴리스 과대주장은 내려갔으나 문서는 `0.1.0-dev`, Cargo는 `0.1.0` | Needs Fix (Minor) |
| IMP-F003 | Major | viewport, theme token, global shortcut 미수정 | Needs Fix |
| IMP-F004 | Major | `/where`, 실제 conflict 분석, cloud AnswerBundle 검증 미수정 | Needs Fix |
| IMP-F005 | Major | profile/session 복원과 멀티플랫폼 CI 미수정 | Needs Fix |
| DBG-F001 | Major | UI의 `recv_timeout` 동기 대기 유지 | Needs Fix |
| DBG-F002 | Major | double scan, shallow watcher, excerpt-only hash 유지 | Needs Fix |
| DBG-F003 | Major | unbounded `read_to_end`/`read_to_string` 유지 | Needs Fix |
| DBG-F004 | Major | `OwnedSemaphorePermit` 소유 및 3번째 acquire 테스트 추가 | **Verified** |
| DBG-F005 | Major | fmt/Clippy는 해소, app/adapter 테스트·CI·audit는 미해소 | Needs Fix |
| DBG-F006 | Major | Git root `C:/`, `Cargo.lock` ignore 유지 | Needs Fix |
| SEC-F001 | Critical | fail-open과 재조립은 수정, receipt/snapshot/request binding과 app 회귀 테스트 부족 | Needs Fix (Major) |
| SEC-F002 | Critical | filename/content 필터 추가, 개인키 본문·다중 secret 누출과 UTF-8 안전성 미해소 | **Hold (Critical)** |
| SEC-F003 | Critical | fail-closed AppData 획득 및 실제 open 경로 격리 검증 추가 | **Verified** |
| SEC-F004 | Major | plaintext serializable key, Gemini URL key, HTTP endpoint 검증 미수정 | Needs Fix |
| SEC-F005 | Major | request 전체 timeout/cancellation 미수정 | Needs Fix |
| SEC-F006 | Major | scan symlink canonical guard와 회귀 테스트 미수정 | Needs Fix |
| SEC-F007 | Major | RustSec High 2건 유지 | Needs Fix |
| SEC-F008 | Major | R/O badge가 여전히 항상 true | Needs Fix |
| SEC-F009 | 신규 Major | redactor가 UTF-8 byte 경계에서 panic 가능 | Needs Fix |

## 4. 상세 재감사

### [IMP-F001] Re-audit #1 — 요구사항 추적표가 baseline ID를 재사용한다

- Pass: Implementation
- Pattern: `IMP-004`, `SPEC-GAP-001`
- Area: authority, requirements traceability
- Severity: Major
- Status: Needs Fix
- Modified Files: `spec.md`, `DESIGN_DECISIONS.md`, `CHANGELOG.md`, `IMPLEMENTATION_SUMMARY.md`
- Evidence:
  - `DESIGN_DECISIONS.md`의 DEC-ARCH-003은 `CODE_MENTAT_SPEC.md`를 baseline으로 선언했다.
  - 그러나 baseline `FR-001`은 “로컬 디렉터리/Git 저장소 열기”인데 `spec.md:17`은 “스캔 및 인덱싱”으로 재정의한다.
  - baseline `FR-003`은 symlink 탈출 차단인데 `spec.md:19`는 AppData 격리로 재정의한다.
  - baseline NFR-001~013/CON-001~008을 완전 매핑한다고 CHANGELOG가 주장하지만 `spec.md:48-52`에는 NFR-001~004와 CON-001만 있다.
  - `spec.md`는 `/where`, 실제 conflict 분석, Gemini/OpenAI adapter를 구현 완료로 표시하지만 전용 검증 테스트가 없다.
- Expected: baseline ID 의미를 그대로 유지하고 각 ID에 구현 상태와 정확한 검증 증거를 연결한다.
- Actual: ID 수만 맞춘 새 기능 목록으로 baseline 의미가 덮였다.
- Impact: 감사 추적성이 끊기고 미구현 기능이 Implemented로 오인된다.
- Suggested Fix: baseline 표를 그대로 복사해 의미를 보존하고 `Implemented / Partial / Missing / Superseded` 상태와 evidence를 추가한다. 새 기능은 새 ID를 부여한다.
- Re-audit Method: baseline 각 FR/NFR/CON 문장과 active matrix를 행 단위로 대조한다.
- Owner: Architect / Coder

### [DBG-F004] Re-audit #1 — semaphore permit 누수 해소

- Pass: Debug
- Pattern: `DBG-002`
- Area: concurrency
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-inference-llama/src/contract.rs`, `crates/mentat-inference-llama/src/lib.rs`
- Evidence:
  - `contract.rs:45-78`이 `OwnedSemaphorePermit`을 handle에 소유한다.
  - `contract.rs:95-110`이 `acquire_owned()`를 사용한다.
  - 테스트가 두 context 중 하나를 drop한 후 세 번째 context 획득과 permit 복원을 확인한다.
- Expected / Actual: context drop 시 permit 반환 — 일치.
- Impact: 기존 교착 위험 해소.
- Suggested Fix: 없음.
- Re-audit Method: 현재 회귀 테스트 유지.
- Owner: Auditor

### [DBG-F005] Re-audit #1 — 로컬 품질 게이트는 개선됐지만 릴리스 gate는 미완료

- Pass: Debug
- Pattern: `TEST-001`, `BUILD-001`
- Area: quality gates, regression coverage
- Severity: Major
- Status: Needs Fix
- Modified Files: 다수 Rust 파일, `IMPLEMENTATION_SUMMARY.md`
- Evidence:
  - formatter와 strict Clippy가 통과한다.
  - workspace 테스트 16개와 Windows release build가 통과한다.
  - `mentat-app`과 `mentat-inference-openai`는 여전히 테스트 0개다.
  - 문서는 15개 테스트라고 기록해 실행 결과와 다시 drift했다.
  - CI workflow와 Linux/macOS 실행 증거가 없다.
- Expected: 핵심 fail-closed consent, adapter timeout/SSE, UI state를 failure-specific tests와 CI로 고정한다.
- Actual: P0 app 흐름은 정적 코드만 바뀌었고 app-level 회귀 테스트가 없다.
- Impact: 다음 변경에서 consent 우회나 UI 회귀가 재발해도 테스트가 잡지 못한다.
- Suggested Fix: app orchestration을 테스트 가능한 순수 상태/서비스로 분리하고 adapter fixture 및 3-platform CI를 추가한다.
- Re-audit Method: clean CI에서 formatter, Clippy, tests, release, audit를 실행한다.
- Owner: Coder

### [SEC-F001] Re-audit #1 — fail-open은 닫혔지만 receipt 계약이 완전히 봉인되지 않았다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: consent, immutable request binding
- Severity: Major (Critical에서 하향)
- Status: Needs Fix
- Modified Files: `crates/mentat-analysis/src/egress.rs`, `crates/mentat-app/src/app.rs`
- Evidence:
  - `app.rs:285-326`은 packet 조립 timeout/실패 시 전송하지 않는다.
  - `app.rs:591-604`는 승인한 `EgressPacket` 객체를 재조립 없이 소비한다.
  - 하지만 packet에는 snapshot ID가 없고 receipt의 `snapshot_id`, `token_count`, `file_count`, `granted_at`은 `start_inference_stream_with_receipt()`에서 검증되지 않는다.
  - packet hash는 `prompt_context`만 묶고 별도 `query` 인자와 provider/profile을 묶지 않는다.
  - “single-use” 상태는 타입이나 소비 저장소에서 강제되지 않는다.
  - 새 egress 테스트는 packet assembly/hash만 검사하며 app의 timeout, consent, receipt 소비를 실행하지 않는다.
- Expected: snapshot, provider, model, question, exact refs, payload를 하나의 승인 digest로 봉인하고 receipt를 한 번만 소비한다.
- Actual: 핵심 TOCTOU 경로는 제거됐지만 receipt의 일부 필드가 장식적이다.
- Impact: 향후 호출 경로 확장 시 승인과 실제 요청이 다시 분리될 수 있다.
- Suggested Fix: immutable `ApprovedInferenceRequest`를 만들고 모든 승인 필드를 hash/검증하며 app-level fail-closed 테스트를 추가한다.
- Re-audit Method: mismatched snapshot/query/provider/file count와 receipt 재사용을 모두 거부하는지 검증한다.
- Owner: Coder / Security

### [SEC-F002] Re-audit #1 — private key 본문과 복수 secret이 여전히 전송된다

- Pass: Security
- Pattern: `SEC-001`
- Area: content secret redaction
- Severity: Critical
- Status: Hold
- Modified Files: `crates/mentat-analysis/src/egress.rs`
- Evidence:
  - 파일명 `token`, key/cert 확장자 차단은 추가됐다.
  - Google/OpenAI/GitHub key와 assignment redaction도 일부 추가됐다.
  - `egress.rs:96-152`는 line-by-line 처리한다. `BEGIN ... PRIVATE KEY` 헤더만 marker로 바꾸고 뒤의 Base64 본문과 `END ... PRIVATE KEY`는 그대로 남긴다.
  - 각 line에서 `find()`를 한 번만 호출하므로 같은 줄의 두 번째 API key/token은 남는다.
  - ADR은 `github_pat_` 탐지를 선언하지만 코드는 `ghp_`만 검사한다.
  - query-aware retrieval은 구현되지 않았고 단순히 첫 8개 문서/entrypoint를 선택한다.
  - 테스트는 marker 존재만 확인하고 개인키 본문 부재나 복수 secret 부재를 확인하지 않는다.
- Expected: secret block 전체와 모든 occurrence가 제거되고 redacted 원문이 복구 불가능해야 한다.
- Actual: 실제 개인키 본문과 일부 token이 prompt context에 남을 수 있다.
- Impact: 외부 API로 credential 원문 유출 가능.
- Suggested Fix: 상태 기반 PEM block redaction, 모든 occurrence 반복 탐지, `github_pat_`/JSON/YAML assignment/고엔트로피 규칙을 구현한다. 검출된 secret이 포함된 파일 전체 제외 정책도 검토한다.
- Re-audit Method: 완전한 PEM fixture, 한 줄 다중 key, `github_pat_`, JSON/YAML secret, Unicode 인접 key에서 원문 0건을 검증한다.
- Owner: Coder / Security

### [SEC-F003] Re-audit #1 — AppData 격리 강제 해소

- Pass: Security
- Pattern: `SEC-004`, `SEC-005`
- Area: strict read-only storage boundary
- Severity: Critical
- Status: Verified
- Modified Files: `crates/mentat-platform/src/lib.rs`, `crates/mentat-app/src/app.rs`, `crates/mentat-core/src/error.rs`
- Evidence:
  - AppData 경로 획득 실패 시 현재 디렉터리 fallback 없이 오류를 반환한다.
  - `app.rs:130-143`이 repository session과 recent-repo DB write 전에 격리 검증을 수행한다.
  - helper가 AppData-in-repo와 repo-in-AppData 양방향을 코드에서 거부한다.
- Expected / Actual: writable app path와 repository root 상호 포함 거부 — 일치.
- Remaining Risk: app orchestration 수준의 파일 이벤트 테스트는 아직 없다.
- Suggested Fix: P5 전체 read-only 이벤트 감사에서 보강.
- Re-audit Method: home/AppData parent/AppData child fixture와 실제 파일 이벤트 0건 검증.
- Owner: Auditor

### [SEC-F007] Re-audit #1 — 공급망 High 취약점 유지

- Pass: Security
- Pattern: `SEC-006`, `DEP-001`
- Area: supply chain
- Severity: Major
- Status: Needs Fix
- Modified Files: `Cargo.lock`
- Evidence:
  - `cargo audit`은 여전히 `quick-xml 0.30.0`의 `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`를 High로 보고한다.
  - `paste 1.0.15`, `ttf-parser 0.25.1` unmaintained 경고도 유지된다.
  - advisory fix는 `quick-xml >=0.41.0`이다.
- Expected: 안전한 dependency path 또는 owner/만료일/reachability 증거가 있는 Accepted Risk.
- Actual: lockfile이 바뀌었지만 취약 경로는 그대로다.
- Impact: Linux accessibility XML 처리 경로의 CPU/메모리 DoS 가능성.
- Suggested Fix: 상위 UI/accessibility dependency를 갱신하거나 reachability/mitigation을 문서화한다.
- Re-audit Method: `cargo audit`와 `cargo tree --target all -i quick-xml@0.30.0` 재실행.
- Owner: Coder / Security

### [SEC-F009] 신규 — Google key redaction이 UTF-8 byte 경계에서 panic할 수 있다

- Pass: Security
- Pattern: `SEC-001`
- Area: untrusted text handling, availability
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `egress.rs:99-103`은 `String::len()` byte 길이를 기준으로 `pos + 39`를 계산하고 `replace_range()`를 호출한다.
  - untrusted line에 `AIza` 뒤 Unicode 문자가 섞이면 `pos + 39`가 char boundary가 아닐 수 있으며 `String::replace_range`가 panic한다.
  - egress 조립 background task의 panic은 consent를 fail-closed시키지만 분석 작업을 실패시키는 입력 기반 DoS가 된다.
- Expected: untrusted UTF-8 입력에서 panic 없이 탐지/무시/마스킹해야 한다.
- Actual: 고정 byte offset을 안전한 문자/ASCII token parser 없이 사용한다.
- Impact: 저장소 내용으로 egress 조립을 반복 실패시킬 수 있다.
- Suggested Fix: ASCII allowed-character scanner 또는 검증된 secret scanning library를 사용하고 모든 range가 char boundary임을 보장한다.
- Re-audit Method: `AIza`와 다국어 문자를 혼합한 proptest/fuzz fixture에서 panic 0건을 검증한다.
- Owner: Coder / Security

## 5. 미해결 기존 Finding 증거 요약

다음 finding은 관련 코드가 실질적으로 바뀌지 않아 Turn 2 판정을 유지한다.

- `IMP-F003`: `main.rs`는 540x52 고정, Tier별 resize 없음, theme token 불일치, global hotkey 없음.
- `IMP-F004`: `/where`는 generic fallback이고 `/conflicts`는 실제 비교하지 않으며 cloud 결과는 AnswerBundle 검증을 거치지 않는다.
- `IMP-F005`: backend profile/session 복원 및 3-platform CI 없음.
- `DBG-F001`: UI 경로에 300ms~4초 `recv_timeout` 유지.
- `DBG-F002`: file list와 snapshot double scan, shallow root-only watcher, excerpt-only EvidenceRef hash 유지.
- `DBG-F003`: 파일 전체 `read_to_end`/`read_to_string`과 resource budget 부재.
- `DBG-F006`: Git root `C:/`, 프로젝트 커밋 없음, `.gitignore`가 `Cargo.lock` 제외.
- `SEC-F004`: serializable plaintext API key, Gemini query-string key, unrestricted custom HTTP 유지.
- `SEC-F005`: connect/send/header에 timeout/cancellation 미적용.
- `SEC-F006`: scan 경로의 symlink canonical root guard 부재.
- `SEC-F008`: R/O badge가 session 상태와 무관하게 항상 true.

## 6. Cross-Pass Conflicts

### [XPF-F003] P0 완료 문서와 Critical redaction 잔여가 충돌한다

- Related Findings: IMP-F001, SEC-F001, SEC-F002, SEC-F009
- Conflict: CHANGELOG와 ADR은 P0 secret 유출 위험 0건을 선언하지만 구현은 개인키 본문과 일부 token을 남긴다.
- Resolution: 보안 실행 증거가 문서 완료 주장보다 우선한다.
- Gate Impact: HOLD
- Required Fix Before PASS: SEC-F002와 SEC-F009 수정 및 adversarial regression tests.

## 7. Accepted Risks

- 없음.
- RustSec finding 및 대형 저장소 위험에 owner, 만료일, 재검토 조건이 없어 Accepted Risk로 전환할 수 없다.

## 8. 다음 수정 우선순위

1. **P0:** SEC-F002 — PEM block 전체/복수 secret/`github_pat_`/구조화 설정 redaction.
2. **P0:** SEC-F009 — Unicode-safe scanner와 fuzz/proptest.
3. **P1:** SEC-F001 — immutable approved request와 app fail-closed 테스트.
4. **P1:** IMP-F001 — baseline ID 의미를 보존한 실제 추적 매트릭스.
5. **P1:** SEC-F004~008, DBG-F001~003/005/006, IMP-F003~005.

## 9. Final Decision

**HOLD**

- Verified: `DBG-F004`, `SEC-F003`
- 부분 개선: `DBG-F005`, `SEC-F001`, `SEC-F002`, `IMP-F001`, `IMP-F002`
- 미해결: 기존 17건 + 신규 `SEC-F009` (Critical 1, Major 16, Minor 1)
- 차단 사유: Critical `SEC-F002`, High dependency advisories, 다수의 보안/정확성 Major finding

## 10. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_3.md`의 Re-audit #1 결과를 기준으로 수정하세요.
먼저 SEC-F002와 SEC-F009를 처리해 개인키 block 전체, 한 줄 복수 secret, github_pat_, JSON/YAML secret, Unicode 인접 key가 원문 0건·panic 0건임을 회귀 테스트로 고정하세요.
그 다음 SEC-F001의 approved request binding과 app-level fail-closed 테스트, IMP-F001의 baseline ID 보존 추적표를 처리하세요.
수정 후 formatter, strict Clippy, workspace tests, locked release build, cargo audit 결과를 기록하세요.
```
