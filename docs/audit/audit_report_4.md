# D3D 재감사 보고서 (Turn 4 / Re-audit #2)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_3.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 재감사 요약

`SEC-F002`의 PEM 전체 마스킹, 한 줄 복수 secret, GitHub PAT, JSON/YAML assignment 검사가 추가됐고 `SEC-F009`의 고정 byte slicing은 Unicode-safe token scanner로 교체됐다. 포맷, Strict Clippy, 23개 테스트, Windows locked release build는 모두 통과한다.

다만 승인 request 계약은 실제 packet 내용과 backend profile을 강제하지 않으며, 새 assignment scanner는 Unicode lowercase 길이가 원문과 달라질 때 byte offset이 어긋나 secret redaction을 우회할 수 있다. Baseline 추적표도 ID를 보존한다고 선언하지만 여전히 원본 의미를 다수 재정의한다. RustSec High 2건과 기존 Major finding도 남아 있으므로 `HOLD`를 유지한다.

## 2. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 23 passed, 0 failed |
| `cargo test --workspace --locked -- --list` | 23 tests 확인 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 경고 2건 |
| `git rev-parse --show-toplevel` | FAIL — 프로젝트가 아닌 `C:/` |

## 3. Finding 재판정표

| Finding | Re-audit #2 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Needs Fix (Major) | Baseline ID 이름은 유지했지만 원본 의미를 여전히 재정의 |
| IMP-F002 | Needs Fix (Minor) | 문서 0.1.0-dev, Cargo 0.1.0 |
| IMP-F003 | Needs Fix (Major) | viewport/theme/global shortcut 미수정 |
| IMP-F004 | Needs Fix (Major) | `/where`, conflict, cloud AnswerBundle 검증 미수정 |
| IMP-F005 | Needs Fix (Major) | profile/session 복원 및 멀티플랫폼 CI 미수정 |
| DBG-F001 | Needs Fix (Major) | UI `recv_timeout` 유지 |
| DBG-F002 | Needs Fix (Major) | double scan/shallow watcher/evidence hash 유지 |
| DBG-F003 | Needs Fix (Major) | unbounded full-file reads 유지 |
| DBG-F004 | Verified | `OwnedSemaphorePermit` 회귀 테스트 유지 |
| DBG-F005 | Needs Fix (Major) | 게이트 통과, 하지만 app/adapter 테스트가 실제 실패모드를 실행하지 않음 |
| DBG-F006 | Needs Fix (Major) | Git root `C:/`, `Cargo.lock` ignore 유지 |
| SEC-F001 | Needs Fix (Major) | actual packet/profile binding 및 single-use 미강제 |
| SEC-F002 | Needs Fix (Major, Critical에서 하향) | 명시 패턴은 개선, high-entropy/query-aware/exact preview 미구현 |
| SEC-F003 | Verified | AppData 격리 강제 유지 |
| SEC-F004 | Needs Fix (Major) | plaintext key/Gemini URL key/HTTP 검증 미수정 |
| SEC-F005 | Needs Fix (Major) | 전체 HTTP 수명주기 timeout/cancel 미수정 |
| SEC-F006 | Needs Fix (Major) | scan symlink canonical guard 미수정 |
| SEC-F007 | Needs Fix (Major) | RustSec High 2건 유지 |
| SEC-F008 | Needs Fix (Major) | R/O badge 항상 true |
| SEC-F009 | Verified | Unicode-safe token boundary와 회귀 테스트 추가 |
| SEC-F010 | 신규 Needs Fix (Major) | lowercase-derived byte offset으로 assignment redaction 우회 가능 |

## 4. 상세 재감사

### [IMP-F001] Re-audit #2 — Baseline ID는 표기만 보존되고 의미는 여전히 바뀌었다

- Pass: Implementation
- Pattern: `IMP-004`, `SPEC-GAP-001`
- Area: requirements traceability
- Severity: Major
- Status: Needs Fix
- Modified Files: `spec.md`, `IMPLEMENTATION_SUMMARY.md`, `CHANGELOG.md`
- Evidence:
  - `spec.md`는 baseline 정의를 원본 그대로 보존한다고 선언한다.
  - baseline `FR-004`는 “저장소 안에서 셸·프로세스·빌드·테스트 실행 금지”지만 active matrix는 “파일 시스템 감시자 및 STALE 상태 전이”로 기록한다.
  - baseline `FR-005`는 파일 트리/내용 탐색이지만 active matrix는 기술스택 탐지다.
  - baseline NFR-002는 UI p95 100ms 반응성이지만 active matrix는 AppData 격리다.
  - baseline NFR-003은 100,000파일/2GiB 성능 기준이지만 active matrix는 Egress Privacy다.
  - baseline CON-004는 모델 출력과 저장소 증거 분리지만 active matrix는 egress 동의다.
  - `FR-021` 증거로 든 `test_app_expansion_tier_transitions`는 앱/viewport를 실행하지 않고 enum 변수에 값을 순서대로 대입한다.
- Expected: baseline 문장을 변경하지 않고 구현 상태와 evidence만 옆에 연결해야 한다.
- Actual: 동일 ID에 다른 구현 기능을 다시 배정했다.
- Impact: 미구현 baseline 요구를 Implemented로 오인하고 Phase gate를 잘못 통과시킨다.
- Suggested Fix: `CODE_MENTAT_SPEC.md`의 FR/NFR/CON 표를 그대로 가져와 의미를 고정하고, 현재 기능은 정확한 원 ID 또는 새 extension ID에 매핑한다.
- Re-audit Method: baseline과 active matrix의 문장을 ID별로 기계적 diff한다.
- Owner: Architect / Coder

### [SEC-F001] Re-audit #2 — ApprovedInferenceRequest가 승인한 payload/profile을 실제 전송에 강제하지 않는다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: immutable consent, request binding
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-analysis/src/egress.rs`, `crates/mentat-app/src/app.rs`
- Evidence:
  - `ApprovedInferenceRequest::new()`은 receipt의 `packet_hash`와 packet의 문자열 필드만 비교하며 `packet.prompt_context`에서 SHA-256을 다시 계산하지 않는다.
  - 따라서 승인 후 `packet.prompt_context`를 변경하고 기존 `packet_hash`를 유지해도 `verify_integrity()`는 true가 될 수 있다.
  - receipt의 `token_count`, `file_count`, `granted_at`도 검증되지 않는다.
  - 승인 digest는 provider/model을 포함하지만 `app.rs:364-370`은 승인된 provider/model이 아니라 현재 `self.profile.clone()`을 실제 요청에 사용한다.
  - `ApprovedInferenceRequest`의 필드와 `calculate_digest()`가 public이고 single-use 소비 상태가 없다.
  - `test_tampered_egress_request_rejection_fail_closed`는 앱이나 네트워크를 실행하지 않고 request의 question 필드만 바꿔 `verify_integrity()`를 호출한다.
- Expected: 승인 시점의 exact payload, snapshot, provider, model, query, included refs가 실제 request 객체 자체로 봉인되고 한 번만 소비되어야 한다.
- Actual: 승인 metadata와 실제 backend profile/prompt가 분리되어 있다.
- Impact: 향후 호출 확장이나 상태 변경에서 승인과 다른 데이터/공급자로 전송될 수 있다.
- Suggested Fix: packet hash를 actual prompt에서 매 검증 시 재계산하고, approved request가 immutable BackendProfile의 non-secret fields와 final InferenceRequest payload를 직접 소유하게 한다. 필드를 private으로 만들고 consume(self) API로 단일 사용을 강제한다.
- Re-audit Method: prompt/provider/model/file-count 변조, digest 재계산 시도, receipt 재사용을 모두 거부하고 network mock 호출 0건을 확인한다.
- Owner: Coder / Security

### [SEC-F002] Re-audit #2 — 명시 secret 회귀는 개선됐으나 전체 egress 정책은 아직 부분 구현이다

- Pass: Security
- Pattern: `SEC-001`
- Area: content secret filtering, least-data egress
- Severity: Major (Critical에서 하향)
- Status: Needs Fix
- Modified Files: `crates/mentat-analysis/src/egress.rs`
- Evidence:
  - PEM block 전체, 복수 key, `github_pat_`, `ghp_`, JSON/YAML/env assignment 회귀 테스트가 통과한다.
  - token scanner는 Unicode char boundary를 사용한다.
  - 하지만 baseline 보안 통제의 고엔트로피 문자열 탐지는 구현되지 않았다. AWS access key/JWT/임의 bearer token 등은 이름이나 assignment 문맥이 없으면 그대로 남는다.
  - `assemble_packet()`은 user question을 retrieval 기준으로 사용하지 않고 첫 8개 문서/entrypoint를 선택한다.
  - consent UI는 포함 파일의 이름과 행 범위 대신 개수와 hash만 표시한다.
- Expected: 질문 관련 최소 문맥, content/high-entropy scan, exact included refs, 사용자 제외 정책을 함께 적용한다.
- Actual: 지정 패턴 redaction은 개선됐지만 least-data와 전체 secret boundary는 닫히지 않았다.
- Impact: 사용자가 확인하지 못한 무관 파일이나 미등록 secret 형식이 외부로 전송될 수 있다.
- Suggested Fix: high-entropy/known credential detectors, query-aware selection, exact file/line preview, per-request exclusions를 추가한다.
- Re-audit Method: AWS/JWT/random bearer, 질문 무관 파일, 사용자 제외, 포함 행 preview fixture를 검증한다.
- Owner: Coder / Security

### [SEC-F009] Re-audit #2 — 고정 byte range panic 해소

- Pass: Security
- Pattern: `SEC-001`
- Area: Unicode-safe token scanning
- Severity: Major
- Status: Verified
- Modified Files: `crates/mentat-analysis/src/egress.rs`
- Evidence:
  - token boundary가 `char_indices()`와 ASCII token 조건으로 계산된다.
  - Unicode 앞/뒤 및 malformed short key 테스트가 통과한다.
- Expected / Actual: token redaction range가 항상 char boundary — 일치.
- Remaining Risk: assignment scanner는 별도 SEC-F010으로 분리한다.
- Suggested Fix: 없음.
- Re-audit Method: 현재 Unicode token 회귀 테스트 유지.
- Owner: Auditor

### [SEC-F010] 신규 — Unicode lowercase 변환이 assignment scanner의 원문 byte offset을 깨뜨린다

- Pass: Security
- Pattern: `SEC-001`
- Area: Unicode-safe assignment parsing
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `egress.rs:330-348`은 `current.to_lowercase()`에서 sensitive key 위치를 찾는다.
  - Unicode lowercase는 원문과 byte 길이가 다를 수 있지만 찾은 `pos/k_end`를 원문 `current` slicing에 그대로 사용한다.
  - 예를 들어 원문 `İ password="secret"`에서 `İ`의 lowercase가 여러 code point로 확장되어 `password` offset이 이동한다. scanner는 separator를 놓쳐 secret 값을 그대로 남길 수 있다.
  - 일부 조합에서는 잘못된 byte 위치가 char boundary가 아니어서 slicing panic 가능성도 있다.
  - 현재 Unicode 테스트는 token prefix만 검사하고 assignment 앞 Unicode casing 변화는 다루지 않는다.
- Expected: case-insensitive 탐지가 원문 byte range와 정확히 대응하고 모든 Unicode 입력에서 panic 없이 secret을 제거해야 한다.
- Actual: 변환된 문자열의 offset과 원문 offset을 혼용한다.
- Impact: repository 텍스트로 assignment redaction 우회 또는 egress 조립 실패를 유도할 수 있다.
- Suggested Fix: 원문에서 ASCII-insensitive key parser를 수행하거나 byte-position mapping을 유지한다. JSON은 실제 parser, YAML/env는 경계가 명확한 scanner를 사용한다.
- Re-audit Method: `İ password`, case variation, combining marks, escaped JSON quote, malformed Unicode 인접 assignment fixture/proptest를 실행한다.
- Owner: Coder / Security

### [DBG-F005] Re-audit #2 — 테스트 수는 증가했지만 이름이 보장 범위를 과장한다

- Pass: Debug
- Pattern: `TEST-001`
- Area: regression-test validity
- Severity: Major
- Status: Needs Fix
- Modified Files: `crates/mentat-app/src/app.rs`, `crates/mentat-inference-openai/src/lib.rs`, `IMPLEMENTATION_SUMMARY.md`
- Evidence:
  - 총 23개 테스트는 통과한다.
  - `test_app_expansion_tier_transitions`는 실제 MentatApp/viewport/event를 만들지 않고 enum 변수에 세 값을 대입한다.
  - `test_tampered_egress_request_rejection_fail_closed`는 `start_inference_stream_with_approved_request()`나 network mock을 실행하지 않는다.
  - inference-openai 테스트는 empty API key health-check만 검사하고 SSE chunking, timeout, cancellation, error mapping을 검사하지 않는다.
- Expected: 테스트 이름이 주장하는 실제 경계와 실패모드를 호출해야 한다.
- Actual: 타입/생성자 단위 smoke를 app-level fail-closed와 UI transition 증거로 문서화했다.
- Impact: 핵심 회귀가 발생해도 문서상 23개 PASS가 이를 가린다.
- Suggested Fix: app orchestration을 주입 가능한 backend/channel/state reducer로 분리하고 실제 network-call count와 viewport command를 assertion한다.
- Re-audit Method: failure-specific integration test가 production 호출 경로를 통과하는지 확인한다.
- Owner: Coder

### [SEC-F007] Re-audit #2 — dependency audit 실패 유지

- Pass: Security
- Pattern: `SEC-006`, `DEP-001`
- Area: supply chain
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `quick-xml 0.30.0`: `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`, CVSS 7.5 High.
  - `paste 1.0.15`, `ttf-parser 0.25.1` unmaintained 경고.
  - `Cargo.lock` 및 UI dependency path가 이번 수정에서 바뀌지 않았다.
- Expected: clean audit 또는 owner/expiry/reachability가 명시된 Accepted Risk.
- Actual: audit command exit 1.
- Impact: Linux accessibility XML 경로의 CPU/메모리 DoS 위험.
- Suggested Fix: 안전한 상위 dependency로 갱신하거나 target별 reachability와 mitigation을 문서화한다.
- Re-audit Method: `cargo audit`와 all-target dependency tree 재실행.
- Owner: Coder / Security

## 5. 변경되지 않은 기존 Finding

다음 finding은 관련 구현이 변경되지 않아 이전 판정을 유지한다.

- `IMP-F002` Minor: 문서 prerelease와 Cargo package version 불일치.
- `IMP-F003` Major: Tier viewport resize/theme/global shortcut 불일치.
- `IMP-F004` Major: `/where`, conflict, cloud evidence validation 미완료.
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

### [XPF-F004] 구현 완료 추적표와 실제 검증 범위가 충돌한다

- Related Findings: IMP-F001, DBG-F005, SEC-F001, SEC-F002
- Conflict: `spec.md`와 구현 요약은 다수 항목을 Implemented/cryptographically sealed로 선언하지만 production app/network 경계와 baseline 의미가 검증되지 않았다.
- Resolution: 테스트 이름과 문서 선언보다 실제 호출 경로 및 baseline 원문을 우선한다.
- Gate Impact: HOLD
- Required Fix Before PASS: traceability correction, approved payload/profile enforcement, failure-specific integration tests.

## 7. 상태 집계

- Verified: `DBG-F004`, `SEC-F003`, `SEC-F009`
- 미해결: Critical 0, Major 17, Minor 1
- 신규: `SEC-F010`
- 전체 판정: **HOLD** — Major finding이 수정 또는 명시적 Accepted Risk로 닫히지 않음

## 8. 다음 수정 우선순위

1. `SEC-F001`: actual prompt hash 재검산, approved profile 직접 소비, private fields, consume-once API.
2. `SEC-F010`: 원문 offset을 보존하는 Unicode-safe assignment parser.
3. `IMP-F001`: baseline 원문을 그대로 보존한 추적 매트릭스.
4. `SEC-F002`: high-entropy/query-aware/exact preview 정책.
5. `SEC-F004~008`, `DBG-F001~003/005/006`, `IMP-F003~005`.

## 9. Final Decision

**HOLD**

Critical secret block 누출과 고정 byte panic은 해소됐지만, egress 승인 계약 자체와 공급망, baseline 추적성, repository/UI 정확성 finding이 아직 닫히지 않았다.

## 10. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_4.md`의 Re-audit #2 결과를 기준으로 수정하세요.
먼저 SEC-F001에서 packet.prompt_context를 다시 해시하고 승인된 provider/model/profile을 실제 InferenceRequest에 직접 소비하며 필드를 private + consume-once로 봉인하세요.
SEC-F010의 Unicode lowercase offset 우회 테스트를 추가하고, IMP-F001은 CODE_MENTAT_SPEC.md의 baseline 문장을 ID별로 그대로 보존해 다시 매핑하세요.
그 후 high-entropy/query-aware egress와 실제 app/network failure-specific 테스트를 처리하고 전체 품질 게이트와 cargo audit를 재실행하세요.
```
