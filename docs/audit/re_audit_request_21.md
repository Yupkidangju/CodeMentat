# CR-UX-001 독립 재감사 요청서 (Request 21)

- 요청일: 2026-08-19
- 감사 대상 clean commit: `4918c3ea9b4a2db18d2831685d65d6583ea97842`
- 원 감사: `docs/audit/audit_report_19.md`, `docs/audit/audit_report_20.md`
- 요청 상태: `INDEPENDENT RE-AUDIT REQUESTED — PASS 미선언`
- 기존 감사 보고서 수정: 없음

## 1. 재감사 목적

report19/20의 Major 4건을 production 경로에서 수정했다. 감사자는 구현자 자체 판정을 승계하지 말고 다음 인과 사슬을 실제 call site, loopback wire fixture, SQLite reopen, UI projection에서 다시 확인해야 한다.

```text
MentatChatApp
→ AgentLoop 단일 진입점
→ provider native tool call
→ RepositoryToolGateway read-only 실행/redaction
→ ProviderBodyEgressGate exact body 승인
→ canonical Prepared receipt durable 저장
→ 같은 bytes socket write / terminal CAS
→ AgentEvent contract terminal
→ GroundingTrace 또는 validated Audit result
→ Grounding drawer/Audit UI 및 restart restore
```

## 2. Finding별 수정 제출

### IMP-CRUX-F001 — Chat UI의 AgentLoop 우회

- 모든 기본 turn은 `AgentLoop::new`를 사용한다.
- 저장소 없음/chat-only도 같은 loop에서 tool catalog/context 없이 실행한다.
- Ready 저장소와 실제 native tool probe를 통과한 모델만 6개 tool/context를 받는다.
- `production_agent_loop_executes_provider_tool_round_and_returns_grounding`은 provider 2-round tool 호출, gateway 실행, final Markdown, SourceRef/trace를 함께 검증한다.

### SEC-CRUX-F001 — provider 송신 직전 canonical egress

- OpenAI 호환/Gemini adapter가 최종 JSON을 `Vec<u8>`로 만든 직후 `ProviderBodyEgressGate`를 호출한다.
- 앱의 durable gate가 consent scope/provider/full endpoint/model/conversation/turn/repository/snapshot/tool/ref/payload/exact body를 seal하고 `Prepared`를 저장한 뒤 같은 byte slice만 송신한다.
- gateway는 tool content와 SourceRef excerpt를 provider 직렬화 전에 secret/high-entropy redaction한다.
- 승인 거부는 network accept 0건, OpenAI redirect는 target 수신 0건, OpenAI/Gemini wire body는 gate body와 byte-for-byte 같음을 fixture로 검증한다.
- terminal은 `Sent`, `Failed`, `OutcomeUnknown` 중 하나로 Prepared에서 단방향 CAS한다.

### IMP-CRUX-F002 — Grounding/Audit production UI

- `AgentEvent::Completed` tagged payload를 Advisor Markdown과 validated Audit bundle로 분기한다.
- Advisor message의 별도 Grounding drawer가 tool/receipt 수와 SourceRef path/range/redacted excerpt를 표시한다.
- Audit은 Ready 저장소와 repository-advisor capability에서만 transient 선택할 수 있고 raw JSON delta를 timeline에 표시하지 않는다.
- GroundingTrace와 Audit result는 SQLite에서 message/turn에 결속해 재실행 후 복원한다.

### DOC-CRUX-F001 — 43개 추적 상태

- `CR-UX-001_TRACEABILITY.md`와 `spec.md`의 신규 43개 ID는 상태 mismatch 0이다.
- 현재 집계는 `Implemented+Verified 29 / Partial 9 / Not Implemented 5`다.
- emulated planner, context compaction, beginner eval, structured tracing, 접근성 전수 등 미완료 항목은 완료로 승격하지 않았다.

## 3. clean commit 실행 증거

| 게이트 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 161 passed, 0 failed, 2 ignored |
| native credential ignored smoke | PASS — OS store put/get/delete |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, scan 86,500ms, peak 46,161,920 bytes < 128MiB |
| release build | PASS |
| 6-target build plan dry-run | PASS |
| 실제 Windows release UI | PASS — 313×660, 한글, chat/settings, 자격증명 안내, 정상 종료 후 창 0 |
| AppData DB secret pattern | PASS — 0건 |
| forbidden secret path | PASS — 0건 |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — SEC-F007 기존 High 2 + unmaintained 2 |
| Git | PASS — `4918c3e` 검증 시작/종료 시 clean |

`cargo audit` 실패는 PASS로 바꾸지 않는다. `quick-xml 0.30.0`의 `RUSTSEC-2026-0194/0195`와 `paste`/`ttf-parser` unmaintained 경고는 기존 `SEC-F007` owner/expiry/review trigger로 재검토한다.

## 4. 필수 공격·회귀 재감사

1. Chat UI source에서 direct provider round 우회가 다시 생기지 않았는지 확인한다.
2. unknown/write tool, malformed/out-of-range arguments, stale/incomplete snapshot을 fail-closed로 검증한다.
3. consent/turn/repo/snapshot/profile/model/endpoint/ref/body mutation과 missing durable storage를 전수 검사한다.
4. send 전 거부, redirect, cancel, network ambiguity와 receipt terminal을 대조한다.
5. repository prompt injection과 secret fixture가 provider body 또는 UI에 원문으로 남지 않는지 검사한다.
6. Advisor/Audit terminal 교차 오염, invalid evidence, Audit raw JSON 비노출을 검사한다.
7. Grounding/Audit DB reopen 및 conversation cascade 삭제를 검사한다.
8. 43개 추적 row를 production call site/test와 전수 대조하고 Partial을 과대 승격하지 않았는지 확인한다.

## 5. 요청 판정

이 문서는 구현 완료 주장이나 감사 PASS가 아니다. 독립 감사자는 `AI_AUDIT_DOC_STANDARD.md`와 `audit_roadmap.md`의 3-pass 절차로 새 보고서를 생성하고, Critical/High/Major/Minor finding과 `SEC-F007`을 다시 판정해야 한다.

