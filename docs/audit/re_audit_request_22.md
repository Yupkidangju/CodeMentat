# CR-UX-001 Atomic Durability 독립 재감사 요청서 (Request 22)

- 요청일: 2026-08-19
- 감사 대상 clean commit: `b0ef4255a28d551f821c56c0e6abda41d87c164f`
- 원 감사: `docs/audit/audit_report_21.md`
- 요청 상태: `INDEPENDENT RE-AUDIT REQUESTED — PASS 미선언`
- 기존 감사 보고서 수정: 없음

## 1. 재감사 대상

report21의 `DBG-CRUX-F001`, `SEC-CRUX-F002`를 다음 두 durable transaction으로 수정했다.

```text
AgentEvent::Completed
→ ActiveTurn.pending_completion (아직 UI/DB Completed 아님)
→ AgentFinished(final GroundingTrace)
→ BEGIN IMMEDIATE
   → final trace/tool/source upsert
   → Advisor/Audit message+turn terminal
   → Audit result
→ COMMIT
→ 성공 후에만 UI Completed/Grounding/Audit projection
```

```text
ProviderBodyEgressGate.finish(receipt_ids, terminal)
→ ID 중복/빈 batch/expected status/exact-body digest 전수 검증
→ BEGIN IMMEDIATE
→ 모든 receipt Prepared→Sent|Failed|OutcomeUnknown
→ COMMIT 또는 전체 rollback
```

## 2. DBG-CRUX-F001 수정 증거

- `MentatChatApp`은 completion event를 `pending_completion`에만 저장한다.
- `AgentFinished`의 final trace ID와 completion trace ID가 다르면 Failed로 닫는다.
- repository-backed completion은 `SqliteStorage::finish_turn_with_grounding`만 사용한다.
- storage transaction은 DB turn의 conversation/snapshot과 trace binding을 다시 검증한다.
- final trace/tool/source와 Advisor 또는 Audit terminal/Audit result를 같은 transaction에 쓴다.
- transaction 실패 시 UI를 Completed로 바꾸지 않고 safe Failed 처리를 시도한다.

회귀 `terminal_and_final_grounding_killpoint_roll_back_together`는 다음을 검증한다.

1. 빈 trace와 Streaming message를 준비한다.
2. final source/tool을 transaction에 쓴 뒤 commit 직전 killpoint를 발생시킨다.
3. connection을 drop하고 DB를 다시 연다.
4. message가 Streaming이고 trace source가 0건임을 확인한다.
5. 정상 atomic retry 뒤에만 Completed와 full source가 함께 나타난다.

## 3. SEC-CRUX-F002 수정 증거

- `DurableToolEgressGate::finish`는 receipt별 CAS 반복을 제거하고 batch API를 한 번 호출한다.
- batch는 모든 ID가 존재하고 `Prepared`이며 같은 `exact_provider_body_digest`를 갖는지 update 전에 확인한다.
- 두 번째 update 직전 killpoint는 첫 번째 UPDATE까지 실행한 뒤 오류를 반환하며 transaction drop으로 둘 다 Prepared로 rollback한다.
- mixed exact-body digest batch도 update 전 거부하며 부분 terminal을 남기지 않는다.
- 정상 batch만 두 receipt를 함께 Sent로 만든다.
- 앱 재실행 시 남은 Prepared receipt 전체는 startup `BEGIN IMMEDIATE`에서 `OutcomeUnknown`으로 reconciliation된다. receipt table이 없는 sparse legacy fixture는 no-op으로 보존한다.

관련 회귀:

- `second_receipt_update_failure_rolls_back_entire_body_batch`
- `receipt_batch_rejects_mixed_exact_body_digests_without_partial_update`
- `restart_reconciles_stale_prepared_body_batch_to_outcome_unknown`
- `durable_gate_prepares_verifies_and_finishes_exact_body_receipt` — 동일 body 2개 receipt

## 4. clean commit 실행 증거

| 게이트 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 165 passed, 0 failed, 2 ignored |
| native credential ignored smoke | PASS |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, scan 81,645ms, peak 46,223,360 bytes < 128MiB |
| release build | PASS |
| 6-target build plan dry-run | PASS |
| AppData secret pattern / forbidden secret paths | PASS — 0 / 0 |
| Git | PASS — `b0ef425` clean |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |

`cargo audit`는 sandbox 밖에서도 다시 실행했다. `quick-xml 0.30.0`의 `RUSTSEC-2026-0194/0195`와 `paste`/`ttf-parser` unmaintained 경고는 기존 `SEC-F007` Accepted Risk로만 추적하며 명령 실패를 PASS로 바꾸지 않는다.

## 5. 독립 감사 요청

감사자는 다음을 코드와 실제 DB reopen 결과로 다시 판정한다.

1. completion event와 `AgentFinished` 사이 kill/crash에서 Completed+empty trace가 0건인지 확인한다.
2. Advisor와 Audit 모두 terminal+final trace 단일 transaction을 사용하는지 확인한다.
3. 두 번째 receipt update 실패 후 동일 body receipt가 `Sent+Prepared`로 갈리지 않는지 확인한다.
4. mixed body/duplicate/empty/missing/non-Prepared batch가 update 전에 거부되는지 확인한다.
5. restart Prepared reconciliation이 active runtime receipt를 오판하지 않고 시작 시점 stale row만 OutcomeUnknown으로 닫는지 확인한다.
6. report21의 기존 통과 영역과 43개 `29/9/5` 추적 상태가 회귀하지 않았는지 확인한다.

이 요청서는 감사 PASS가 아니다. `AI_AUDIT_DOC_STANDARD.md`와 `audit_roadmap.md`에 따라 새 독립 보고서가 `DBG-CRUX-F001`, `SEC-CRUX-F002`, `SEC-F007`을 재판정해야 한다.
