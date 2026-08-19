# Code Mentat CR-UX-001 감사 로드맵

- **문서 버전:** 2.0.0-plan
- **참조 표준:** `AI_AUDIT_DOC_STANDARD.md`
- **기존 baseline 감사:** `docs/audit/audit_report_17.md` — `PASS WITH KNOWN RISKS`
- **최신 확장 범위 감사:** `docs/audit/audit_report_19.md` — `HOLD`
- **신규 변경 상태:** `CR-0~2 PASS / CR-3~7 PARTIAL / CR-8 HOLD`
- **전체 판정:** `HOLD — Major 4건 및 SEC-F007 Accepted Risk`

기존 audit PASS는 `c8684b4` 이전 구조화 조언자 baseline에 대한 판정이다. CR-UX-001 구현 완료를 의미하지 않는다. 기존 SEC-F007 Accepted Risk는 그대로 유지한다.

## 1. 단계 감사 원칙

| Pass | 질문 | CR-UX-001 통과 기준 |
|---|---|---|
| Pass 1 — Implementation | 기본 실행 경로가 새 요구사항과 실제로 연결됐는가? | 요구사항→domain→provider→storage→UI 역추적, orphan/dead surface 0 |
| Pass 2 — Causal/UX/Engineering | 상태 연결과 사용자 경험이 회귀 없이 재현되는가? | 결정적 fixtures, build, persistence/restart, 17장 수용 시나리오 |
| Pass 3 — Security/Privacy | 자유 prompt/tool loop가 read-only/egress/secret 경계를 약화하지 않았는가? | capability absence, tamper, prompt injection, cancel, deletion tests |

각 CR 단계는 구현자가 자체 게이트를 통과한 뒤에만 독립 감사 대상으로 넘어간다. 구현자 테스트는 감사 PASS를 대신하지 않는다.

## 2. CR 단계별 감사 게이트

| 단계 | Pass 1 | Pass 2 | Pass 3 | 출구 |
|---|---|---|---|---|
| CR-0 | 43/43 owner/file/test/phase, supersede closure | 문서 명령·수치·상태 일치 | 위협/동의/receipt/삭제 결정 closure | 사용자 GO |
| CR-1 | conversation/prompt/store 실제 연결 | migration/reopen/factory checksum | corrupt DB, prompt elevation, AppData boundary | CR-1 PASS |
| CR-2 | no-repo chat, free Markdown path | stream=complete=reload, cancel | prompt/log secret absence | CR-2 PASS |
| CR-3 | six tools + agent loop/provider mapping | round/call/byte/loop/cancel | traversal/injection/no-write API | CR-3 PASS |
| CR-4 | consent/receipt/SourceRef path | generation/stale/revoke transitions | canonical tamper/secret/live hash | CR-4 PASS |
| CR-5 | vertical UI/prompt settings wired | breakpoints/resize/restart/keyboard | prompt preview/no remote Markdown fetch | CR-5 PASS |
| CR-6 | grounding/Audit/advanced mode wiring | Advisor/Audit causal isolation | invalid citation no privilege | CR-6 PASS |
| CR-7 | provider parity/migration/capability UI | long context/platform/recovery | honest limitation/credential/DB failure | CR-7 PASS |
| CR-8 | full requirements trace | all scenarios + clean builds | full attack suite + accepted risks | final re-audit |

## 3. Pass 1 필수 call graph

```text
User input
→ ChatMessage(Pending)
→ active PromptVersion
→ PromptComposer
→ AgentRequest
→ provider adapter
→ AgentEvent stream
→ ChatMessage terminal state/storage
→ Advisor UI Markdown
```

Repository 질문은 다음을 추가한다.

```text
ToolCallRequested
→ analysis AgentLoop
→ RepositoryToolGateway
→ ReadOnlyRepository
→ sanitized ToolResult + SourceRef
→ consent/receipt
→ provider next round
→ GroundingTrace
→ evidence drawer
```

기본 Advisor call graph에 다음이 있으면 CR-2/6은 실패다.

- AnswerBundle JSON system contract
- `compose_verified_answer` 본문 교체
- PersonaRenderer intro/outro
- repository absence hard block
- static top-8/first-60-lines를 유일한 조사 경로로 사용

## 4. Pass 2 필수 연결 감사

- repository 없음 ↔ chat-only ↔ tool 0
- Ready/STALE/Incomplete ↔ tool availability ↔ reindex 안내
- prompt draft ↔ apply ↔ next turn ↔ restart ↔ restore
- Persona prompt ↔ style ↔ SourceRef/fact 불변
- resize ↔ settings/evidence/Audit ↔ restart restore
- stream delta ↔ terminal ↔ persistence ↔ reload
- native ↔ emulated tool ↔ 같은 semantic events
- invalid SourceRef ↔ drawer ↔ Markdown body 불변
- follow-up ↔ compact summary ↔ prior decision/unknown 유지
- conversation delete ↔ cascade ↔ backup/quarantine privacy cleanup

## 5. Pass 3 필수 공격/실패 시험

1. absolute/`../`/symlink/reparse/nested root
2. binary/oversized file/line/turn budget
3. repository prompt injection과 editable prompt 권한 상승
4. tool name/argument/schema mutation과 repeated loop
5. consent/turn/repo/snapshot/profile/model/path/range/hash/payload tamper
6. API key/token/private key/high entropy redaction
7. redirect, endpoint change, response size/JSON/SSE error
8. file changed during read, watcher channel disconnect, STALE
9. cancel/timeout/stream disconnect/tool race
10. migration failure, malformed UUID/date/enum, future schema version
11. conversation/privacy delete failure and reopen
12. Markdown image/file/http/data automatic load 0

## 6. 단계 공통 명령

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --release --locked -p mentat-app
git diff --check
```

CR-7/8 추가:

```text
cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture
cargo audit --no-fetch --file Cargo.lock
cargo mentat-build build --platform all --profile release --dry-run
```

## 7. 감사 결과물

```text
docs/audit/CR-UX-001_PASS1_IMPLEMENTATION.md
docs/audit/CR-UX-001_PASS2_CAUSAL_UX.md
docs/audit/CR-UX-001_PASS3_SECURITY.md
docs/audit/CR-UX-001_FINAL_REAUDIT.md
```

각 finding은 기존 ID 또는 CR-specific ID를 유지하고 evidence, expected/actual, impact, fix, re-audit method를 포함한다. 감사자는 코드를 수정하지 않는다.

## 8. Completion 판정 범위

- CR completion coverage는 FR-027~047, NFR-014~024, CON-009~019과 직접 영향 legacy overlay다.
- 비관련 baseline Partial은 CR PASS를 자동 차단하지 않지만 전체 제품 100% 완료 주장을 차단한다.
- CR 범위 Critical/High/Major/Minor 미해결 0건이어야 한다.
- SEC-F007은 owner/expiry/review trigger를 갖는 기존 Accepted Risk이며 `cargo audit` 실패를 PASS로 표기하지 않는다.

## 9. 현재 상태

```text
Baseline audit: PASS WITH KNOWN RISKS (audit_report_17)
Expanded working-tree audit: HOLD (audit_report_18)
Clean-commit full re-audit: HOLD (audit_report_19)
CR-UX-001 CR-0: APPROVED
CR-UX-001 CR-1~7: PARTIAL — AgentLoop/egress/Grounding·Audit 연결 필요
Implementation approval: RECEIVED
Final decision: CR-8 HOLD — Major 4건
```
