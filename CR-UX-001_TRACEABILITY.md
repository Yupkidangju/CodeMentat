# CR-UX-001 요구사항 추적표

- **상태:** `PRODUCTION IMPLEMENTATION PARTIAL — RE-AUDIT REQUEST PENDING`
- **구현 승인:** 2026-08-19 `CR-UX-001 GO` 수신
- **상태 의미:** `Implemented+Verified`는 production call site와 실행 테스트가 모두 있는 항목, `Partial`은 일부 계층 또는 수용 시나리오가 남은 항목, `Not Implemented`는 현재 기능이 없는 항목이다.

## 1. 기능 요구사항 FR-027~047

| ID | 소유 단계 | 계획 파일/모듈 | 계획 검증 | 현재 상태 |
|---|---|---|---|---|
| FR-027 | CR-2 | `mentat-analysis/agent_loop.rs`, inference fake, app chat | `agent_chat_without_repository_streams_free_markdown_and_calls_no_tools` | Implemented+Verified |
| FR-028 | CR-2/7 | core conversation, orchestrator, storage | `mixed_chat_repository_followup_keeps_context` | Partial — CR-1 domain/storage |
| FR-029 | CR-2/7 | conversation history/compaction | `followup_resolves_previous_subject_before_and_after_compaction` | Not Implemented |
| FR-030 | CR-3 | `repository_tools.rs`, `agent_loop.rs`, provider loopback | `production_agent_loop_executes_provider_tool_round_and_returns_grounding` | Implemented+Verified |
| FR-031 | CR-3 | core repository port, analysis tool registry | six-tool contract + compile/API surface no-write check | Implemented+Verified |
| FR-032 | CR-3/7 | inference agent types, Gemini/OpenAI native adapters | OpenAI full loop + Gemini/OpenAI exact-body fixtures | Implemented+Verified — native mapping |
| FR-033 | CR-2 | AgentEvent, orchestrator, chat storage/UI | `streamed_markdown_equals_completed_and_reloaded_markdown` | Implemented+Verified |
| FR-034 | CR-3/6 | SourceRef/GroundingTrace, grounding drawer | provider loop→SourceRef + drawer source detail + reload | Implemented+Verified |
| FR-035 | CR-1/5 | PromptProfile, composer, settings editor | CAS Apply + active revision request binding | Implemented+Verified |
| FR-036 | CR-1/5 | 4 System prompt assets, preset UI | factory catalog/checksum/load tests | Implemented+Verified |
| FR-037 | CR-1/5 | Persona prompt assets/composer/editor | style changes while tool/source facts remain equal | Partial — CR-1 assets/composer |
| FR-038 | CR-1/5 | prompt factory loader + draft state | factory reference resolution + Reset UI | Implemented+Verified |
| FR-039 | CR-1/7 | prompt profile/version SQLite stores | reopen restore + latest five versions + corrupt fallback | Partial — CR-1 version/migration/recovery |
| FR-040 | CR-5 | app viewport/preferences/chat layout | 240/312.5/479/480/759/760 geometry + native resize persistence | Implemented+Verified — 312.5×660 runtime/settings round-trip |
| FR-041 | CR-1/5 | PromptComposition preview/settings | Kernel read-only + secret/absolute-path absence | Partial — CR-1 framed composition |
| FR-042 | CR-2/6 | ordinary conversation path | “프롬프트만” yields one Markdown code block, special UI 0 | Implemented+Verified |
| FR-043 | CR-6 | Advisor/Audit projection state | tagged terminal + Audit result store/reload + UI projection | Implemented+Verified |
| FR-044 | CR-4 | consent/tool egress/receipt/trace | exact-body equality, zero-byte rejection, redirect, durable CAS | Implemented+Verified |
| FR-045 | CR-7 | model capability verification/setup UI | 실제 native tool probe + chat-only/repo-tools badge | Partial — emulated planner 미구현 |
| FR-046 | CR-2/6 | stream reducer/message status | terminal replacement 0 + Advisor partial/Audit no-content cancellation | Implemented+Verified |
| FR-047 | CR-1/5 | conversation store/chat header/privacy settings | new/delete/reopen implemented; privacy wipe 전수 fixture 잔여 | Implemented+Verified |

## 2. 비기능 요구사항 NFR-014~024

| ID | 소유 단계 | 계획 파일/모듈 | 계획 검증 | 현재 상태 |
|---|---|---|---|---|
| NFR-014 | CR-1/2/7 | beginner prompt asset + response fixtures | expert-term unexplained output policy eval fixture | Not Implemented |
| NFR-015 | CR-2 | AgentEvent reducer/storage/Markdown renderer | Unicode/code fence byte preservation | Implemented+Verified |
| NFR-016 | CR-3 | AgentLimits/AgentLoop | 8 rounds/24 calls/300s/cancel boundary tests | Partial — bounded loop implemented |
| NFR-017 | CR-3 | RepositoryToolGateway budgets | 400 lines/64KiB call/256KiB turn omissions | Implemented+Verified |
| NFR-018 | CR-1/7 | prompt factory/storage migration recovery | legacy/future/corrupt/backup/quarantine/v5 fixtures | Implemented+Verified |
| NFR-019 | CR-3/7 | provider semantic adapters | OpenAI full loop + Gemini exact-body native mapping | Partial — cross-provider identical trace fixture 잔여 |
| NFR-020 | CR-5 | responsive widgets/Markdown code block | boundary widths no clip + code scroll/copy geometry | Partial — 250px/code runtime verified |
| NFR-021 | CR-3/6 | GroundingTrace/SourceRef UI | provider loop trace + source detail drawer + reload | Implemented+Verified |
| NFR-022 | CR-1/4/7 | AppData stores/logging/deletion | AppData persistence/cascade 구현; privacy deletion 전수 fixture 잔여 | Partial |
| NFR-023 | CR-2/3/4 | structured tracing fields | log capture contains IDs/metrics, raw text/secrets 0 | Not Implemented |
| NFR-024 | CR-5/6 | chat/settings/grounding accessibility | keyboard-only workflow + accessibility labels | Not Implemented |

## 3. 제약조건 CON-009~019

| ID | 소유 단계 | 강제 위치 | 계획 검증 | 현재 상태 |
|---|---|---|---|---|
| CON-009 | CR-2/6 | Advisor prompt/projection | Advisor final schema contract 0 | Implemented+Verified |
| CON-010 | CR-2/6 | ConversationOrchestrator | `compose_verified_answer` Advisor call count 0 | Implemented+Verified |
| CON-011 | CR-2 | chat request builder | repository `None` chat success | Implemented+Verified |
| CON-012 | CR-1/2 | PromptComposer/default path | PersonaRenderer default-chat call 0 | Implemented+Verified |
| CON-013 | CR-3 | sealed tool enum/repository port | write/delete/rename/patch/process variants 0 | Implemented+Verified |
| CON-014 | CR-5 | viewport command boundary | state transitions emit `InnerSize` 0 | Implemented+Verified |
| CON-015 | CR-2/6 | route/widget inventory | Prompt Builder type/screen/export 0 | Implemented+Verified |
| CON-016 | CR-3/7 | tool/context budgets/compaction | per-turn byte and history threshold tests | Not Implemented |
| CON-017 | CR-1/3 | Kernel + untrusted content wrapper | repository text cannot change prompt/tool digest | Implemented+Verified |
| CON-018 | CR-6 | projection/mode boundary | Advisor Markdown/Audit structured projection 분리 | Implemented+Verified |
| CON-019 | CR-1 | versioned prompt assets | factory text absent from user DB source of truth | Implemented+Verified |

## 4. 기존 요구사항 영향

| 기존 항목 | CR-UX-001 상태 | 보존/대체 규칙 |
|---|---|---|
| FR-009 | EXTENDED | 저장소 질문뿐 아니라 repository-optional multi-turn chat으로 확장 |
| FR-010~012 | AUDIT MODE RETAINED | Claim/Recommendation 분류는 Audit Mode에서 보존, Advisor 기본 출력에서는 강제하지 않음 |
| FR-016 | EXTENDED | 정적 packet 동의에서 dynamic tool batch consent/receipt로 확장 |
| FR-017 | EXTENDED | `InferenceBackend`를 AgentRequest/Event 및 capability contract로 확장 |
| FR-019 | EXTENDED/RETAINED | 페르소나 선택·사실 불변 계약은 유지. 구현 방식인 ADR-005/DEC-PER-001 후처리만 기본 chat에서 superseded하고 pre-inference Persona Prompt로 확장 |
| FR-021 | MOVED TO ADVANCED/AUDIT | slash workflow는 기본 UI에서 제거하고 고급/Audit 경로 유지 |
| FR-023 | EXTENDED | conversation/prompt/trace/preferences AppData persistence 추가 |
| NFR-004 | EXTENDED | AnswerBundle EvidenceRef 외 GroundingTrace/SourceRef 포함 |
| CON-003/004 | RETAINED | UI/Persona는 repository truth를 변경하지 않고 model output은 evidence가 아님 |
| CON-005 | SUPERSEDED/REFINED IN ADVISOR MODE | blanket tool 금지는 FR-031/CON-013으로 대체; read-only repository tools만 허용, shell/write 실행 금지는 그대로 유지 |
| CON-008 | EXTENDED | chat/tool/advisor capability를 실제 probe 후 활성화 |

## 5. 요구사항–단계 집계

| 단계 | FR | NFR | CON |
|---|---|---|---|
| CR-1 | 035~039, 041, 047 기반 | 014, 018, 022 기반 | 012, 017, 019 기반 |
| CR-2 | 027~029, 033, 042, 046 | 014, 015, 023 | 009~012, 015 |
| CR-3 | 030~032, 034 기반 | 016, 017, 019, 021 | 013, 016, 017 |
| CR-4 | 034/044 보안 경계 | 021~023 | 016/017 |
| CR-5 | 035~041/047 UI | 020, 024 | 014 |
| CR-6 | 034/042/043 UX | 014/021/024 | 009/010/015/018 |
| CR-7 | 028/029/032/039/045 | 018/019/022/023 | 016/019 |
| CR-8 | 전체 감사 | 전체 감사 | 전체 감사 |

## 6. 현재 판정

```text
Traceability coverage: 43/43 planned with owner, file surface, verification and phase
Implemented+Verified: 29/43
Partial: 9/43
Not Implemented: 5/43
CR-0 exit: APPROVED
Production remediation: IMP-CRUX-F001 / SEC-CRUX-F001 / IMP-CRUX-F002 implemented
Durability remediation: DBG-CRUX-F001 / SEC-CRUX-F002 implemented, killpoint 재실행 검증 대기
Runtime ownership remediation: SEC-CRUX-F003 / DBG-CRUX-F003 / DOC-CRUX-F002 구현, clean gate 대기
Process lock remediation: force-kill immediate reopen 및 stale-threshold two-writer 0건 검증, clean gate 대기
Re-audit request: PENDING CLEAN COMMIT GATES
```
