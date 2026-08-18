# D3D 통합 감사 보고서 (Turn 2)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-18
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 감사 방식: Implementation / Debug / Security 3-pass
- 변경 제한: 소스 코드, 테스트, 설정, 기존 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 감사 요약

복구된 문서 패키지는 필요한 파일명과 기본 구조를 갖췄지만, 실제 구현을 검증하기 전에 Phase 1~5 완료와 릴리스 PASS를 선언했다. 현재 실행 증거는 다음과 같다.

| 검사 | 결과 |
|---|---|
| `cargo test --workspace --locked` | PASS — 14 passed, 0 failed |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `cargo fmt --all -- --check` | FAIL — 다수 파일 포맷 차이 |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | FAIL — `manual_div_ceil` 2건 |
| `cargo audit --file Cargo.lock` | FAIL — High 취약점 2건, unmaintained 경고 2건 |
| Git 범위 확인 | FAIL — Git 최상위가 프로젝트가 아니라 `C:/`, 커밋 없음 |

Critical finding 3건과 Major finding이 남아 있으므로 기존 `docs/audit/audit_report_1.md`의 PASS 및 릴리스 승인 선언은 재현되지 않는다.

## 2. 감사 범위

- 마스터/통제 문서: `spec.md`, `CODE_MENTAT_SPEC.md`, `designs.md`, `IMPLEMENTATION_SUMMARY.md`, `DESIGN_DECISIONS.md`, `CHANGELOG.md`, `BUILD_GUIDE.md`, `audit_roadmap.md`
- 전체 Cargo workspace 및 10개 crate
- 테스트, 포맷, Clippy, Windows release build
- Cargo dependency tree 및 RustSec advisory scan
- 읽기 전용, 경로, AppData, egress consent, API secret, timeout/cancellation 경계

## 3. 제외 범위

- 실제 Google/OpenRouter/OpenAI 계정 호출: 사용자 비밀과 비용이 필요하므로 제외
- Linux/macOS 실기 빌드와 실행: 현재 Windows 환경에서 제외
- GUI 수동 실행: 실제 AppData를 변경하므로 제외
- 10,000/100,000 파일 성능 벤치마크: 전용 fixture와 측정 harness가 없어 제외
- 종료/취소/오류 전후 파일 이벤트 및 Git 상태 3-pass 실증: 해당 harness가 없어 제외

## 4. Pass 1 — Implementation Compliance Findings

### [IMP-F001] 두 마스터 사양의 권위와 Phase 상태가 충돌한다

- Pass: Implementation
- Pattern: `IMP-004`, `SPEC-GAP-001`
- Area: 문서 권위, Phase gate, 요구사항 추적성
- Severity: Major
- Status: Needs Spec Clarification
- Evidence:
  - `spec.md:12-14`는 자신을 유일한 마스터 진실원으로 선언한다.
  - `CODE_MENTAT_SPEC.md:1-6,988`은 전체 실행 계약을 유지하며 최종 승인선을 `Phase 1 GO`로 둔다.
  - 새 `spec.md`는 기존 FR/NFR/CON/ADR ID, Phase별 선행조건과 출구조건 대부분을 제거했지만 supersede 결정이나 대응표가 없다.
- Expected: 기존 승인 사양을 보존하면서 새 `spec.md`가 대체/요약/현재 상태 중 무엇인지 명시하고, 요구사항 ID와 Phase 상태를 양방향 매핑해야 한다.
- Actual: 구현된 코드를 축약해 새 요구사항처럼 동결하고 기존 승인선과의 충돌을 기록하지 않았다.
- Impact: 후속 코더가 미완료 구현을 요구사항으로 오인하고 원래 보안·성능·멀티플랫폼 계약을 누락할 수 있다.
- Suggested Fix: 두 문서의 authority/supersession을 ADR로 확정하고, 기존 요구사항별 `Implemented / Partial / Missing / Superseded` 추적표를 만든다. 완료 상태는 검증 증거가 생기기 전까지 되돌린다.
- Re-audit Method: 두 사양, ADR, 요구사항 추적표, Phase gate를 다시 대조한다.
- Owner: Architect / Human

### [IMP-F002] 제품 버전과 동결 의존성 선언이 실제 매니페스트와 다르다

- Pass: Implementation
- Pattern: `IMP-004`, `DEP-001`
- Area: 버전, dependency contract
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `CHANGELOG.md:10`과 `IMPLEMENTATION_SUMMARY.md:4`는 1.0.0/Phase 5 완료를 선언한다.
  - `Cargo.toml:17`의 실제 workspace 버전은 0.1.0이다.
  - `spec.md:57`은 `rusqlite 0.32`를 동결하지만 `Cargo.toml:51`은 0.33이다.
  - 문서는 Tokio 1.43을 동결하지만 semver 범위로 인해 `Cargo.lock`은 1.53.1을 사용한다.
- Expected: 릴리스 문서, package version, lockfile, 동결 의존성 정책이 동일해야 한다.
- Actual: 문서만 1.0.0 릴리스 완료이고 빌드는 0.1.0이다.
- Impact: 패키징, 지원, 재현성, 취약점 대응 기준이 불명확하다.
- Suggested Fix: 현재 실제 성숙도를 기준으로 버전을 확정하고 모든 문서/매니페스트를 동기화한다. exact pin 또는 lockfile-authoritative 정책을 명시한다.
- Re-audit Method: `cargo metadata --locked`와 릴리스 문서를 대조한다.
- Owner: Architect / Coder

### [IMP-F003] 3-Tier UI 계약과 실제 뷰포트·토큰·단축키가 일치하지 않는다

- Pass: Implementation
- Pattern: `IMP-001`
- Area: UI/UX, design token, runtime viewport
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `spec.md:165-176`, `designs.md:40-96`은 580x48/580x280/640x460과 Slate 토큰을 동결한다.
  - `crates/mentat-app/src/main.rs:18-24`는 시작 크기를 540x52로 고정한다.
  - Tier 전환 코드에는 `ViewportCommand::InnerSize`가 없어 Card/Inspector 크기로 확장되지 않는다.
  - `crates/mentat-app/src/theme.rs:8-19`의 배경/카드/보더/충돌 색상은 문서 값과 다르다.
  - README의 글로벌 단축키를 구현하는 호출이 없고 `global-hotkey`는 앱 의존성에도 연결되지 않았다.
- Expected: 문서의 크기·토큰·단축키가 코드와 런타임 상태 전이에 연결되어야 한다.
- Actual: UI 골격만 존재하며 문서의 100% 반영 주장은 성립하지 않는다.
- Impact: Tier 2/3 콘텐츠가 작은 고정 창에서 잘리거나 명세와 다른 UX가 된다.
- Suggested Fix: 뷰포트 상태 전이를 구현하고, 토큰과 단축키의 canonical source를 하나로 정한 뒤 UI 테스트/스크린샷 증거를 추가한다.
- Re-audit Method: 각 Tier의 실제 창 크기, 키보드 동작, 스크린샷을 검증한다.
- Owner: Coder

### [IMP-F004] 로컬/클라우드 답변이 Evidence Before Advice 계약을 닫지 못한다

- Pass: Implementation
- Pattern: `IMP-003`
- Area: workflow, AnswerBundle, evidence validation
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `spec.md:21-33`, `README.md:13,23`은 증거 기반 Claim과 `/where`를 기능으로 선언한다.
  - `crates/mentat-analysis/src/semantic_kernel.rs:146-180`은 실제 문서-코드 비교 없이 문서 개수만 보고하며 `/where`는 generic fallback으로 처리한다.
  - `crates/mentat-app/src/app.rs:320-343,389-405`은 클라우드 텍스트를 그대로 `answer_preview`에 넣고 Claim/Evidence 검증이나 구조화 정규화를 수행하지 않는다.
  - 클라우드 질의 시작 시 이전 `recent_claims`/`evidence_map`을 비우지 않아 이전 질문의 증거가 새 답변 옆에 남을 수 있다.
- Expected: 모든 핵심 주장은 현재 snapshot의 유효 EvidenceRef와 분류를 가져야 하며 `/where`는 고유 계약을 구현해야 한다.
- Actual: 클라우드 답변은 검증되지 않은 원문이고 일부 로컬 workflow는 이름만 존재한다.
- Impact: 사용자에게 서로 다른 질문의 답변과 증거가 결합되어 표시될 수 있다.
- Suggested Fix: AnswerBundle 정규화/검증 경로를 클라우드 응답에도 강제하고, 요청 시작 시 결과 상태를 원자적으로 초기화한다. 각 slash command에 별도 테스트를 둔다.
- Re-audit Method: 가짜 인용, 이전 결과 잔존, `/where`, 실제 conflict fixture를 검증한다.
- Owner: Coder

### [IMP-F005] 영속화와 멀티플랫폼 완료 주장이 구현 증거보다 앞서 있다

- Pass: Implementation
- Pattern: `IMP-003`, `IMP-004`
- Area: storage, cross-platform release
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `IMPLEMENTATION_SUMMARY.md:22-24`는 최근 저장소와 BackendProfile 로드를 주장한다.
  - `crates/mentat-storage/src/db.rs:33-55`는 `saved_profiles` 테이블만 만들고 profile/session CRUD를 제공하지 않는다.
  - `crates/mentat-app/src/app.rs:91-114`은 최근 저장소만 읽고 backend profile/persona/session을 복원하지 않는다.
  - `.github`/CI workflow가 없고 이번 감사에서 확인한 release build는 Windows 한 플랫폼뿐이다.
- Expected: 완료 선언마다 구현 파일, 호출 경로, 플랫폼별 실행 증거가 있어야 한다.
- Actual: 스키마 골격과 Windows 빌드만으로 Phase 4~5 완료를 선언했다.
- Impact: 재실행 복원과 멀티플랫폼 지원을 사용자가 신뢰할 수 없다.
- Suggested Fix: 완료 주장을 부분 구현으로 낮추거나 실제 CRUD/복원/CI 매트릭스와 플랫폼 smoke를 추가한다.
- Re-audit Method: 설정 저장→재시작→복원 및 3개 플랫폼 CI 결과를 확인한다.
- Owner: Coder

## 5. Pass 2 — Debug / Engineering Quality Findings

### [DBG-F001] UI update 경로가 최대 수 초 동안 동기 대기한다

- Pass: Debug
- Pattern: `DBG-001`
- Area: responsiveness, async orchestration
- Severity: Major
- Status: Needs Fix
- Evidence: `crates/mentat-app/src/app.rs:149-173,182-212,236-258,268-310,354-371`에서 UI 이벤트 처리 중 `recv_timeout`으로 300ms, 500ms, 3초, 4초까지 대기한다.
- Expected: I/O와 분석은 background task가 수행하고 UI는 polling/state transition만 해야 한다.
- Actual: 저장소 열기와 ping이 UI 스레드를 각각 최대 3초/4초 차단한다. 성공 기준 100ms와 로컬 workflow <10ms도 보장되지 않는다.
- Impact: 창이 멈추고 대형 저장소에서는 조용히 timeout한 뒤 session만 열린 불완전 상태가 된다.
- Suggested Fix: 모든 blocking receive를 비차단 상태 머신/channel poll로 바꾸고 진행/실패/취소 상태를 명시한다.
- Re-audit Method: egui frame long-task 측정, 3초 이상 scan/ping fixture, 취소 latency를 재검증한다.
- Owner: Coder

### [DBG-F002] snapshot, file list, watcher, EvidenceRef가 동일 시점의 무결성을 보장하지 않는다

- Pass: Debug
- Pattern: `DBG-002`
- Area: snapshot consistency, stale detection, evidence hash
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `crates/mentat-app/src/app.rs:151-154`은 파일 목록을 한 번 스캔한 뒤 `create_snapshot()`에서 다시 스캔한다.
  - `crates/mentat-repository/src/watcher.rs:30-43`은 루트 직계 엔트리의 증가한 mtime만 확인해 깊은 파일/삭제/mtime 역행을 놓친다.
  - `crates/mentat-analysis/src/evidence.rs:43-58`은 전체 파일 해시가 아니라 excerpt만 해시하고 요청한 `line_end`를 실제 범위로 보정하지 않는다.
- Expected: UI file list, snapshot digest, EvidenceRef가 같은 봉인 시점에 생성되고 모든 외부 변경을 STALE로 전환해야 한다.
- Actual: 두 scan 사이 변경과 깊은 파일 변경을 놓칠 수 있다.
- Impact: 오래되거나 다른 snapshot의 근거를 현재 증거로 표시할 수 있다.
- Suggested Fix: 단일 scan 결과로 snapshot을 봉인하고, 정렬된 digest와 실제 파일 hash/line range를 연결하며 recursive watcher 또는 digest 검증을 사용한다.
- Re-audit Method: scan 중 변경, 깊은 파일 수정/삭제, mtime 역행, excerpt 외부 변경 fixture를 실행한다.
- Owner: Coder

### [DBG-F003] 파일 크기·총량 예산 없이 전체 파일을 메모리로 읽는다

- Pass: Debug
- Pattern: `DBG-002`
- Area: performance, resource limits
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `crates/mentat-repository/src/scanner.rs:111-130`은 metadata 확인 후 모든 파일을 `read_to_end`한다.
  - `crates/mentat-repository/src/session.rs:116-141`은 preview/search 대상 파일 전체를 `read_to_string`한다.
  - 파일 수, 개별 크기, 총 바이트, 행 길이, 취소 토큰 제한이 구현되어 있지 않다.
  - `spec.md:38`의 10,000파일/1초 기준을 검증하는 benchmark가 없다.
- Expected: untrusted repository에 명시적 resource budget과 streaming/cancellation 경계가 있어야 한다.
- Actual: 대형/희소/비정상 파일 하나가 메모리와 UI 응답성을 고갈시킬 수 있다.
- Impact: 로컬 저장소만으로 메모리 DoS 또는 장시간 정지가 가능하다.
- Suggested Fix: pre-read size gate, bounded streaming hash/text sample, 총량/파일수/행길이 예산, cancellation을 구현한다.
- Re-audit Method: giant file, 10,000파일, 100,000파일, 긴 단일 행 fixture로 시간/메모리를 측정한다.
- Owner: Coder

### [DBG-F004] NativeLlama ConcurrencyGate가 semaphore permit을 영구 누수한다

- Pass: Debug
- Pattern: `DBG-002`
- Area: concurrency, future backend contract
- Severity: Major
- Status: Needs Fix
- Evidence: `crates/mentat-inference-llama/src/contract.rs:88-104`은 borrowed permit을 `std::mem::forget`하지만 `IsolatedContextHandle`에 permit을 보관하지 않는다. handle drop은 별도 counter만 줄인다.
- Expected: context drop 시 semaphore permit도 반환되어 이후 요청이 진행해야 한다.
- Actual: `max_concurrent`번 acquire 이후 context를 drop해도 다음 acquire는 영구 대기한다.
- Impact: 문서가 완료라고 선언한 미래 native contract가 반복 요청에서 교착된다.
- Suggested Fix: `OwnedSemaphorePermit`을 handle 필드로 소유하고 drop으로 반환하며 `max+1` 요청 회귀 테스트를 추가한다.
- Re-audit Method: 두 context drop 후 세 번째 acquire가 제한 시간 내 성공하는지 검증한다.
- Owner: Coder

### [DBG-F005] 품질 게이트와 테스트 증거가 릴리스 PASS를 지지하지 않는다

- Pass: Debug
- Pattern: `TEST-001`, `BUILD-001`
- Area: format, lint, regression coverage, CI
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `cargo fmt --all -- --check` 실패.
  - strict Clippy가 `egress.rs:80`, `mentat-inference/src/lib.rs:24`에서 실패.
  - app과 `mentat-inference-openai`에는 테스트가 0개다.
  - `docs/audit/audit_report_1.md:40`은 13개 테스트라고 기록하지만 실제는 14개다.
  - CI workflow가 없다.
- Expected: 문서화된 게이트가 동일 명령으로 재현되고 핵심 실패모드 테스트가 있어야 한다.
- Actual: broad unit tests만 통과하며 UI/실제 adapter/consent/timeout/security 경로는 검증되지 않는다.
- Impact: 테스트 PASS가 제품 PASS로 과대해석된다.
- Suggested Fix: 포맷/Clippy를 통과시키고 consent, adapters, SSE chunking, timeout, snapshot, UI state에 회귀 테스트와 CI를 추가한다.
- Re-audit Method: 문서의 모든 명령과 새 failure-specific tests를 clean 환경에서 실행한다.
- Owner: Coder

### [DBG-F006] 프로젝트 Git 경계와 lockfile 정책이 재현성을 훼손한다

- Pass: Debug
- Pattern: `BUILD-001`, `DEP-001`
- Area: repository scope, lockfile
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `git rev-parse --show-toplevel` 결과가 `C:/`이고 저장소에는 커밋이 없다.
  - `git status`가 시스템 드라이브 전체를 untracked로 탐색한다.
  - `.gitignore:5`가 application workspace의 `Cargo.lock`을 제외한다.
- Expected: Git root가 프로젝트 루트이고 binary application의 lockfile이 버전 관리되어야 한다.
- Actual: 프로젝트 상태·diff·release provenance를 안전하게 판정할 수 없다.
- Impact: 시스템 파일을 실수로 Git 범위에 포함하거나 lockfile 없는 비재현 빌드를 만들 수 있다.
- Suggested Fix: 프로젝트 범위의 Git 저장소를 확립하고 `Cargo.lock`을 추적하며 비밀/산출물 ignore만 유지한다.
- Re-audit Method: 프로젝트 루트에서만 clean `git status`, tracked lockfile, clean locked build를 확인한다.
- Owner: Human / Coder

## 6. Pass 3 — Security & Privacy Findings

### [SEC-F001] Egress consent가 fail-open이며 승인 패킷과 실제 송신 패킷이 다르다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: consent, TOCTOU, external transmission
- Severity: Critical
- Status: Hold
- Evidence:
  - `crates/mentat-app/src/app.rs:261-279`에서 consent packet 조립이 500ms 내 끝나지 않으면 pending consent를 만들지 않고 그대로 `start_inference_stream()`으로 진행한다.
  - `crates/mentat-app/src/app.rs:547-552`는 사용자가 승인한 packet을 폐기한다.
  - `crates/mentat-app/src/app.rs:296-310`은 송신 직전에 packet을 다시 조립하므로 승인 범위와 실제 전송 범위가 달라질 수 있다.
- Expected: 승인 receipt에 고정된 immutable packet만 전송하며 조립/검증 실패는 fail-closed여야 한다.
- Actual: 느린 저장소에서 승인 없이 전송될 수 있고 승인 후 저장소 변경도 새 packet에 유입된다.
- Impact: 사용자 승인 없는 소스 코드 외부 전송 가능성.
- Suggested Fix: packet 조립 실패/timeout 시 전송을 중단하고, 승인된 packet hash/snapshot/file list를 그대로 request에 소비하는 단일-use EgressReceipt를 구현한다.
- Re-audit Method: 500ms 초과 조립, 승인 전 변경, 승인 후 변경, 취소 fixture에서 네트워크 호출 0건과 packet 동일성을 검증한다.
- Owner: Coder / Security

### [SEC-F002] 민감정보 필터가 파일명 일부만 검사하여 비밀 원문 전송을 막지 못한다

- Pass: Security
- Pattern: `SEC-001`
- Area: secret detection, least-data egress
- Severity: Critical
- Status: Hold
- Evidence:
  - `spec.md:158`은 `token` 파일 차단을 요구하지만 `crates/mentat-analysis/src/egress.rs:19-39`에는 token 규칙이 없다.
  - filter는 파일명만 검사하고 내용의 API key/token/고엔트로피 문자열을 검사하지 않는다.
  - `crates/mentat-analysis/src/egress.rs:41-78`은 `_user_question`을 사용하지 않고 모든 문서/매니페스트의 앞 60줄을 조립한다.
  - UI는 `app.rs:522-532`에서 포함 파일 이름/행을 보여주지 않고 개수만 표시한다.
- Expected: 질문 관련 최소 문맥만 선택하고 파일명+내용 secret scan 결과와 실제 포함 행을 사용자가 확인해야 한다.
- Actual: README/Cargo 설정 등에 포함된 실키나 `token.txt`가 그대로 전송될 수 있다.
- Impact: 저장소 비밀정보와 무관한 코드의 외부 유출.
- Suggested Fix: content-aware secret scanner, user exclusion, query-aware selection, exact included refs, redacted preview를 구현하고 차단 테스트를 추가한다.
- Re-audit Method: token 파일, 정상 이름 안의 API key, 고엔트로피 값, 사용자 제외, 질문 무관 파일 fixture를 검증한다.
- Owner: Coder / Security

### [SEC-F003] AppData 격리 helper가 실제 저장소 열기 경로에서 강제되지 않는다

- Pass: Security
- Pattern: `SEC-004`, `SEC-005`
- Area: strict read-only boundary, storage path
- Severity: Critical
- Status: Hold
- Evidence:
  - `crates/mentat-platform/src/lib.rs:23-41`에 격리 검증 함수가 있다.
  - `crates/mentat-app/src/app.rs:83-89,132-145`은 DB를 먼저 열고 저장소 선택 시 해당 함수를 호출하지 않는다.
  - 사용자가 AppData의 상위 디렉터리(예: 사용자 홈)를 저장소로 선택하면 `mentat.db`와 후속 DB write가 선택한 저장소 내부에 놓인다.
  - `PlatformManager::get_app_data_dir()`는 OS 경로 획득 실패 시 현재 디렉터리 `.`로 fallback한다.
- Expected: 저장소 session 확립 전에 app data/log/temp/export 경로가 모두 root 밖임을 검증하고 실패 시 열기를 거부해야 한다.
- Actual: helper는 단위 테스트에서만 사용되고 실제 호출 경로는 enforce하지 않는다.
- Impact: 제품의 핵심 Read-Only 보증을 직접 위반해 조사 대상에 파일을 생성/수정할 수 있다.
- Suggested Fix: fallback을 fail-closed로 바꾸고 repository open transaction에서 모든 writable path를 검증한 뒤에만 session/storage를 활성화한다.
- Re-audit Method: home/appdata parent/current-dir root fixture에서 파일 이벤트 0건과 시작 거부를 검증한다.
- Owner: Coder / Security

### [SEC-F004] API key와 네트워크 프로필 경계가 비밀 노출을 최소화하지 않는다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: credential handling, transport
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `crates/mentat-inference/src/types.rs:37-46`의 serializable profile이 실제 API key 문자열을 소유한다.
  - `crates/mentat-app/src/widgets/settings_panel.rs:122-139`은 매 frame key를 복제한다.
  - `crates/mentat-inference-openai/src/gemini_adapter.rs:34-37,79-82`은 API key를 URL query에 넣고 네트워크 오류 문자열을 UI까지 전달한다.
  - Google 공식 REST 예시는 `x-goog-api-key` 헤더를 사용하고 secret store/environment 사용을 권고한다.
  - Custom URL은 `http://localhost`가 기본이나 비루프백 HTTP를 막는 검증이 없다.
- Expected: profile에는 secret reference만 두고 요청 시점에 credential store에서 읽으며, 외부 endpoint는 HTTPS, loopback만 HTTP를 허용해야 한다.
- Actual: key가 일반 Clone/Serialize 데이터이며 URL/error surface에 포함될 수 있다.
- Impact: 로그, 오류 UI, crash dump, serialization을 통한 key 노출 가능성.
- Suggested Fix: OS keychain/secret_ref 경계, redacted error, header authentication, endpoint validation을 구현한다.
- Re-audit Method: key가 Debug/Serialize/error/URL/DB에 나타나지 않는지와 non-loopback HTTP 거부를 검증한다.
- Owner: Coder / Security

### [SEC-F005] timeout과 cancellation이 HTTP 요청 전체 수명주기를 감싸지 않는다

- Pass: Security
- Pattern: `SEC-002`
- Area: resource exhaustion, request lifecycle
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `BackendProfile.timeout_secs`는 선언만 되고 adapter에서 사용되지 않는다.
  - `openai_adapter.rs:130-140`, `gemini_adapter.rs:107-117`의 `.send().await`는 cancellation select 밖에 있다.
  - cancellation은 response를 받은 뒤 byte stream loop에서만 확인한다.
- Expected: connect/send/header/body 전체에 hard timeout과 request-scoped cancellation을 적용해야 한다.
- Actual: 연결 또는 response header가 멈추면 Esc 취소와 5분 상한이 작동하지 않는다.
- Impact: 네트워크 장애나 악성 custom endpoint가 작업을 무기한 점유할 수 있다.
- Suggested Fix: client/request timeout과 `tokio::select!`를 전체 요청 수명주기에 적용하고 최종 이벤트 단일성을 검증한다.
- Re-audit Method: connect hang, slow header, slow body, cancel-before-response fixture를 실행한다.
- Owner: Coder

### [SEC-F006] scan 경로가 symlink 외부 파일을 canonical boundary 검사 없이 열 수 있다

- Pass: Security
- Pattern: `SEC-004`
- Area: path traversal, repository boundary
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `ReadOnlySession::read_file_content`는 canonical 검사를 하지만 `scan_files()`는 `session.rs:83-109`에서 `path.is_file()` 후 `FileScanner::inspect_file()`을 직접 호출한다.
  - `scanner.rs:111-123`은 `root.join(rel_path)`를 canonicalize/root-check 없이 `metadata`와 `File::open`으로 연다.
  - 기존 `test_external_path_blocked`는 `../` 직접 읽기만 검사하고 scan 중 symlink 파일을 검사하지 않는다.
- Expected: scan/read/hash 모든 경로가 동일 canonical root guard를 사용해야 한다.
- Actual: 파일 symlink는 walker가 따라가지 않더라도 `Path::is_file`/`File::open`에서 target을 따라 외부 내용을 읽을 수 있다.
- Impact: 승인하지 않은 외부 파일의 크기와 hash를 수집하고 root 경계를 위반한다.
- Suggested Fix: 모든 entry를 canonicalize한 뒤 root containment와 symlink policy를 검사하는 단일 path guard를 사용한다.
- Re-audit Method: 외부 file symlink, directory symlink, junction, nested repo fixture를 OS별로 검증한다.
- Owner: Coder / Security

### [SEC-F007] RustSec High 취약점 2건이 Linux 접근성 의존 경로에 남아 있다

- Pass: Security
- Pattern: `SEC-006`, `DEP-001`
- Area: supply chain
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `cargo audit --file Cargo.lock`은 `quick-xml 0.30.0`에서 `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`를 검출했다.
  - 둘 다 CVSS 7.5 High이며 fix는 `quick-xml >=0.41.0`이다.
  - `cargo tree --target all -i quick-xml@0.30.0`은 `zbus_xml -> atspi/accesskit -> egui-winit -> eframe -> mentat-app` 경로를 확인했다.
  - `paste 1.0.15`, `ttf-parser 0.25.1` unmaintained 경고도 존재한다.
- Expected: 릴리스 완료 선언 전에 target별 취약점 reachability와 upgrade/mitigation을 문서화해야 한다.
- Actual: 기존 감사는 dependency 취약점 검사를 수행하지 않고 PASS했다.
- Impact: Linux 빌드의 XML 처리 경로에서 CPU/메모리 DoS 위험이 남는다.
- Suggested Fix: 상위 UI/accessibility 의존성을 안전 버전으로 갱신하거나 reachability를 증명하고 만료 조건이 있는 Accepted Risk로 기록한다.
- Re-audit Method: `cargo audit`, `cargo tree --target all`, Linux smoke를 재실행한다.
- Owner: Coder / Security

### [SEC-F008] UI의 Read-Only 배지가 실제 경계 상태와 무관하게 항상 true다

- Pass: Security
- Pattern: `SEC-005`
- Area: security claim, user trust
- Severity: Major
- Status: Needs Fix
- Evidence: `crates/mentat-app/src/widgets/pill_bar.rs:20-33,75-95`에서 `is_read_only`가 session/격리 검증 결과와 관계없이 항상 true다.
- Expected: 배지는 경계 검증과 writable-path audit가 성공한 뒤에만 `READ_ONLY_READY`가 되어야 한다.
- Actual: 저장소 미선택, scan 실패, AppData 격리 미검증 상태에서도 녹색 R/O를 표시한다.
- Impact: 실제 보안 상태보다 강한 보증을 사용자에게 제공한다.
- Suggested Fix: explicit boundary state를 core에서 만들고 검증 성공 전에는 Unknown/Validating/Error로 표시한다.
- Re-audit Method: no repo, invalid repo, appdata-inside-root, scan error 상태의 badge를 검증한다.
- Owner: Coder

## 7. Cross-Pass Conflicts

### [XPF-F001] 구현 완료 문서와 실제 보안 gate가 충돌한다

- Related Findings: IMP-F001, IMP-F005, SEC-F001, SEC-F002, SEC-F003
- Conflict: 문서는 Phase 5/릴리스 완료를 선언하지만 승인 없는 egress와 저장소 내부 AppData write 가능성이 남아 있다.
- Resolution: 보안 hard boundary가 우선한다. 완료/PASS 선언을 철회하고 Critical 3건 수정 전 릴리스를 중단한다.
- Gate Impact: HOLD
- Required Fix Before PASS: Critical finding 전체 해소 및 관련 Pass 1/2/3 재감사.

### [XPF-F002] 단위 테스트 PASS와 제품 품질 PASS가 충돌한다

- Related Findings: DBG-F001~DBG-F005, SEC-F005, SEC-F007
- Conflict: 14개 단위 테스트는 통과하지만 formatter, strict Clippy, dependency audit가 실패하고 핵심 app/adapter 경로 테스트가 없다.
- Resolution: 테스트 개수는 부분 증거로만 인정하고 릴리스 gate로 사용하지 않는다.
- Gate Impact: HOLD
- Required Fix Before PASS: 명시된 quality gate와 failure-specific tests 통과.

## 8. Accepted Risks

- 없음.
- 기존 문서의 대형 저장소 위험은 owner, 만료일, 상한 측정, 재검토 조건이 없어 `Accepted Risk` 요건을 충족하지 않는다.

## 9. Needs Spec Clarification

1. `spec.md`와 `CODE_MENTAT_SPEC.md`의 authority/supersession 관계.
2. 실제 제품 버전이 0.1.0인지 1.0.0인지 및 Phase 완료 상태.
3. 외부 provider를 포함한 1.0 릴리스 범위와 원래 Phase 1 승인선 변경 근거.
4. Linux/macOS 지원을 선언만 할지 release gate로 강제할지.

## 10. Required Fix Order

1. **P0 보안:** SEC-F001, SEC-F002, SEC-F003.
2. **P0 문서 권위:** IMP-F001과 버전/완료 선언 정정.
3. **P1 보안:** SEC-F004~SEC-F008.
4. **P1 정확성/응답성:** DBG-F001~DBG-F004, IMP-F004.
5. **P1 품질/공급망:** DBG-F005~DBG-F006, SEC-F007.
6. **P2 UI/영속화/멀티플랫폼:** IMP-F003, IMP-F005.

## 11. 재감사 체크리스트

- [ ] 승인 packet과 실제 request packet hash/snapshot/refs가 동일함
- [ ] packet 생성 실패/timeout/변경 시 네트워크 호출 0건
- [ ] AppData parent를 repo로 선택해도 저장소 write 이벤트 0건
- [ ] token/content secret fixture가 egress에서 차단됨
- [ ] external symlink가 scan/read/hash에서 차단됨
- [ ] HTTP connect/send/body timeout 및 취소 테스트 통과
- [ ] 세 번째 NativeLlama context acquire가 drop 후 성공함
- [ ] snapshot/file list/evidence가 단일 시점에 봉인됨
- [ ] 깊은 파일 수정/삭제가 STALE 전이를 발생시킴
- [ ] `cargo fmt`, strict Clippy, tests, release build, `cargo audit` 통과
- [ ] Linux/macOS CI 및 UI Tier별 실제 크기/스크린샷 증거 존재
- [ ] Git root가 프로젝트이며 `Cargo.lock`이 추적됨

## 12. Final Decision

**HOLD**

Critical 3건과 다수의 Major finding이 남아 있어 현재 상태는 릴리스 또는 PASS가 아니다. 기존 `audit_report_1.md`는 이번 재감사 증거로 superseded된다.

## 13. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_2.md`의 HOLD 감사 결과를 기준으로 수정하세요.
먼저 SEC-F001, SEC-F002, SEC-F003과 IMP-F001을 처리하고, 각 finding의 Suggested Fix와 Re-audit Method를 회귀 테스트로 고정하세요.
계약 변경은 권위 사양과 ADR을 먼저 갱신한 뒤 구현하고, 소스 수정 후 formatter, strict Clippy, workspace tests, locked release build, cargo audit 결과를 기록하세요.
기존 audit_report_1.md의 PASS 선언은 재사용하지 마세요.
```
