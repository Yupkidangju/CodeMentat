# CR-UX-001 구현 로드맵

- **문서 상태:** `APPROVED PLAN — IMPLEMENTATION ACTIVE`
- **변경요청:** `CR-UX-001`
- **기준일:** 2026-08-19
- **현재 게이트:** `CR-0~2 PASS / CR-3~4 IN PROGRESS`
- **구현 권한:** 2026-08-19 사용자 `CR-UX-001 GO` 승인 수신
- **Prompt 원문:** `PROMPT_CONTRACT.md`

## 1. 목표와 완료 정의

기본 제품 경로를 구조화 감사 결과 생성기에서 자유 대화형 읽기 전용 저장소 멘토로 전환한다. 구현은 CR-0부터 CR-8까지 순서대로 진행하며 각 단계는 자체 테스트와 문서 증거를 남기고 다음 단계로 넘어간다.

완료는 다음 세 흐름이 모두 실제 UI까지 연결될 때만 인정한다.

```text
일반 대화: UserMessage → PromptComposer → AgentRequest → Markdown stream → ChatMessage 저장/UI
저장소 조사: 질문 → ToolDecision → RepositoryToolGateway → SourceRef → contract-specific final payload → Grounding/Audit projection
설정 복구: Prompt draft → Apply/Version → Restart restore → Factory reset draft → Apply
```

## 2. 동결된 실행 결정

| 항목 | 동결값 |
|---|---|
| 기본 UI | `312.5×660`, 최소 `240×360`, 사용자 resize 유지, 상태 기반 `InnerSize` 금지 |
| 프롬프트 계층 | Immutable Kernel → Editable System → Editable Persona → Repository state → Messages |
| 프롬프트 이력 | layer별 immutable content version + atomic profile revision; System/Persona 각각 latest 5 unreferenced 유지 |
| Agent 한도 | 최대 8 rounds, 24 tool calls, 5분, tool result 총 256KiB |
| 파일 읽기 한도 | 호출당 최대 400행 및 64KiB, 초과는 `ToolOmission` |
| 저장소 도구 | `repo_status`, `list_tree`, `search_paths`, `search_text`, `read_file_lines`, `file_metadata`만 기본 제공 |
| 최종 출력 | Advisor Mode는 자유 Markdown, Audit Mode만 AnswerBundle/Claim 사용 |
| 공급자 도구 | native 우선, capability가 없으면 검증된 emulated planner, 둘 다 실패하면 chat-only |
| 출처 소유권 | `SourceRef`는 Tool Gateway만 생성, 모델 생성 UUID/hash는 수용하지 않음 |
| 동의 | `RequestOnce` 또는 `RepositorySession`; revoke 후 신규 tool egress 차단 |
| 데이터 위치 | 대화·프롬프트·trace는 AppData SQLite, 저장소 내부 쓰기 0건 |
| 결정 ID | 요청서의 개념적 `DEC-UI-002`는 기존 ID 충돌로 `DEC-UI-004`를 canonical ID로 사용 |
| 최초 프로필 | `Intermediate` System + `DefaultAnalyst` Persona factory prompt |
| 대화 저장 | 기본 ON, 자동 만료 없음, AppData local persistence; OFF는 이후 message부터, 기존 기록 삭제는 별도 CTA |
| Audit Mode | conversation UI transient 토글, 기본 Advisor, 제출 시 turn response contract 고정, 앱 재시작 시 Advisor |
| STALE 정책 | 기존 message/trace와 metadata-only `repo_status`만 허용; 나머지 repository tools는 재인덱싱 전 차단 |
| CR 완료 범위 | 신규 43개 + 직접 영향 legacy overlay의 regression. 기존 비관련 Partial은 전역 제품 완료 주장만 차단 |

## 3. 의존 그래프

```text
CR-0 문서 계약
  ↓
CR-1 Conversation + Prompt + Storage ports/migration
  ↓
CR-2 Chat-only AgentRequest/Event + free Markdown persistence
  ↓
CR-3 RepositoryToolGateway + native/emulated AgentLoop + GroundingTrace
  ↓
CR-4 Dynamic consent + ToolEgressReceipt + SourceRef validation
  ↓
CR-5 Vertical responsive chat UI + prompt editors
  ↓
CR-6 Grounding drawer + Audit Mode isolation
  ↓
CR-7 Provider parity + migration/recovery + context compaction
  ↓
CR-8 Independent 3-pass audit + final re-audit
```

CR-1 내부의 순수 도메인 타입과 SQLite schema 설계는 계약 동결 후 병렬화할 수 있다. CR-3 provider adapter 작업은 `AgentRequest/AgentEvent/ToolDefinition`이 고정된 뒤에만 병렬화한다. CR-4는 CR-3의 실제 tool result 계약 없이 시작하지 않는다.

## 4. 단계별 작업

### CR-0 — 문서 기준선 재확정

**현재 상태:** PASS, 사용자 승인 수신.

- [x] 변경요청 원문과 기존 baseline/active spec 대조
- [x] 신규 FR/NFR/CON의 소유 단계·파일·테스트 계획
- [x] ADR supersede 및 ID 충돌 해소
- [x] 목표 아키텍처·보안 위협 모델·감사 로드맵 작성
- [x] factory Kernel/System/Persona canonical 원문과 합성 계약 작성
- [x] 기존 구현을 `Current`와 `Migration Required`로 분리
- [x] 사용자 `CR-UX-001 GO`

**출구 게이트:** 문서 정합성 검사 PASS + 사용자 승인. 승인 전 코드 변경 금지.

### CR-1 — Conversation 및 Prompt 도메인

#### CR-1A 핵심 대화 타입과 포트

- **파일:** `mentat-core/src/models.rs`, `mentat-core/src/ports.rs`, core tests
- **산출:** `Conversation`, `ConversationTurn`, `ChatMessage`, `MessageStatus`, `PromptProfile`, `PromptContentVersion`, `PromptProfileRevision`, 저장 포트
- **수용:** 저장소 없는 conversation이 유효하고 Kernel text는 editable 타입에 포함되지 않는다.

#### CR-1B PromptComposer와 공장 리소스

- **파일:** `mentat-persona/src/*`, `mentat-persona/assets/prompts/**`
- **산출:** 4개 System preset, 3개 Persona prompt, checksum/version, 결정적 composer
- **수용:** 동일 입력 byte-for-byte 동일 합성; factory reset 원문 checksum 일치.

#### CR-1C SQLite 순차 마이그레이션

- **파일:** `mentat-storage/src/db.rs`, storage tests
- **산출:** schema version, conversations/turns/messages/prompt_profiles/prompt_content_versions/prompt_profile_revisions/ui_preferences tables, SQLite online backup/quarantine manifest와 explicit ephemeral mode. Grounding/receipt tables는 CR-3/4 migration으로 분리
- **수용:** 기존 provider/snapshot 데이터 보존, layer별 active/turn 참조 content version+latest 5 unreferenced, atomic profile revision, cascade delete, malformed UUID/date/enum/future schema fail-closed, 저장소 쓰기 0건.

**CR-1 게이트:** core/persona/storage tests + 빈 DB/기존 DB/손상 DB 복구 fixture PASS.

### CR-2 — 자유 대화 및 자유 출력 계약

#### CR-2A AgentRequest/AgentEvent v1

- **파일:** `mentat-inference/src/types.rs`, `lib.rs`, `fake.rs`
- **산출:** messages 기반 request, `TextDelta`, terminal status, capability flags
- **수용:** chat-only request에는 repository tool 0개; terminal event 후 delta 0개.

#### CR-2B 공급자 chat-only mapping

- **파일:** `mentat-inference-openai/src/gemini_adapter.rs`, `openai_adapter.rs`, contract fixtures
- **수용:** Gemini/OpenAI/OpenRouter가 같은 message 의미와 Markdown stream을 보존.

#### CR-2C ConversationOrchestrator chat path

- **파일:** `mentat-analysis/src/conversation_orchestrator.rs`, core storage ports, storage adapter
- **수용:** 저장소 없이 잡담 성공, 완료 전후 Markdown 동일, Advisor 취소는 `AdvisorPartialMarkdown` message로 저장.

**CR-2 게이트:** FR-027~029/033/046 chat-only acceptance PASS. 구형 AnswerBundle은 기본 경로에서 호출 0건.

### CR-3 — 읽기 전용 Repository Tool Gateway 및 Agent Loop

#### CR-3A 도구 타입·registry·gateway

- **파일:** `mentat-analysis/src/*`, `mentat-core/src/ports.rs`, `mentat-repository/src/session.rs`
- **산출:** 6개 tool schema, argument validator, bounded results, omissions, `SourceRef`
- **수용:** 상대 경로/canonical root/symlink/binary/size 경계 PASS; write/process symbol 0개.

#### CR-3B Agent loop state machine

- **파일:** `mentat-inference/src/types.rs`, orchestration module, fake backend
- **산출:** round/call budget, loop fingerprint, shared cancellation, `GroundingTrace`
- **수용:** 8/24/256KiB/5분 한도, 반복 인자 loop 중단, 부분 근거 보고.

#### CR-3C Native tool adapters

- **파일:** Gemini/OpenAI adapters와 wire fixtures
- **수용:** tool call ID/arguments/result/final Markdown 의미 동등성.

#### CR-3D Emulated planner fallback

- **파일:** inference orchestration과 strict planner schema fixtures
- **수용:** schema 오류 자동 복구 1회, planner JSON은 사용자 UI에 노출 0건.

**CR-3 게이트:** 질문에 경로명이 없어도 content search로 구현 위치 발견; 악성 저장소 지시가 capability를 변경하지 않음.

CR-3 완료 시점의 external provider tool path는 authorization hook까지 연결하되 repository 원문 전송은 항상 차단 상태다. CR-4의 typed consent와 durable receipt가 통과한 뒤에만 external repository tool result 전송을 활성화한다.

### CR-4 — 동적 Egress·동의·출처 검증

#### CR-4A Consent scope state machine

- **파일:** `mentat-analysis/src/consent.rs`, core types, app integration tests
- **수용:** `None → RequestOnce/RepositorySession → Revoked`; 승인 전 repository bytes 0; 저장 OFF/ephemeral에서는 external repository egress 0.

#### CR-4B ToolEgressReceipt canonical seal

- **파일:** core receipt/store ports, `mentat-analysis/src/tool_egress.rs`, `mentat-storage` v4 migration, receipt fixtures
- **수용:** conversation/turn/call/repo/snapshot/path/range/hash/profile/model/redaction/payload digest 중 하나의 tamper도 전송 전 차단. `Prepared` durable write 실패 시 transport 0, crash 결과는 `OutcomeUnknown`.

#### CR-4C SourceRef validator와 stale 재읽기

- **파일:** analysis evidence, repository read path, tests
- **수용:** current hash 불일치 시 old body 전송 0; invalid source는 drawer에서 제외/경고.
- **추가 경계:** 실제 file SHA-256, SourceRef identity hash, redacted payload digest를 혼용하지 않음; watcher disconnect는 STALE.

**CR-4 게이트:** 동적 tool batch tamper matrix, secret/high-entropy redaction, local backend egress 0 PASS.

### CR-5 — 세로형 자유 대화 UI와 설정

#### CR-5A 고정 크기 전환 제거

- **파일:** `mentat-app/src/main.rs`, `app.rs`, `mentat-storage/src/db.rs`, storage/viewport tests
- **저장 계약:** egui logical points, resize 종료 500ms debounce와 orderly close에 저장. 최초 restore frame은 저장값을 overwrite하지 않음. finite/positive 확인 후 최소 240×360과 현재 monitor work area로 clamp
- **수용:** 최초 312.5×660, 최소 240×360, 사용자 600×800 resize가 모든 상태 전환과 재실행 뒤 유지.

#### CR-5B 반응형 header와 timeline

- **파일:** `widgets/chat_header.rs`, `chat_timeline.rs`, `responsive.rs`, `widgets/mod.rs`
- **수용:** repository/model/mode 상태 2행 header, ordered message/status timeline, breakpoint별 단일/보조 panel과 세로 scroll.

#### CR-5C Composer와 Markdown message

- **파일:** `widgets/chat_composer.rs`, `markdown_message.rs`, app input tests
- **렌더러:** `pulldown-cmark 0.13.x`(locked 0.13.4), default features off; app-owned event renderer, image/html/resource loading 금지
- **수용:** 다중행 2~6행, Enter 전송/Shift+Enter 줄바꿈, stream/cancel/partial 상태, 장문 세로 스크롤, 코드 블록 가로 스크롤/복사.
- **입력 설정:** `ComposerSubmitMode::EnterSend` 기본(Shift+Enter newline), `CtrlEnterSend` 대안(Enter newline, Ctrl+Enter send); IME composing Enter는 전송 0

#### CR-5D Prompt 설정 편집기

- **파일:** `widgets/settings_panel.rs`, `prompt_editor.rs`, `privacy_panel.rs`, prompt draft state, storage integration tests
- **수용:** Kernel read-only; System/Persona apply/cancel/reset/version restore; API key 미리보기 0.
- **dirty close:** settings/new conversation/app close 전에 `계속 편집` 또는 `변경사항 폐기`를 선택하며 자동 저장하지 않음

**CR-5 게이트:** 240/250/479/480/759/760px headless layout + Windows 실제 resize/restart/keyboard smoke PASS.

### CR-6 — Grounding UX 및 Audit Mode 이관

#### CR-6A Grounding drawer

- **파일:** `widgets/grounding_drawer.rs`, `file_inspector.rs`, `widgets/mod.rs`
- **산출:** 메시지별 `근거 N개`, path/range/status/excerpt, file jump
- **수용:** 일반 화면 UUID/hash/confidence 강제 노출 0; invalid ref 명시.

#### CR-6B Audit Mode 격리

- **파일:** `widgets/audit_panel.rs`, `conversation_orchestrator.rs`, `answer_bundle.rs`, storage audit result adapter
- **산출:** 기존 AnswerBundle/Claim/Conflict UI를 명시적 mode로 이동
- **수용:** toggle은 다음 turn부터만 적용; Ready repo+Advisor capability 없으면 cloud Audit disabled; turn에 `ResponseContract`/validated Audit result 저장; Advisor call graph에서 Audit schema와 `compose_verified_answer` 0회; Audit 취소는 `AuditNoContent`이며 내부 JSON buffer의 UI/DB 노출 0바이트.

#### CR-6C 고급 workflow 이관

- **파일:** `widgets/advanced_menu.rs`, app routing tests
- **산출:** slash/quick chips를 고급 메뉴로 이동
- **수용:** “프롬프트만” 요청은 별도 builder 없이 assistant Markdown code block.

**CR-6 게이트:** FR-034/042/043 및 CON-009/010/015/018 PASS.

### CR-7 — 공급자 동등성·마이그레이션·안정화

#### CR-7A Capability verification

- **산출:** `CHAT_CAPABLE`, `NATIVE_TOOL_CAPABLE`, `EMULATED_TOOL_CAPABLE`, `REPOSITORY_ADVISOR_CAPABLE`
- **수용:** chat-only 모델이 repository fact를 조사한 것처럼 표시되지 않음.

#### CR-7B Legacy migration/recovery

- **산출:** CR-1/3/4 migration의 legacy fixture compatibility와 Windows/Linux/macOS recovery 재검증. 기존 AnswerBundle DB row는 없으므로 거짓 ChatMessage 변환 없음
- **수용:** 실제 Persona DB row가 없으므로 `Intermediate + DefaultAnalyst` seed, 기존 provider/DB 보존, CR-1 backup/quarantine/ephemeral 경계의 플랫폼 동등성, `.ok()`/임의 decode fallback 0.

#### CR-7C Context compaction

- **동결 기준:** message 40개 또는 직렬화 64KiB 초과 시, 최근 12개 원문을 유지하고 이전 완료 message를 compact summary로 교체
- **수용:** follow-up 대상·결정·미확정 질문 보존, 원문 code/secret 기본 로그 0.

#### CR-7D 플랫폼 안정화

- **수용:** Windows/Linux/macOS current-target release, accessibility, storage path, dialog/clipboard smoke.

**CR-7 게이트:** provider별 native 또는 emulated advisor contract PASS; 실계정 부재는 `UNVERIFIED EXTERNAL ENVIRONMENT`로 명시.

### CR-8 — 3-pass 감사 및 최종 재감사

새 기능 구현을 금지한다.

1. `CR-UX-001_PASS1_IMPLEMENTATION.md`
2. `CR-UX-001_PASS2_CAUSAL_UX.md`
3. `CR-UX-001_PASS3_SECURITY.md`
4. finding 수정 후 각 관련 pass 전체 재감사
5. clean commit 기준 `CR-UX-001_FINAL_REAUDIT.md`

**최종 게이트:** FR-027~047 Implemented+Verified, NFR-014~024 Verified, CON-009~019 위반 0, CR finding 0, 기존 read-only/security regression 0.

## 5. 공통 검증 명령

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture
cargo build --release --locked -p mentat-app
cargo audit --no-fetch --file Cargo.lock
git diff --check
```

`cargo audit`의 기존 SEC-F007 Accepted Risk는 owner/expiry/review trigger로만 유지하며 실패를 PASS로 바꾸지 않는다.

## 6. 리스크와 완화

| 위험 | 영향 | 완화 |
|---|---|---|
| 자유 Markdown이 검증을 약화 | 높음 | 답변과 GroundingTrace 분리, SourceRef gateway 소유, 무근거 저장소 fact 경고 |
| emulated planner JSON 노출 | 중간 | planner event 내부 전용 타입, UI projection compile/test 차단 |
| prompt 편집으로 권한 상승 | 치명 | immutable Kernel + capability whitelist + adversarial prompt tests |
| 동적 tool egress TOCTOU | 치명 | batch별 canonical receipt와 live hash 재검증 |
| DB migration 데이터 손실 | 높음 | transaction, timestamp backup, quarantine, destructive reset 금지 |
| 250px UI 정보 과밀 | 중간 | 단일 column, drawer, breakpoint tests, 강제 expansion 금지 |
| 공급자 tool dialect drift | 높음 | 공통 semantic contract + provider golden fixtures + capability 제한 |
| 장기 대화 비용/누락 | 중간 | 동결 compaction threshold와 follow-up regression fixtures |

## 7. 실행 상태와 잔여 게이트

2026-08-19 사용자 승인을 수신해 구현이 진행 중이다.

- 구현 방향을 바꾸는 미확정 아키텍처 결정: 0
- 사용자 결정 대기: 0

| 단계 | 현재 상태 | 잔여 |
|---|---|---|
| CR-0 | PASS | 없음 |
| CR-1 | PASS | privacy wipe retention hardening은 CR-7 재검증 |
| CR-2 | PASS | provider-native message golden fixture 보강 |
| CR-3 | Partial Verified | gateway/local loop 완료, cloud native/emulated body gate 연결 필요 |
| CR-4 | Partial Verified | seal/v4 receipt store 완료, adapter exact-body authorization 연결 필요 |
| CR-5 | Partial Verified | 실제 250px UI/prompt 확인, 600×800 실제 drag smoke 미실행 |
| CR-6 | Partial Verified | Audit validator/store 완료, Grounding drawer/Audit projection 연결 필요 |
| CR-7 | In Progress | capability tool probe, context compaction, platform 재검증 |
| CR-8 | Not Started | 독립 3-pass audit |

```text
CR-UX-001 Plan: APPROVED
CR-0 Documentation: PASS
Implementation: AUTHORIZED — CR-3~4 IN PROGRESS
Approval received: 2026-08-19 CR-UX-001 GO
```

`Implementation coverage: 100%`는 CR-UX-001 범위 43개와 직접 영향 legacy overlay를 뜻한다. 기존 baseline의 비관련 Partial이 남으면 전체 제품 100% 완료라고 표현하지 않으며 별도 baseline 상태를 함께 보고한다.
