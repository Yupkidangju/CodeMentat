# D3D 재감사 보고서 (Turn 18 / Re-audit #16)

- 감사 일시: 2026-08-19
- 기준 commit: `c8684b416cc26de554dbda0b247477a0f35f0580`
- 감사 대상: 기준 commit 위 대규모 미커밋 working tree
- 최종 판정: **HOLD**

## 1. 요약

직전 PASS 대상과 달리 현재 working tree에는 agent loop, repository tools, tool egress, chat UI, native credential state, conversation/prompt/grounding storage, markdown renderer와 신규 통제 문서가 추가됐다. tracked 35개 파일에서 3,958 additions/551 deletions이며 신규 파일도 다수다. 이는 기존 finding의 단순 보완이 아니라 별도 아키텍처/보안 기능 확장이다.

Formatter, strict Clippy, 기본 workspace tests는 통과했지만 현재 변경은 commit으로 고정되지 않았고 신규 native credential ignored smoke와 100k/2GiB ignored profile을 이번 turn에서 재실행하지 못했다. 따라서 직전 `PASS WITH KNOWN RISKS`를 현재 working tree에 승계할 수 없다.

## 2. 실행 결과

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy | PASS |
| `cargo test --workspace --locked` | PASS — 147 passed, 0 failed, 2 ignored |
| `git diff --check` | PASS — LF→CRLF 경고 |
| Git 상태 | DIRTY — tracked 35개 수정 + 신규 파일 다수 |
| native credential ignored smoke | 미실행 |
| 100k/2GiB ignored gate | 미실행 |
| release build / cargo audit | 이번 확장 working tree의 최종 독립 gate 미완료 |

## 3. Findings

### [IMP-F002] 신규 agent/chat/storage 범위의 독립 감사가 필요하다

- Severity: Major
- Status: Needs Fix
- Evidence: agent tool execution, prompt composition, native secret reference, durable conversation/grounding receipt, markdown link handling 등 새로운 trust boundary가 호출 가능한 상태로 추가됐다.
- Expected: 신규 범위의 요구사항·위협모델·상태기계·egress·persistence migration을 독립 3-pass 감사한다.
- Actual: 기존 Re-audit #15 PASS 이후 한 번에 약 4천 줄이 추가됐다.
- Suggested Fix: 변경을 commit으로 고정한 뒤 agent loop/tool egress/prompt injection/secret store/DB migration/chat renderer를 별도 감사 범위로 제출한다.

### [DBG-F006] 현재 release candidate가 재현 불가능하다

- Severity: Major
- Status: Needs Fix
- Evidence: 기준 HEAD와 실제 테스트 대상 working tree가 다르고 신규 파일이 Git에 포함되지 않았다.
- Expected: 전체 source/docs/assets/lockfile을 clean commit으로 고정한다.
- Actual: 대규모 DIRTY 상태다.
- Suggested Fix: 관련 변경을 의도적 commit으로 고정하고 모든 ignored/release/security gate를 재실행한다.

## 4. 상태

- Critical: 미판정 — 신규 보안 범위의 전체 감사 전
- Major: 2
- 판정: **HOLD**
- 기존 `SEC-F007` Accepted Risk는 유지하되 신규 범위 감사 결과를 대체하지 않는다.

## 5. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_18.md`를 확인하세요.
현재 agent/chat/storage/security 확장을 전체 source/docs/assets와 함께 clean commit으로 고정하세요.
native credential ignored smoke, 100k/2GiB, release build, cargo audit를 실행한 뒤
agent loop/tool egress/prompt injection/secret storage/DB migration/markdown renderer 전체 감사를 요청하세요.
기존 감사 보고서는 수정하지 마세요.
```
