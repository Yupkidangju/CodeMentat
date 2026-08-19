# D3D 재감사 보고서 (Turn 21 / Re-audit #19)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_19.md`, `docs/audit/audit_report_20.md`
- 재감사 요청: `docs/audit/re_audit_request_21.md`
- 기준 commit: `453053950133b6ac43b7ae618a89906b36db0dfe`
- 핵심 구현 commit: `3e0eff8` / 추적 동기화 `4918c3e`
- 감사 대상: clean commit 전체
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 요약

report19/20의 production wiring 문제는 실질적으로 개선됐다. 기본 Chat UI는 `AgentLoop::new`를 단일 진입점으로 사용하고, Ready repository와 검증된 provider capability에서 6개 read-only tool을 제공한다. OpenAI/Gemini adapter는 tool result가 포함된 최종 JSON bytes를 만든 뒤 `ProviderBodyEgressGate`를 호출하고, 같은 bytes를 요청 body로 보낸다. 승인 거부·redirect·unknown/write tool·stale snapshot·secret redaction 회귀도 통과한다. Grounding drawer와 Audit tagged projection, SQLite restore도 production UI에 연결됐다. 43개 추적표는 `29 Implemented+Verified / 9 Partial / 5 Not Implemented`로 보정되어 과대 완료 주장을 제거했다.

그러나 terminal durability가 두 곳에서 원자적이지 않다. Chat UI는 `AgentEvent::Completed`를 먼저 받아 turn/message/Audit terminal을 저장하고, 이후 `AgentFinished`에서 실제 GroundingTrace 상세를 별도 transaction으로 저장한다. 두 단계 사이 crash나 trace 저장 실패 시 완료 답변이 빈 grounding trace에 연결된다. 또한 하나의 provider body에 여러 tool result가 포함될 때 receipt terminal 상태는 개별 SQL CAS 반복으로 갱신되어 crash/DB 실패 시 같은 body의 receipt들이 Sent/Prepared로 갈릴 수 있다.

Critical 누출은 확인되지 않았지만 Major 2건이 남아 **HOLD**다.

## 2. 실행 게이트

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 161 passed, 0 failed, 2 ignored |
| native credential ignored smoke | PASS — put/get/delete |
| 100k/2GiB ignored profile | PASS — scan 89,997ms, preview 3,358,720 bytes, peak 46,198,784 bytes < 128MiB |
| `cargo build --release --locked -p mentat-app` | PASS |
| 6-target build dry-run | PASS |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |
| forbidden tracked secret path scan | PASS — 0건 |
| `git diff --check` / Git status | PASS / CLEAN |

`SEC-F007`은 Windows 비도달, owner `@Yupkidangju`, expiry `2026-11-30`, eframe/accesskit/Linux review trigger를 근거로 Accepted Risk를 유지한다.

## 3. report19 Findings 재판정

| Finding | 상태 | 근거 |
|---|---|---|
| `IMP-CRUX-F001` Chat UI AgentLoop 우회 | **Verified** | `submit_chat` → `AgentLoop::new`; direct round 우회 없음 |
| `SEC-CRUX-F001` provider exact-body egress | Needs Fix (Major, 부분 개선) | body gate 연결 완료, multi-receipt terminal atomicity 미완료 |
| `IMP-CRUX-F002` Grounding/Audit 사용자 경로 | Needs Fix (Major, 부분 개선) | UI/restore 연결 완료, terminal+trace crash atomicity 미완료 |
| `DOC-CRUX-F001` 상태 문서 불일치 | **Verified** | 29/9/5 및 current/partial/not-implemented 명시 |

## 4. Findings

### [DBG-CRUX-F001] 완료 terminal과 최종 GroundingTrace가 원자적으로 저장되지 않는다

- Pass: Debug / Architecture
- Severity: Major
- Status: Needs Fix
- Evidence:
  - turn 시작 시 `chat_app.rs:1302-1312`가 같은 trace ID의 빈 GroundingTrace를 먼저 저장한다.
  - `AgentLoop`는 `AgentEvent::Completed`를 event sink로 전송한 뒤 `agent_loop.rs:222`에서 receipt IDs를 최종 trace에 붙이고 outcome을 반환한다.
  - 앱은 `chat_app.rs:1609-1659`의 Completed 처리에서 assistant message/turn/Audit terminal을 먼저 `finish_turn()`으로 저장한다.
  - 이후 별도 `AsyncResult::AgentFinished`가 도착해야 `chat_app.rs:1554-1561`에서 tool calls/source refs가 들어간 trace를 `prepare_grounding_trace()`로 덮어쓴다.
  - 두 DB 작업은 서로 다른 transaction이며 atomic ordering test/killpoint가 없다.
- Expected: completed message/turn/Audit result와 최종 GroundingTrace/tool/source/receipt 결속이 하나의 durable transaction으로 확정되어야 한다.
- Actual: crash 또는 final trace 저장 실패 시 completed turn이 빈 trace ID를 참조한다. UI는 완료 답변을 보여주지만 재실행 후 근거가 0건일 수 있다.
- Impact: grounded answer와 Audit 결과의 복구 무결성이 깨지고 완료 상태가 실제 durable evidence보다 앞선다.
- Suggested Fix: terminal payload와 final trace를 하나의 outcome으로 전달하고 `finish_turn_with_grounding(trace, terminal_update)`를 `BEGIN IMMEDIATE` 단일 transaction으로 구현한다. 성공 후에만 UI를 Completed로 전환한다.
- Re-audit Method: empty trace 준비 후 final trace 저장 직전/turn terminal 직전 killpoint를 주입해 재실행 시 Completed+full trace 또는 Failed/Interrupted 중 하나만 남는지 검증한다.
- Owner: Coder / Storage

### [SEC-CRUX-F002] 한 provider body의 receipt terminal 전이가 batch atomic이 아니다

- Pass: Security / Durability
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `DurableToolEgressGate::authorize_exact_body()`는 body 안의 tool result마다 Prepared receipt를 만든다.
  - `tool_egress_gate.rs:160-168`의 `finish()`는 receipt ID를 순회하며 `compare_and_set_tool_egress_status()`를 개별 실행한다.
  - storage CAS도 각 호출마다 독립 SQL UPDATE이며 batch transaction이 없다.
  - 한 body에 둘 이상의 tool result가 포함되면 첫 receipt 갱신 후 crash/DB 오류가 발생할 수 있다.
- Expected: 동일 exact body에 묶인 모든 Prepared receipt가 한 transaction에서 모두 Sent/Failed/OutcomeUnknown으로 전이되거나 전혀 전이되지 않아야 한다.
- Actual: 동일 네트워크 송신의 receipt 상태가 `Sent + Prepared`처럼 부분 완료될 수 있다.
- Impact: 감사 로그가 실제 단일 body 전송 결과를 일관되게 설명하지 못하고 Prepared recovery 판단이 모호해진다.
- Suggested Fix: `compare_and_set_tool_egress_status_batch(ids, Prepared, terminal)`을 단일 Immediate transaction으로 구현하고 expected row count가 다르면 rollback한다. startup recovery에서 stale Prepared를 OutcomeUnknown으로 reconcile한다.
- Re-audit Method: 2개 tool result body와 두 번째 update 실패/프로세스 kill fixture에서 partial terminal 0건을 검증한다.
- Owner: Coder / Security / Storage

## 5. 통과 영역

- AgentLoop production 연결과 chat-only 동일 진입점
- OpenAI/Gemini native tool mapping 및 exact-body zero-byte rejection/redirect 차단
- prompt Kernel framing과 editable prompt 격리
- native credential 저장소와 SQLite opaque reference
- v1~v5 migration, backup/quarantine, future schema 차단
- Markdown no-fetch/unsafe scheme/size-depth-event limits
- Grounding drawer/Audit projection 기능 및 정상 경로 reload
- 요구사항 추적표 current/partial/not-implemented 정합성

## 6. 상태 집계

- Critical: 0
- Major: 2 — `DBG-CRUX-F001`, `SEC-CRUX-F002`
- Minor: 0
- Accepted Risk: `SEC-F007` 1건
- 최종 판정: **HOLD**

## 7. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_21.md`를 기준으로 수정하세요.
완료 terminal과 최종 GroundingTrace를 단일 storage transaction으로 저장하고,
동일 provider body의 receipt terminal CAS를 batch atomic transaction으로 전환하세요.
crash/killpoint와 두 번째 receipt update 실패 fixture에서 partial durable state 0건을 검증한 뒤
clean commit으로 전체 gate와 ignored smoke를 재실행하세요. 기존 감사 보고서는 수정하지 마세요.
```
