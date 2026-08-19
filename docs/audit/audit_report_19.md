# D3D 재감사 보고서 (Turn 19 / Re-audit #17)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_18.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`, `CR-UX-001_TRACEABILITY.md`
- 기준 commit: `b77e85341a18105880cc2fb2d65cd405d2082257`
- 감사 대상: clean commit 전체
- 중점 범위: agent loop, tool egress, prompt injection, secret storage, DB migration, Markdown renderer
- 변경 제한: 기존 감사 보고서와 소스 코드 수정 없음
- 최종 판정: **HOLD**

## 1. 요약

Turn 18이 요구한 전체 신규 파일 고정과 네 가지 실행 게이트는 완료됐다. 기준 commit은 감사 시작 시 clean했고, native credential 실제 OS 저장소 round-trip, 100k/2GiB profile, release build가 통과했다. 전체 workspace 147 tests, formatter, strict Clippy도 통과했다.

보안 경계 자체는 대체로 fail-closed다. Prompt Kernel은 길이 프레이밍과 digest로 editable prompt와 분리되고, API key는 SQLite가 아니라 OS native credential store에 저장된다. DB는 v1~v5 순차 `BEGIN IMMEDIATE`, online backup, DB/WAL/SHM quarantine, future-version 차단을 구현했다. Markdown은 1MiB/10,000-event/32-depth 상한을 두며 이미지와 unsafe scheme을 실행하지 않는다.

그러나 실제 기본 Chat UI가 `AgentLoop`를 호출하지 않고 `tools = []`, `repository_context = None`으로 provider backend를 직접 호출한다. 외부 provider tool result의 canonical exact-body authorization도 adapter 전송 직전에 연결되지 않았으며, 현재 `AgentLoop`는 LocalMock 외 요청을 의도적으로 차단한다. 따라서 저장소를 연결해도 사용자 질문이 read-only tool loop와 grounding trace를 이용하지 못한다. 문서 추적표도 여전히 `PLAN ONLY`, `0/43`과 실제 구현 상태를 동시에 담고 있어 release 판단 근거로 사용할 수 없다.

Critical 보안 누출은 확인되지 않았지만 Major 3건이 남아 최종 판정은 **HOLD**다.

## 2. 실행 게이트

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 147 passed, 0 failed, 2 ignored |
| native credential ignored smoke | PASS — OS store put/get/delete 후 재조회 `None` |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, scan 96,705ms, peak 46,133,248 bytes < 128MiB |
| `cargo build --release --locked -p mentat-app` | PASS |
| `cargo mentat-build build --platform all --profile release --dry-run` | PASS — 6개 target 계획 생성 |
| clean commit 확인 | PASS — 감사 시작 시 `git diff --exit-code`, `git status --short` clean |
| AppData SQLite secret pattern scan | PASS — API key/native smoke fixture 검출 0 |
| tracked secret-path scan | PASS — `.env`, PEM/P12/PFX/private-key 파일 0; 탐지 문자열은 redaction 합성 fixture뿐 |
| `cargo audit --no-fetch --file Cargo.lock` | **FAIL** — High 2건, unmaintained 2건 |

`cargo audit` 실패 상세:

- `quick-xml 0.30.0`: `RUSTSEC-2026-0194`, CVSS 7.5 High
- `quick-xml 0.30.0`: `RUSTSEC-2026-0195`, CVSS 7.5 High
- `paste 1.0.15`: `RUSTSEC-2024-0436`, unmaintained
- `ttf-parser 0.25.1`: `RUSTSEC-2026-0192`, unmaintained

기존 `SEC-F007`의 Windows 비도달 Accepted Risk는 유지하되, 명령 실패 자체를 PASS로 바꾸지 않는다. 멀티 플랫폼 Linux release 범위에 들어가기 전 reachability와 의존성 갱신을 다시 검증해야 한다.

## 3. Pass 1 — 구현 정합성

### 3.1 Agent loop — FAIL

- 도메인과 로컬 실행기는 존재한다: `AgentLoop`는 최대 round/tool/byte/time, 반복 호출, cancellation, contract-specific terminal을 처리한다.
- 실제 기본 Chat UI는 `crates/mentat-app/src/chat_app.rs:1084-1107`에서 `AgentRequest`의 `tools`를 빈 배열, `repository_context`를 `None`으로 만들고 `backend.infer_round_stream`을 직접 호출한다.
- 저장소 scan은 `RepositoryToolGateway`를 `chat_app.rs:1224-1234`에 보관하지만 질문 제출 경로에서 소비하지 않는다.
- production 코드에서 `AgentLoop::new` 호출은 없고 `agent_loop.rs` 내부 테스트 fixture만 존재한다.

결론: chat-only는 동작하지만 repository mentor의 실제 tool loop는 사용자 경로에 연결되지 않았다.

### 3.2 Tool egress — FAIL-CLOSED / 기능 미완료

- `ToolEgressSealer`는 consent scope, turn/repository/snapshot/profile/provider/model/endpoint, semantic payload, exact provider body, canonical refs와 receipt ID를 digest에 결속한다.
- durable store는 `Prepared` 선저장과 terminal 단방향 compare-and-set을 강제한다.
- tampered receipt와 exact-body mismatch 테스트는 통과한다.
- 그러나 `ToolEgressSealer` 사용처는 `tool_egress.rs` 자체 테스트뿐이며 OpenAI/Gemini adapter 송신 직전 경로에는 연결되지 않았다.
- `agent_loop.rs:177-181`은 LocalMock이 아닌 provider의 tool result를 `TOOL_EGRESS_CONSENT_REQUIRED`로 차단한다.

결론: 무승인 repository bytes 외부 전송은 차단되지만 FR-032/044의 외부 provider tool loop는 사용할 수 없다.

### 3.3 Prompt injection — PASS

- immutable Kernel은 binary resource에서 로드되며 editable System/Persona와 별도 길이 프레임으로 합성된다.
- repository 원문은 PromptComposition에 직접 합쳐지지 않고 승인된 tool result 경로만 사용하도록 계약이 분리됐다.
- fake marker가 section을 탈출하지 못하는 결정성 테스트, editable prompt가 Kernel digest를 바꾸지 못하는 테스트, factory checksum 검증이 통과했다.
- tool surface는 core enum의 6개 read-only operation으로 닫혀 있으며 write/exec variant가 없다.

단, 실제 cloud tool path가 미연결이므로 end-to-end provider prompt-injection 시험은 해당 경로 연결 후 다시 수행해야 한다.

### 3.4 Secret storage — PASS

- `NativeSecretStore`는 `keyring`을 통해 Windows Credential Manager/macOS Keychain/Linux Secret Service를 사용한다.
- SQLite `provider_secret_preferences`에는 `provider:<UUID>` reference와 remember flag만 저장하며 API key column은 없다.
- native 오류는 원문 플랫폼 오류를 그대로 노출하지 않고 고정 safe message로 매핑한다.
- native smoke가 put/get/delete를 실제로 통과했고 삭제 후 `None`을 확인했다.
- workspace와 AppData DB secret scan에서 실키가 검출되지 않았다.

잔여 운영 위험: 같은 OS 사용자 권한을 획득한 공격자는 native credential에 접근할 수 있다. 이는 자체 암호화 파일보다 안전하지만 계정 탈취까지 방어하는 보장은 아니다.

### 3.5 DB migration — PASS

- `CURRENT_SCHEMA_VERSION = 5`이며 v1~v5가 각 `TransactionBehavior::Immediate` transaction으로 순차 적용된다.
- 기존 non-empty DB는 migration 전 SQLite online backup을 만들고, 실패/손상 시 DB/WAL/SHM을 같은 quarantine 디렉터리로 이동한다.
- future schema는 downgrade 없이 차단한다.
- registered-v1 ledger 복구, 기존 row 보존, corrupt quarantine, future schema, 사용자 resize 보존, legacy 기본 크기만 이동, prompt/conversation 재실행 복구 테스트가 통과했다.
- validated Audit result는 raw model response 저장을 거부하며 response contract가 Audit인지 transaction 안에서 재검증한다.

### 3.6 Markdown renderer — PASS WITH LIMITATION

- 실제 message UI는 `render_markdown`을 호출한다.
- parser input은 1MiB, event는 10,000개, nesting은 32단계로 제한된다.
- fenced code는 non-wrapping horizontal scroll과 copy CTA를 제공한다.
- image destination은 사용하지 않고 차단 문구만 표시하며 `file:`, `data:`, `javascript:` link는 출력 대상에서 제거된다.
- `http/https` 링크도 현재 클릭 가능한 widget이 아니라 plain text로만 표시되어 자동 fetch가 없다.

한계: CommonMark 전체 표현을 보존하는 renderer는 아니며 list/emphasis/table 등은 단순 text로 축약된다. 현재 NFR-020의 코드 블록·no-fetch 핵심 기준은 충족하지만, 향후 링크 클릭 기능을 추가하면 URL parser 기반 scheme/host 확인과 사용자 확인이 필요하다.

## 4. Pass 2 — 인과·UX·엔지니어링

- PASS: 저장소 없는 자유 대화, Markdown stream→terminal→SQLite reload, 취소 terminal, prompt Apply/CAS, native credential restore, window preference reopen.
- PASS: 100k/2GiB 대형 저장소 profile과 128MiB peak 임계값.
- PASS: stale/incomplete snapshot은 metadata-only `repo_status` 외 도구를 차단한다.
- FAIL: 저장소 연결 → 질문 → tool decision → gateway → SourceRef → GroundingTrace → UI의 인과 사슬이 기본 앱에서 끊긴다.
- FAIL: Grounding drawer와 Audit projection이 기본 Chat UI에 연결되지 않아 SourceRef line jump와 Advisor/Audit 격리 수용 시나리오를 end-to-end로 실행할 수 없다.
- 미실행: 실제 마우스 drag로 600×800 resize 후 프로세스 재기동 smoke. SQLite 600×800 fixture와 실제 기본 313×660 재기동은 통과했지만 이 수동 항목은 남는다.

## 5. Pass 3 — 보안·프라이버시

- PASS: repository path traversal, outside-root, stale/incomplete, call/byte/round 한도는 gateway/loop에서 fail-closed.
- PASS: prompt layer framing, immutable Kernel digest, source ownership, Audit evidence validator.
- PASS: canonical receipt tamper와 exact-body mismatch 거부, Prepared→terminal 단방향 전이.
- PASS: secret은 native store에만 저장되고 DB는 opaque reference만 유지한다.
- PASS: migration future-version 차단, backup/quarantine, malformed identifier 실패.
- PASS: Markdown image/unsafe scheme 자동 로드 0.
- 보류: 외부 provider exact-body authorization은 미연결이어서 안전하게 zero-egress지만 실제 승인 전송/redirect/cancel race의 end-to-end 재감사는 구현 후 필요하다.

## 6. Findings

### [IMP-CRUX-F001] 기본 Chat UI가 AgentLoop를 우회한다

- Severity: Major
- Status: Needs Fix
- Evidence: `chat_app.rs:1084-1107`, `chat_app.rs:1224-1234`, `agent_loop.rs:27-255`
- Expected: 저장소가 Ready이고 질문에 조사가 필요하면 동일 turn이 AgentLoop와 RepositoryToolGateway를 거쳐 SourceRef/GroundingTrace를 생성한다.
- Actual: 모든 기본 질문이 tool 0/repository context 없음으로 provider backend에 직접 전달된다.
- Impact: 저장소 멘토 핵심 기능, grounded answer, Audit evidence가 실제 UI에서 동작하지 않는다.
- Suggested Fix: chat orchestration을 AgentLoop 단일 진입점으로 통합하고 chat-only는 gateway/tool catalog가 없는 동일 경로로 처리한다.
- Re-audit: 저장소 연결 후 path hint 없는 질문이 tool call과 SourceRef를 만들고 terminal/reload까지 같은 trace를 유지하는 실제 UI fixture를 실행한다.

### [SEC-CRUX-F001] canonical tool egress seal이 provider 송신 경로에 연결되지 않았다

- Severity: Major
- Status: Needs Fix (현재 fail-closed)
- Evidence: `tool_egress.rs:44-149`, `agent_loop.rs:177-181`; repository-wide `ToolEgressSealer` production call site 0
- Expected: 승인 scope 검증 → exact provider body 생성 → Prepared durable receipt → 송신 직전 body/seal 재검증 → Sent/Failed terminal 전이가 하나의 경로여야 한다.
- Actual: seal/store는 독립 구현됐고 외부 provider tool result는 전부 차단된다.
- Impact: 데이터 유출은 막지만 승인된 cloud repository mentoring을 제공하지 못한다. 향후 단순 차단 해제만 하면 canonical 경계를 우회할 위험이 있다.
- Suggested Fix: adapter 공통 egress hook을 만들고 body 직렬화 이후, socket write 이전에만 authorization을 소비하도록 한다. 실패·redirect·cancel도 durable terminal 상태로 닫는다.
- Re-audit: consent/turn/repo/snapshot/profile/model/endpoint/path/range/hash/body tamper matrix와 redirect target zero-byte fixture를 실제 adapter에 실행한다.

### [IMP-CRUX-F002] Grounding/Audit 사용자 경로가 미완료다

- Severity: Major
- Status: Needs Fix
- Evidence: trace/store/validator 타입은 존재하지만 기본 turn은 항상 `AdvisorMarkdown`, `grounding_trace_id: None`; `CR-UX-001_TRACEABILITY.md`의 FR-034/043 및 NFR-021도 미완료로 기록됨
- Expected: conversation별 transient Audit 선택, contract-specific terminal, evidence drawer, SourceRef line jump가 실제 UI와 storage에 연결된다.
- Actual: validator와 storage는 있으나 production projection과 drawer가 없다.
- Impact: 사용자에게 근거를 역추적하거나 Audit Mode를 사용할 수 없고 구현 완료 주장도 불가능하다.
- Suggested Fix: mode를 turn 시작 시 고정하고 `CompletedPayload` tagged terminal을 UI reducer/storage에 연결한 뒤 validated SourceRef만 drawer에 투영한다.
- Re-audit: Advisor/Audit 교차 오염 0, invalid citation 비표시, valid path/range jump, restart restore 시나리오를 실행한다.

### [DOC-CRUX-F001] 추적·보안·아키텍처 상태 문서가 기준 commit과 불일치한다

- Severity: Major
- Status: Needs Fix
- Evidence: `CR-UX-001_TRACEABILITY.md:3,98`, `SECURITY_PRIVACY.md:232`, `SYSTEM_ARCHITECTURE.md:660`
- Expected: 각 요구사항은 실제 구현/검증/미완료 상태를 단일하게 표시하고 release gate가 이를 소비할 수 있어야 한다.
- Actual: 승인 일자와 일부 Implemented row가 존재하면서도 문서 전체 상태는 `PLAN ONLY`, `0/43`, runtime `NOT STARTED`로 남아 있다.
- Impact: 구현 범위, 남은 보안 작업, 완료율을 신뢰할 수 없어 감사 재현성과 인수인계가 깨진다.
- Suggested Fix: 43개 row를 실제 call-site/test 증거로 재판정하고 문서별 current/planned 섹션을 분리한다. Partial을 PASS로 승격하지 않는다.
- Re-audit: trace matrix의 status와 실제 production call graph/test를 전수 대조해 mismatch 0을 확인한다.

## 7. 최종 상태

- Verified/Pass 영역: prompt injection, secret storage, DB migration, Markdown no-fetch/limits, 대형 저장소 memory gate
- Needs Fix: Major 4, Minor 0, Critical 0
- Accepted Risk: `SEC-F007` 1건
- 최종 판정: **HOLD**

clean commit과 실행 증거는 재현 가능해졌으나, repository mentor의 실제 AgentLoop/egress/Grounding 경로와 권위 문서 동기화가 완료되지 않았다. 이 Major finding들을 닫기 전에는 CR-UX-001 완료 또는 멀티 플랫폼 release PASS를 선언하면 안 된다.

## 8. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_19.md`를 기준으로 수정하세요.
우선 기본 Chat UI를 AgentLoop 단일 경로에 연결하고, 외부 provider exact-body 송신 직전
canonical egress seal + durable receipt를 강제하세요. 이후 Grounding drawer/Audit projection을
연결하고 43개 추적표의 실제 상태를 동기화하세요. 기존 감사 보고서는 수정하지 말고
clean commit에서 전체 게이트와 end-to-end tamper/redirect/cancel 검증을 재실행하세요.
```
