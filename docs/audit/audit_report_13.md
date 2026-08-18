# D3D 재감사 보고서 (Turn 13 / Re-audit #11)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_12.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `5924821e4fb4699a71a13f8e563f88fe2b945b4b`
- 감사 대상: 기준 commit과 그 위의 **미커밋 working tree**
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. Audit Scope

### 확인 범위

- 프로젝트: Rust 10-crate Windows desktop workspace
- 주요 문서: `CODE_MENTAT_SPEC.md`, `spec.md`, `designs.md`, `README.md`, `IMPLEMENTATION_SUMMARY.md`, `DESIGN_DECISIONS.md`, `CHANGELOG.md`, `BUILD_GUIDE.md`, `audit_roadmap.md`, `audit_report_12.md`
- 주요 소스:
  - `mentat-analysis`: AnswerBundle, consent, egress, semantic workflow
  - `mentat-app`: repository scan, provider setup, consent, streaming, shortcuts, theme
  - `mentat-repository`: scanner, session, watcher, regression/benchmark tests
  - `mentat-inference`: provider/profile/model catalog contracts
  - `mentat-inference-openai`: Gemini/OpenAI discovery, verification, inference adapters
  - `mentat-storage`: profile/snapshot persistence
- 검증 축: Implementation Compliance, Debug/Engineering Quality, Security/Privacy, Performance
- 기준 상태: `master`가 `origin/master`보다 1 commit 앞서며, 추가 수정·신규 파일이 남은 DIRTY working tree

### 제외 범위

- 실제 OpenAI/Gemini 계정과 실 API key를 사용하는 외부 통합 시험: 자격 증명 미제공
- Linux/macOS 패키지 실행 시험: 현재 Windows 환경
- 네이티브 GUI 육안 QA 및 실제 OS global hotkey lifecycle: 자동화/수동 runtime fixture 부재
- 실제 2GiB corpus 메모리 측정: 제공된 ignored test가 2GiB 데이터를 만들지 않음
- `target`, `.git`, Cargo registry 전체: 생성물/외부 의존성. 단, reqwest redirect 경계 확인에 설치된 `reqwest 0.12.28`의 해당 소스만 제한적으로 대조

## 2. 재감사 요약

`IMP-F002` 버전 설명, `SEC-F002` 사용자 질문 마스킹·단일 전송, cloud AnswerBundle schema/snapshot/hash 제공, 앱 `ScanOutcome`/취소/omission UI, viewport/theme 문서 동기화는 확인됐다. Formatter, strict Clippy, 81개 기본 tests, 1개 별도 ignored test, Windows locked release build도 통과한다.

그러나 `SEC-F001` 승인 봉인이 새 `redacted_user_question`과 citation validation 자료를 해시에 포함하지 않아 승인 후 데이터 교체를 거부하지 못하는 Critical 회귀가 생겼다. 또한 `spec.md`가 1:1 보존을 선언하면서 master baseline의 FR-013/FR-017 원문을 다른 요구사항으로 교체했다. 취소된 부분 scan도 Ready snapshot으로 저장·분석되고, watcher는 각 파일의 첫 8KiB만 재해시하며 100,000파일 전수 open을 3초 간격으로 반복한다. Global hotkey는 등록되지 않아 창 내부 hide 후 self-unhide가 불가능하다. `audit_roadmap.md`는 이 상태에서도 전 Phase PASS와 불완전한 Accepted Risk를 주장한다.

상태 집계는 **Critical 1, Major 7, Minor 0, Accepted Risk 1**이며 전체 판정은 **HOLD**다.

## 3. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 81 passed, 0 failed, 1 ignored |
| `cargo test --workspace --locked -- --list` | 82 tests 확인 |
| `cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture` | PASS — 84.58s, 단 실제 fixture는 100,000×3 bytes |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `git diff --check` | PASS — whitespace error 없음, LF→CRLF 경고 존재 |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 2건 |
| `git status --short --branch` | DIRTY — 수정 18개 경로, 신규 provider/font/audit 파일 |

`cargo audit` 결과:

- `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`: `quick-xml 0.30.0`, CVSS 7.5 High.
- `RUSTSEC-2024-0436`: `paste 1.0.15` unmaintained.
- `RUSTSEC-2026-0192`: `ttf-parser 0.25.1` unmaintained.
- `DEC-SEC-004`의 Windows 비도달 분석, owner `@Yupkidangju`, expiry `2026-11-30`, upstream update trigger를 근거로 `SEC-F007` Accepted Risk를 유지한다.

## 4. Finding 재판정표

| Finding | Re-audit #11 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | **Needs Fix (Major, 회귀)** | baseline 1:1 보존 선언과 달리 FR-013/FR-017 원문 교체 |
| IMP-F002 | **Verified** | Cargo 0.1.0과 문서 0.1.0-dev 관계 명시 |
| IMP-F003 | Needs Fix (Major, 부분 개선) | size/theme 정렬, OS global hotkey 미등록 및 hide 복귀 불가 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | cloud 계약 연결, 무증거 Observed claim 검증 누락 |
| IMP-F005 | Verified | stable repo/profile/snapshot 복원 유지 |
| IMP-F006 | **Needs Fix (Major, 회귀)** | audit roadmap 전 Phase PASS 및 무효 Accepted Risk 과대 주장 |
| DBG-F001 | Verified | UI blocking receive 제거 유지 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | 첫 8KiB fingerprint와 지속 전수 I/O |
| DBG-F003 | Needs Fix (Major, 부분 개선) | app cancel 연결, cancelled partial snapshot Ready 처리와 2GiB/memory 증거 미완료 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | Verified | formatter/Clippy/build/wire tests 통과 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | Verified | async terminal 처리 유지 |
| DBG-F008 | Verified | watcher UI thread 비차단 유지 |
| SEC-F001 | **Needs Fix (Critical, 회귀)** | 새 outbound/validation fields가 승인 hash에 결속되지 않음 |
| SEC-F002 | **Verified** | question redaction, single canonical wire copy, zero-leak tests |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | **Needs Fix (Major, 회귀)** | Gemini custom key header가 cross-host redirect에서 제거되지 않음 |
| SEC-F005 | Verified | timeout/cancel/SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review trigger 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 처리 유지 |
| SEC-F010 | Verified | Unicode assignment 처리 유지 |
| SEC-F011 | Verified | consent generation guard 유지 |

## 5. Pass 1: Implementation Compliance Findings

### [IMP-F001] Re-audit #11 — master baseline 원문이 다른 요구사항으로 교체됐다

- Pass: Implementation
- Pattern: `IMP-001`, `IMP-004`
- Area: specification authority
- Severity: Major
- Status: Needs Fix
- Summary: `spec.md`는 baseline 정의·수용 기준·ID를 1:1 원본 보존한다고 선언하지만 FR-013과 FR-017을 새로운 provider workflow 요구사항으로 덮어썼다.
- Evidence:
  - `spec.md:7`, `spec.md:17`은 `CODE_MENTAT_SPEC.md`의 baseline 원문을 1:1 보존한다고 선언한다.
  - `CODE_MENTAT_SPEC.md:154`의 FR-013은 base URL, protocol, model, optional headers, timeout 저장과 구조화된 connection test를 요구한다.
  - 현재 `spec.md:31`은 이를 dynamic model discovery/verification/activation 요구로 교체하고 `Implemented`로 표시한다.
  - `CODE_MENTAT_SPEC.md:158`의 FR-017 acceptance는 fake와 OpenAI backend의 동일 계약 및 profile 교체를 요구하지만 `spec.md:35`는 cloud/local discovery workflow로 교체한다.
  - 현재 `BackendProfile`에는 protocol과 optional headers가 없고 UI에서도 구성할 수 없으므로 원 FR-013 acceptance는 완료되지 않았다.
- Expected: baseline 행은 원문 그대로 유지하고 신규 provider state machine은 별도 derived ID/section으로 추가한다.
- Actual: baseline ID를 유지한 채 요구사항 의미와 acceptance를 변경해 원 요구사항 누락을 가린다.
- Impact: 마스터 요구사항 추적성이 깨지고 구현 완료 판정이 다른 제품 계약을 기준으로 이뤄진다.
- Suggested Fix: FR-013/FR-017 원문을 복구하고 현재 provider activation 요구는 `DR-FR-*` 또는 별도 보강 acceptance로 분리한다. 원 FR-013의 protocol/optional-header/config persistence 상태를 실제 구현에 맞게 Partial로 판정한다.
- Re-audit Method: `CODE_MENTAT_SPEC.md` FR/NFR/CON과 `spec.md` baseline 열을 자동 또는 수동 1:1 diff하고, derived section이 baseline을 변경하지 않는지 확인한다.
- Owner: Architect / Coder

### [IMP-F003] Re-audit #11 — 화면·토큰은 정렬됐지만 global shortcut이 아니다

- Pass: Implementation
- Pattern: `IMP-003`
- Area: UI shortcut contract
- Severity: Major
- Status: Needs Fix
- Summary: Tier size/theme token은 구현 기준으로 문서화됐으나 README/designs가 약속한 OS global shortcut 등록이 없다.
- Evidence:
  - viewport constants와 theme colors는 `designs.md`와 현재 코드가 일치한다.
  - `Cargo.toml`에는 `global-hotkey`가 있으나 앱 소스에서 `GlobalHotKeyManager`, `HotKey`, event receiver 사용이 전혀 없다.
  - `app.rs:847-872`는 egui가 포커스된 창에 전달한 key event만 처리한다.
  - `Alt+Space` 또는 `Ctrl+Alt+M` 입력 시 `ViewportCommand::Visible(false)`로 창을 숨긴다. 숨은 비포커스 창은 다시 egui key event를 받을 수 없어 같은 단축키로 복귀할 수 없다.
  - README/designs는 OS global 등록과 실패 시 focused fallback을 기술한다.
- Expected: OS global hotkey lifecycle이 등록·충돌 처리·event polling·해제까지 연결되거나, 숨김 동작을 제거하고 실제 지원 범위를 문서화한다.
- Actual: focused-window shortcut만 있고 hide 후 self-unhide 경로가 없다.
- Impact: 사용자가 단축키로 창을 숨긴 뒤 앱을 다시 표시하지 못할 수 있으며 사용자 기능 약속이 성립하지 않는다.
- Suggested Fix: `global-hotkey`를 실제 연결하고 registration failure를 명시적으로 표시한다. 전역 등록이 불가한 모드에서는 `Visible(false)`를 실행하지 말고 안전한 최소화/접기 fallback을 사용한다.
- Re-audit Method: Windows에서 register→hide→global event→show→unregister smoke와 collision/failure fixture를 수행한다.
- Owner: Coder / Designer

### [IMP-F004] Re-audit #11 — cloud 계약은 연결됐지만 무증거 Observed를 신뢰한다

- Pass: Implementation
- Pattern: `IMP-003`, evidence trust boundary
- Area: AnswerBundle validation
- Severity: Major
- Status: Needs Fix
- Summary: 모델에 JSON schema/snapshot/hash/range를 제공하고 실제 adapter→normalizer fixture를 추가했지만, validator는 `Observed` claim의 evidence 목록이 비어 있어도 강등하지 않는다.
- Evidence:
  - `AnswerBundleNormalizer::system_contract()`와 egress citation catalog가 snapshot ID, content hash, allowed range를 제공한다.
  - 앱은 `from_model_text_with_contents()`와 included file text를 사용한다.
  - `validate_citations()`의 `has_invalid`는 `claim.evidence_ids.iter().any(...)`다. 빈 목록은 false이며 추가 invariant 검사가 없다.
  - 따라서 untrusted model이 `classification: Observed`, `evidence_ids: []`를 반환하면 Observed 상태가 유지된다.
  - 현재 loopback test는 valid citation과 wrong hash만 다루고 empty evidence/high confidence bounds를 다루지 않는다.
- Expected: 최소한 Observed/Conflict처럼 저장소 사실을 주장하는 classification은 하나 이상의 유효 EvidenceRef를 요구하고, 없으면 Unknown으로 강등해야 한다.
- Actual: schema 지시를 위반한 모델 출력의 무증거 사실 주장이 validation을 통과한다.
- Impact: 근거 기반 조언자의 핵심 신뢰 경계가 모델 지시 준수에 의존한다.
- Suggested Fix: classification별 evidence cardinality/invariant와 confidence 0..1 범위를 validator에서 강제하고 empty/missing/duplicate evidence regression tests를 추가한다.
- Re-audit Method: Observed empty evidence, missing ID, duplicate ID, invalid confidence를 adapter loopback 경로로 검증한다.
- Owner: Coder

### [IMP-F006] Re-audit #11 — audit roadmap이 현재 gate를 과대 표시한다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: phase/traceability status
- Severity: Major
- Status: Needs Fix
- Summary: 활성 `spec.md`는 많은 항목을 Partial로 표시하지만 `audit_roadmap.md`는 Phase 1~5를 모두 PASS Verified로 주장한다.
- Evidence:
  - `audit_roadmap.md:24-54`의 다섯 Phase가 전부 PASS다.
  - 같은 문서는 Phase 5에서 전체 tests 완전 통과를 근거로 들지만 기본 suite는 1개 test를 ignored한다.
  - `audit_roadmap.md:58-63`은 100,000+ file memory를 Minor Accepted Risk로 적지만 owner, expiry, review trigger가 없다.
  - 실제 worst-case preview memory와 2GiB evidence가 없으며 본 감사의 Critical/Major findings와 충돌한다.
- Expected: roadmap gate는 최신 감사 결과와 동일한 status를 사용하고 Accepted Risk는 owner/reason/expiry/review 조건을 모두 갖춘다.
- Actual: 현재 구현보다 높은 PASS 상태와 무효 Accepted Risk가 최신 운영 기준처럼 남아 있다.
- Impact: 후속 작업자가 release/Phase 진행을 잘못 승인할 수 있다.
- Suggested Fix: roadmap의 Phase gates를 현재 finding에 맞춰 HOLD/Partial로 보정하고, memory risk는 수정하거나 공식 risk record로 승격한다.
- Re-audit Method: roadmap Phase별 evidence를 실제 test list, spec status, 최신 audit decision과 재대조한다.
- Owner: Architect / Coder

## 6. Pass 2: Debug / Engineering Quality Findings

### [DBG-F002] Re-audit #11 — watcher rehash는 파일 전체가 아니라 첫 8KiB만 본다

- Pass: Debug
- Pattern: `DBG-002`
- Area: stale detection / performance
- Severity: Major
- Status: Needs Fix
- Summary: preserved-mtime fixture는 통과하지만 content fingerprint가 각 파일의 첫 8KiB만 읽으므로 후반부 동일크기 변경을 놓친다.
- Evidence:
  - `watcher.rs:240-278`의 `compute_content_fingerprint()`는 파일마다 8192-byte buffer를 한 번만 `read()`한다.
  - path와 size가 같고 첫 8KiB가 같으면 그 이후 내용 변경은 fingerprint에 반영되지 않는다.
  - regression fixture는 4-byte 파일 `aaaa→bbbb`만 바꿔 prefix 변경만 검증한다.
  - worker는 1초마다 전체 metadata walk, 3초마다 모든 파일 open+8KiB read를 수행한다.
  - large-tree test는 UI `poll_changes()` latency만 측정하고 worker I/O·CPU·stop latency를 측정하지 않는다.
- Expected: snapshot의 의미 있는 content change를 놓치지 않으면서 representative tree에서 검증된 I/O budget을 지킨다.
- Actual: 검출 완전성과 resource budget을 동시에 충족하지 못한다.
- Impact: 8KiB 이후 변경이 기존 evidence를 최신처럼 보이게 할 수 있고 대형 저장소에서 지속적인 디스크 부하가 발생한다.
- Suggested Fix: OS watcher event를 primary로 사용하고 changed path만 full hash하거나, snapshot hash cache와 bounded incremental verification을 결합한다. worker 내부 stop token을 walk/read 사이에도 확인한다.
- Re-audit Method: 16KiB same-size file의 후반부만 변경하고 mtime 복원, 100k-tree bytes-read/CPU/open count/stop latency profile을 검증한다.
- Owner: Coder

### [DBG-F003] Re-audit #11 — 취소된 부분 scan이 Ready snapshot으로 사용된다

- Pass: Debug
- Pattern: `DBG-002`, `TEST-001`
- Area: scan lifecycle / memory budget
- Severity: Major
- Status: Needs Fix
- Summary: app cancellation과 omission UI는 연결됐지만 cancelled partial result를 정상 Ready snapshot으로 저장하고 분석에 사용하며, 대표 test는 2GiB와 memory를 검증하지 않는다.
- Evidence:
  - `app.rs:267-274`는 cancellation 여부와 무관하게 partial files로 `create_snapshot_from_files()`를 호출한다.
  - `app.rs:529-575`는 `outcome.cancelled`일 때 status text만 취소로 표시하고 summary/kernel/snapshot을 저장한다. 새 snapshot status는 `Ready`이며 DB에도 기록된다.
  - 이후 일반 query는 이 partial summary/snapshot을 정상 분석 근거로 사용할 수 있다.
  - 새 repository를 여는 경로는 기존 `scan_cancel`을 취소하기 전에 새 token으로 덮어쓸 수 있어 이전 scan이 receiver 없이 계속될 수 있다.
  - ignored `test_dbg_f003_100k_2gib_benchmark_profile`은 100,000개 파일에 각각 `b"//\n"` 3 bytes만 기록한다. 총 약 300KB이며 2GiB corpus가 아니다.
  - `FileRecord`는 text file마다 최대 16KiB preview를 보관한다. 100,000개가 모두 preview cap에 도달하면 payload만 약 1.526GiB이며 path/hash/Vec overhead는 별도다.
  - 84.58초 실행은 통과했지만 memory RSS/peak와 2GiB workload를 기록하지 않는다.
- Expected: cancellation은 snapshot을 `Cancelled/Incomplete`로 끝내고 저장·cloud/local analysis를 차단해야 한다. 대표 profile은 실제 size distribution과 peak memory를 측정해야 한다.
- Actual: 부분 scan이 Ready truth로 승격되고 benchmark 이름이 acceptance보다 훨씬 작은 fixture를 가리킨다.
- Impact: 누락된 repository를 완전한 것으로 분석해 잘못된 답을 만들 수 있고 대형 corpus에서 메모리 고갈 위험이 남는다.
- Suggested Fix: cancelled/limit-incomplete snapshot 상태와 query gate를 추가하고 repository switch 시 이전 scan을 먼저 cancel한다. Preview를 lazy/bounded global cache로 바꾸고 실제 2GiB synthetic/sparse corpus에서 peak RSS·시간·cancel latency를 기록한다.
- Re-audit Method: app-level mid-scan cancel 후 DB/query 차단, repository switch old-task termination, 100k/2GiB peak memory artifact를 검증한다.
- Owner: Coder

## 7. Pass 3: Security Findings

### [SEC-F001] Re-audit #11 — 승인 봉인이 새 outbound/validation fields를 결속하지 않는다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: egress consent integrity
- Severity: Critical
- Status: Needs Fix
- Summary: packet hash와 receipt는 `prompt_context`만 봉인하며 실제 outbound question과 citation validation source는 해시에 포함하지 않는다.
- Evidence:
  - `EgressPacket`에는 `redacted_user_question`과 `included_file_texts`가 추가됐다.
  - `egress.rs:69-72`, `egress.rs:141-156`, `egress.rs:779-782`는 `prompt_context` bytes만 SHA-256한다.
  - `ApprovedInferenceRequest::new()`는 packet의 `redacted_user_question`을 그대로 채택한 뒤 새 digest를 계산하므로, 동일 receipt와 prompt hash를 유지한 채 question을 교체한 packet도 생성자가 수락한다.
  - `included_file_texts`는 cloud response citation validity를 결정하지만 prompt/receipt hash에 결속되지 않고 별도 context 일치 검사도 없다.
  - packet snapshot ID, included file refs, redaction count도 canonical seal에 포함되지 않는다.
  - tamper tests는 `prompt_context` 변경만 검증한다.
- Expected: 사용자에게 표시·승인되고 outbound 또는 사후 evidence 판정에 영향을 주는 모든 필드가 하나의 canonical digest에 결속돼야 한다.
- Actual: 새 field는 승인 hash 밖에 있으며 생성자가 승인 이후 교체를 구분하지 못한다.
- Impact: 승인받지 않은 질문을 외부로 보내거나 조작된 citation source로 모델 주장을 Observed로 검증할 수 있어 consent/evidence hard boundary를 우회한다.
- Suggested Fix: canonical packet representation에 snapshot, prompt context, redacted question, included refs, hashes/ranges, validation text digest, provider endpoint/profile identity를 포함한다. Receipt와 consume-time verification이 같은 digest를 사용하도록 단일 함수로 통합한다.
- Re-audit Method: 같은 길이 question swap, included_file_texts swap, snapshot/ref/profile endpoint swap을 각각 거부하는 constructor 및 app integration tests.
- Owner: Coder / Security

### [SEC-F004] Re-audit #11 — Gemini API key가 cross-host redirect에 전달될 수 있다

- Pass: Security
- Pattern: `SEC-001`, network credential boundary
- Area: Gemini adapter redirects
- Severity: Major
- Status: Needs Fix
- Summary: Gemini는 custom `x-goog-api-key` header를 사용하지만 reqwest 기본 redirect policy는 이 header를 cross-host redirect에서 제거하지 않는다.
- Evidence:
  - `gemini_adapter.rs:58`은 redirect policy를 지정하지 않은 `reqwest::Client::builder()`를 사용한다.
  - discovery, verification, health, inference 요청은 `x-goog-api-key`를 header에 넣는다.
  - 설치된 `reqwest 0.12.28`은 기본 10-hop redirect policy를 사용한다.
  - 해당 reqwest `redirect.rs:239-249`는 cross-host에서 `Authorization`, `Cookie`, `Proxy-Authorization`, `WWW-Authenticate`만 제거하며 `x-goog-api-key`는 제거하지 않는다.
  - cross-host redirect zero-leak regression test가 없다.
- Expected: secret-bearing requests는 redirect를 거부하거나 동일 origin만 허용하고, origin 변경 시 모든 custom credential headers를 제거해야 한다.
- Actual: endpoint가 3xx를 반환하면 custom Gemini key가 다음 host로 전달될 수 있다.
- Impact: 예상하지 않은 redirect 대상에 API key가 노출될 수 있다.
- Suggested Fix: Gemini client에 `Policy::none()` 또는 explicit same-origin policy를 적용하고 scheme/host/port 변경을 거부한다. 필요하면 수동 redirect에서 key를 재주입하지 않는다.
- Re-audit Method: 두 loopback server를 사용해 첫 server가 둘째 host/port로 redirect할 때 둘째 server에 `x-goog-api-key`가 0회 도착하는지 검증한다.
- Owner: Coder / Security

## 8. Cross-Pass Conflicts

### [XPF-F001] baseline 1:1 보존 선언과 provider 요구사항 교체가 충돌한다

- Related Findings: `IMP-F001`, `IMP-F006`
- Conflict: 문서는 master baseline을 원문 보존한다고 선언하지만 구현에 맞춰 baseline 의미를 변경했다.
- Resolution: baseline을 복구하고 provider discovery state machine을 derived requirement로 분리한다.
- Gate Impact: Major documentation authority conflict이므로 HOLD.
- Required Fix Before PASS: FR/NFR/CON 1:1 diff와 derived ID 분리.

### [XPF-F002] security/engineering evidence와 audit roadmap PASS가 충돌한다

- Related Findings: `IMP-F006`, `DBG-F002`, `DBG-F003`, `SEC-F001`, `SEC-F004`
- Conflict: 코드와 테스트는 partial/ignored/미검증 경계를 보이지만 roadmap은 전 Phase PASS를 선언한다.
- Resolution: latest audit evidence를 gate source로 반영하고 무효 Accepted Risk를 제거한다.
- Gate Impact: Critical/Major가 닫힐 때까지 HOLD.
- Required Fix Before PASS: roadmap status/owner/expiry/evidence 동기화.

## 9. Accepted Risks

### SEC-F007 — 유지

- 상태: Accepted Risk
- 범위: `quick-xml 0.30.0` High 2건의 현재 Windows runtime 비도달
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: `eframe 0.31.0` 또는 상위 `accesskit_unix` patch
- 참고: Linux target을 release scope에 포함하기 전에는 reachability를 다시 검토해야 한다.

### 대형 저장소 memory risk — Accepted Risk로 인정하지 않음

- `audit_roadmap.md`의 memory 항목은 owner, expiry, review trigger가 없고 실제 peak memory 근거도 없다.
- 본 재감사에서는 `DBG-F003 Needs Fix (Major)`로 관리한다.

## 10. Needs Spec Clarification

- 없음. 신규 provider discovery/verification workflow 자체는 baseline의 하위 구현으로 추가할 수 있으나 baseline 원문을 교체할 권한은 없다.

## 11. Required Fixes Before PASS

1. `SEC-F001`: outbound question, validation map, snapshot/ref/profile을 canonical approval digest에 결속한다.
2. `IMP-F001`: FR-013/FR-017 baseline 원문을 복구하고 provider state machine을 derived requirement로 분리한다.
3. `DBG-F003`: cancelled partial scan의 Ready 저장·분석을 차단하고 실제 2GiB/peak-memory acceptance를 만든다.
4. `DBG-F002`: prefix-only rehash를 changed-path full hash 또는 bounded incremental watcher로 교체한다.
5. `IMP-F004`: evidence가 필수인 claim classification invariant를 검증한다.
6. `SEC-F004`: Gemini secret-bearing cross-host redirect를 차단한다.
7. `IMP-F003`: 실제 global hotkey 또는 안전한 non-hide fallback을 구현한다.
8. `IMP-F006`: audit roadmap gate와 Accepted Risk를 최신 증거로 동기화한다.
9. 수정 commit 기준으로 전체 gate와 보안 회귀를 재실행한다.

## 12. Re-audit Checklist

- [ ] packet canonical digest tamper matrix 전부 거부
- [ ] FR-013/FR-017 baseline 원문 1:1 복구
- [ ] Observed empty evidence가 Unknown으로 강등
- [ ] cancelled scan이 Ready/DB/query로 진입하지 않음
- [ ] repository switch가 이전 scan 종료
- [ ] 16KiB 후반부 preserved-mtime 변경 감지
- [ ] 100k/2GiB peak memory와 watcher I/O budget evidence
- [ ] Windows global hide/show lifecycle 또는 non-hide fallback
- [ ] Gemini cross-host redirect key zero-leak
- [ ] audit roadmap Phase gate/Accepted Risk 정합성
- [ ] fmt, strict Clippy, workspace tests, ignored representative profile, locked release build, cargo audit
- [ ] clean commit 기준 `git status`

## 13. 상태 집계

- Verified: 16건
  - `IMP-F002`, `IMP-F005`
  - `DBG-F001`, `DBG-F004`, `DBG-F005`, `DBG-F006`, `DBG-F007`, `DBG-F008`
  - `SEC-F002`, `SEC-F003`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`, `SEC-F011`
- Needs Fix: Critical 1건 — `SEC-F001`
- Needs Fix: Major 7건 — `IMP-F001`, `IMP-F003`, `IMP-F004`, `IMP-F006`, `DBG-F002`, `DBG-F003`, `SEC-F004`
- Accepted Risk: 1건 — `SEC-F007`
- Minor: 0건
- 전체 판정: **HOLD**

## 14. Final Decision

**HOLD**

직전 finding 중 version 설명과 outbound secret filtering은 닫혔고 cloud contract·scan UI·UI token도 의미 있게 개선됐다. 그러나 consent hard boundary의 Critical 회귀, master baseline 변조, incomplete snapshot 신뢰, prefix-only stale detection, unverified large-repository memory, global shortcut dead-end, roadmap overclaim, Gemini redirect key 경계 때문에 PASS할 수 없다. 현재 대상도 미커밋 working tree이므로 수정 완료 후 clean commit을 기준으로 재감사가 필요하다.

## 15. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_13.md`의 Re-audit #11 결과를 기준으로 수정하세요.
각 finding을 CODE_MENTAT_SPEC.md, spec.md, audit_roadmap.md와 실제 코드에 대조한 뒤 문서 권위를 먼저 복구하세요.
최우선은 SEC-F001 canonical egress seal이며, 이어 IMP-F001 baseline 복구, DBG-F003 incomplete snapshot/memory,
DBG-F002 watcher, IMP-F004 claim invariant, SEC-F004 redirect key, IMP-F003 global shortcut, IMP-F006 roadmap 순으로 처리하세요.
수정 후 tamper matrix, cancelled-scan query block, 16KiB tail edit, redirect zero-leak, global hide/show, 100k/2GiB memory 증거와
전체 fmt/clippy/test/release/audit 결과를 clean commit 기준으로 기록하세요. 기존 감사 보고서는 수정하지 마세요.
```
