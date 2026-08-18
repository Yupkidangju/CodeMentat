# D3D 재감사 보고서 (Turn 5 / Re-audit #3)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_4.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

이번 수정으로 `SEC-F001`의 actual prompt 재해시, 승인 profile 직접 소비, private fields, consume-once API가 연결되었고 `SEC-F010`의 Unicode lowercase offset 혼용도 제거됐다. `spec.md`도 baseline FR/NFR/CON 문장과 ID를 원문 기준으로 복구하여 `IMP-F001`의 authority/ID 보존 문제는 해소됐다.

그러나 traceability matrix의 구현 상태와 증거가 실제 코드보다 강하다. UI blocking, unbounded file reads, HTTP timeout 미적용, 저장 복원 부재 등을 `Implemented`로 표시하고 존재하지 않는 타입/오류 이름을 evidence로 제시한다. 공급망 High 2건과 기존 repository/UI/security Major finding도 유지되므로 전체 판정은 `HOLD`다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 24 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | **24 tests** 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 경고 2건 |
| `git rev-parse --show-toplevel` | FAIL — 프로젝트가 아닌 `C:/` |

`IMPLEMENTATION_SUMMARY.md`의 23 passed 주장은 실제 24개와 다르다.

## 3. Finding 재판정표

| Finding | Re-audit #3 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | **Verified** | baseline authority, ID, 원문 요구사항 복구 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major) | viewport/theme/global shortcut 미수정 |
| IMP-F004 | Needs Fix (Major) | `/where`, conflict, cloud AnswerBundle 검증 미수정 |
| IMP-F005 | Needs Fix (Major) | profile/session 복원 및 멀티플랫폼 CI 미수정 |
| IMP-F006 | 신규 Needs Fix (Major) | traceability 상태/evidence가 실제 구현을 과대 표시 |
| DBG-F001 | Needs Fix (Major) | UI blocking receive 유지 |
| DBG-F002 | Needs Fix (Major) | snapshot/watcher/evidence 일관성 미수정 |
| DBG-F003 | Needs Fix (Major) | unbounded full-file reads 유지 |
| DBG-F004 | **Verified** | semaphore permit 반환 회귀 유지 |
| DBG-F005 | Needs Fix (Major) | 테스트 수 증가, 실제 app/network failure coverage 부족 |
| DBG-F006 | Needs Fix (Major) | Git root `C:/`, `Cargo.lock` ignore 유지 |
| SEC-F001 | **Verified** | prompt 재해시, approved profile 직접 소비, private consume-once |
| SEC-F002 | Needs Fix (Major) | high-entropy/query-aware/exact preview 및 escaped-value 처리 미완료 |
| SEC-F003 | **Verified** | AppData 격리 강제 유지 |
| SEC-F004 | Needs Fix (Major) | plaintext key/Gemini URL key/custom HTTP 미수정 |
| SEC-F005 | Needs Fix (Major) | HTTP 전체 timeout/cancel 미수정 |
| SEC-F006 | Needs Fix (Major) | scan symlink canonical guard 미수정 |
| SEC-F007 | Needs Fix (Major) | RustSec High 2건 유지 |
| SEC-F008 | Needs Fix (Major) | R/O badge 항상 true |
| SEC-F009 | **Verified** | Unicode token range 회귀 유지 |
| SEC-F010 | **Verified** | 원문 byte offset 기반 ASCII-insensitive assignment parser |

## 4. 상세 재감사

### [IMP-F001] Re-audit #3 — authority와 baseline ID 보존 해소

- Pass: Implementation
- Pattern: `IMP-004`, `SPEC-GAP-001`
- Area: requirements authority
- Severity: Major
- Status: Verified
- Modified Files: `spec.md`, `CHANGELOG.md`, `IMPLEMENTATION_SUMMARY.md`
- Evidence:
  - `CODE_MENTAT_SPEC.md`가 master baseline이고 `spec.md`가 active tracking 문서라는 관계가 명시됐다.
  - FR-001~026, NFR-001~013, CON-001~008의 baseline 문장과 ID가 원문 의미로 복구됐다.
  - Partial 상태도 일부 구분됐다.
- Expected / Actual: authority와 ID 의미 보존 — 일치.
- Remaining Risk: 구현 상태와 evidence 정확성은 신규 `IMP-F006`으로 분리한다.
- Suggested Fix: 없음.
- Re-audit Method: baseline 문장/ID diff를 계속 유지한다.
- Owner: Auditor

### [SEC-F001] Re-audit #3 — approved payload/profile consume-once 계약 해소

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: immutable consent request
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-analysis/src/egress.rs`, `crates/mentat-app/src/app.rs`, 관련 Cargo manifests
- Evidence:
  - constructor와 `verify_integrity()`가 `packet.prompt_context`에서 SHA-256을 다시 계산한다.
  - receipt packet hash, snapshot, file count, token count를 actual packet과 대조한다.
  - `ApprovedInferenceRequest` 필드는 private이며 approved `BackendProfile`을 소유한다.
  - `into_inference_request(self)`가 승인된 profile/prompt/question을 직접 이동해 단일 소비한다.
  - app stream 경로는 더 이상 현재 `self.profile`로 request를 재구성하지 않는다.
  - tampered prompt 및 consume-once 회귀 테스트가 통과한다.
- Expected / Actual: 승인된 exact payload/profile을 재조립 없이 한 번만 전송 — 일치.
- Remaining Risk: 실제 network mock 호출 수 검증은 `DBG-F005`에 남긴다.
- Suggested Fix: 없음.
- Re-audit Method: 현재 integrity/consume-once tests와 향후 app network mock을 유지한다.
- Owner: Auditor

### [SEC-F010] Re-audit #3 — Unicode lowercase offset 혼용 해소

- Pass: Security
- Pattern: `SEC-001`
- Area: Unicode-safe assignment redaction
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-analysis/src/egress.rs`
- Evidence:
  - `find_ascii_case_insensitive()`가 원문 UTF-8의 char boundary에서 ASCII byte 비교를 수행한다.
  - 변환된 lowercase 문자열의 offset을 원문에 재사용하지 않는다.
  - Turkish dotted I 및 emoji 인접 assignment 테스트가 통과한다.
- Expected / Actual: 원문 byte offset 정렬과 무패닉 redaction — 일치.
- Remaining Risk: escaped structured value 처리는 `SEC-F002`에 남긴다.
- Suggested Fix: 없음.
- Re-audit Method: 현재 Unicode assignment 회귀 테스트 유지.
- Owner: Auditor

### [IMP-F006] 신규 — traceability matrix의 Implemented/evidence 판정이 실제 코드와 다르다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: completion claims, verification authority
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `spec.md` FR-006은 resource limit을 Implemented로 표시하지만 scanner는 파일 전체를 `read_to_end`하고 개별/총량/파일수 상한이 없다.
  - NFR-002 UI p95 100ms를 Implemented로 표시하지만 `app.rs`에 300ms~4초 `recv_timeout`이 남아 있다.
  - NFR-003 비고는 chunk 읽기를 주장하지만 `scanner.rs`와 `session.rs`는 전체 파일 읽기다.
  - NFR-008 hard timeout을 Implemented로 표시하지만 `timeout_secs`는 adapter에서 사용되지 않는다.
  - FR-015 evidence로 `MentatError::InferenceError`를 제시하지만 해당 variant가 없다.
  - FR-012 evidence의 `RecommendationKind`는 실제 타입 `RecommendationBasis`와 다르다.
  - FR-023은 session/settings 복원을 Implemented로 표시하지만 storage/app은 recent repository만 복원한다.
  - CON-007은 API key 비저장을 Implemented로 표시하지만 `BackendProfile`은 plaintext `api_key`를 Clone/Serialize/Debug 가능한 필드로 소유한다.
- Expected: acceptance criterion 전체를 재현하는 증거가 있을 때만 Implemented로 표시한다.
- Actual: 타입/함수 존재 또는 부분 골격을 기능 완료로 승격한다.
- Impact: 남은 finding이 추적표에서 숨겨지고 잘못된 Phase gate가 형성된다.
- Suggested Fix: 각 baseline 항목을 `Implemented / Partial / Missing / Deferred`로 재판정하고 acceptance criterion을 직접 실행하는 test/command만 evidence로 연결한다.
- Re-audit Method: traceability 각 행을 source+test+runtime evidence와 대조한다.
- Owner: Architect / Coder

### [SEC-F002] Re-audit #3 — 패턴 redaction은 개선됐지만 egress 최소화/일반 secret 경계는 미완료

- Pass: Security
- Pattern: `SEC-001`
- Area: secret detection, least-data egress
- Severity: Major
- Status: Needs Fix
- Evidence:
  - PEM, 복수 token, GitHub PAT, Unicode assignment tests는 통과한다.
  - 일반 고엔트로피 credential, AWS key/JWT/bearer token 탐지는 없다.
  - `assemble_packet()`은 질문 관련도를 계산하지 않고 첫 8개 문서/entrypoint를 선택한다.
  - consent UI는 exact included file/line 목록 대신 개수와 hash만 표시한다.
  - quoted assignment parser는 escaped quote를 인식하지 않는다. JSON 값에 `\"`가 있으면 첫 escaped quote에서 값을 끝낸 것으로 보고 뒤 secret 조각을 남길 수 있다.
  - long token은 max length까지만 치환하고 연속 suffix를 남길 수 있다.
- Expected: 전체 secret value 제거, query-aware 최소 문맥, exact preview.
- Actual: 알려진 단순 패턴에는 강해졌지만 일반/구조화 edge case와 least-data 정책은 닫히지 않았다.
- Impact: 일부 credential 또는 무관 파일 내용이 외부로 전송될 수 있다.
- Suggested Fix: high-entropy/known credential detector, escape-aware structured parser, token 전체 소비, query-aware retrieval, exact file/line preview를 추가한다.
- Re-audit Method: AWS/JWT/bearer, escaped JSON/YAML, oversized token, 질문 무관 파일, exact preview fixture를 검증한다.
- Owner: Coder / Security

### [DBG-F005] Re-audit #3 — 24개 테스트 중 production failure path 증거는 여전히 제한적이다

- Pass: Debug
- Pattern: `TEST-001`
- Area: test validity
- Severity: Major
- Status: Needs Fix
- Evidence:
  - 실제 테스트 수는 24개이며 문서는 23개로 기록했다.
  - app expansion test는 viewport command를 실행하지 않고 enum 값을 대입한다.
  - app egress test는 production stream/network path나 network-call count를 실행하지 않는다.
  - inference-openai test는 empty-key health check만 검증하며 SSE chunk split, timeout, cancellation, provider error mapping을 다루지 않는다.
- Expected: 테스트 이름이 주장하는 production boundary를 실제로 호출한다.
- Actual: constructor/type smoke가 app-level/adapter-level 보안 증거로 문서화된다.
- Impact: 회귀가 있어도 green test suite가 이를 놓칠 수 있다.
- Suggested Fix: mock backend와 request recorder를 주입해 network 호출 0/1건, approved profile, viewport command, timeout/cancel을 assertion한다.
- Re-audit Method: failure-specific integration tests가 production 호출 경로를 통과하는지 확인한다.
- Owner: Coder

### [SEC-F007] Re-audit #3 — 공급망 High 취약점 유지

- Pass: Security
- Pattern: `SEC-006`, `DEP-001`
- Area: supply chain
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `quick-xml 0.30.0`: `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`, CVSS 7.5 High.
  - `paste 1.0.15`, `ttf-parser 0.25.1` unmaintained 경고.
  - `cargo audit` exit 1.
- Expected: clean audit 또는 owner/expiry/reachability가 명시된 Accepted Risk.
- Actual: dependency lock 변경 후에도 advisory가 남아 있다.
- Impact: Linux accessibility XML 경로의 CPU/메모리 DoS 위험.
- Suggested Fix: 안전한 상위 dependency로 갱신하거나 target별 reachability/mitigation을 문서화한다.
- Re-audit Method: `cargo audit`와 all-target dependency tree 재실행.
- Owner: Coder / Security

## 5. 변경되지 않은 기존 Finding

- `IMP-F002` Minor: 문서 prerelease와 Cargo package version 불일치.
- `IMP-F003` Major: Tier viewport resize/theme/global shortcut 불일치.
- `IMP-F004` Major: `/where`, 실제 conflict 분석, cloud evidence validation 미완료.
- `IMP-F005` Major: profile/session 복원 및 멀티플랫폼 CI 부재.
- `DBG-F001` Major: UI의 300ms~4초 blocking receive.
- `DBG-F002` Major: snapshot double scan, shallow watcher, excerpt-only evidence hash.
- `DBG-F003` Major: unbounded full-file reads와 resource budget 부재.
- `DBG-F006` Major: Git root `C:/`, `Cargo.lock` ignore.
- `SEC-F004` Major: plaintext key, Gemini query-string key, unrestricted custom HTTP.
- `SEC-F005` Major: HTTP 전체 수명주기 timeout/cancel 부재.
- `SEC-F006` Major: scan symlink canonical guard 부재.
- `SEC-F008` Major: R/O badge 항상 true.

## 6. Cross-Pass Conflict

### [XPF-F005] 정확한 baseline 문장과 부정확한 완료 상태가 충돌한다

- Related Findings: IMP-F006, DBG-F001~003/005, SEC-F002/004/005
- Conflict: baseline acceptance 문장은 복구됐지만 동일 행의 Implemented/evidence가 실제 실패 경로를 통과하지 않는다.
- Resolution: status는 타입 존재가 아니라 acceptance criterion 재현 결과로 판정한다.
- Gate Impact: HOLD
- Required Fix Before PASS: traceability 상태 정정과 관련 failure-specific evidence.

## 7. 상태 집계

- Verified: `IMP-F001`, `DBG-F004`, `SEC-F001`, `SEC-F003`, `SEC-F009`, `SEC-F010`
- 미해결: Critical 0, Major 15, Minor 1
- 신규: `IMP-F006`
- 전체 판정: **HOLD**

## 8. 다음 수정 우선순위

1. `IMP-F006`: traceability의 실제 상태/evidence 정정.
2. `SEC-F002`: high-entropy, escape-aware structured parser, query-aware exact preview.
3. `SEC-F004~008`: credential, timeout, symlink, dependency, security badge.
4. `DBG-F001~003/005/006`: UI async, snapshot/watch, resource budgets, integration tests, Git.
5. `IMP-F003~005`: UI 계약, evidence workflow, persistence/CI.

## 9. Final Decision

**HOLD**

승인 request와 Unicode assignment 경계는 해소됐지만, 완료 상태 과대기록과 15개 Major finding이 수정 또는 Accepted Risk로 닫히지 않았다.

## 10. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_5.md`의 Re-audit #3 결과를 기준으로 수정하세요.
먼저 IMP-F006에서 baseline acceptance criterion별 실제 상태를 Partial/Missing으로 정정하고 존재하는 타입명이 아닌 재현 가능한 test/command만 evidence로 연결하세요.
다음으로 SEC-F002의 high-entropy/escaped structured value/query-aware exact preview를 구현하고, SEC-F004~008 및 DBG-F001~003/005/006을 순서대로 처리하세요.
수정 후 formatter, strict Clippy, workspace tests, locked release build, cargo audit를 재실행하세요.
```
