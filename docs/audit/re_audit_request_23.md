# CR-UX-001 Runtime Ownership 독립 재감사 요청서 (Request 23)

- 요청일: 2026-08-19
- 감사 대상 clean commit: `a03da5d6991d7dd7e186fa4e7bc51f1e9bfbb4c5`
- 원 감사: `docs/audit/audit_report_22.md`
- 요청 상태: `INDEPENDENT RE-AUDIT REQUESTED — PASS 미선언`
- 기존 감사 보고서 수정: 없음

## 1. SEC-CRUX-F003 수정

SQLite schema v6에 singleton runtime ownership을 추가했다.

```text
SqliteStorage::open
→ migration/integrity 확인
→ BEGIN IMMEDIATE
→ runtime_ownership(owner UUID, PID, acquired_at, heartbeat_at) 확인
   ├─ heartbeat < 30초: STORAGE_RUNTIME_OWNED, write/recovery 0
   └─ owner 없음/true stale: 새 owner lease 획득
      → Prepared→OutcomeUnknown
      → orphan Pending/Streaming→INTERRUPTED_BY_RESTART Failed
→ COMMIT
→ 2초 heartbeat worker 시작
```

- `SqliteStorage::clone`은 같은 `Arc<RuntimeOwnershipGuard>`를 공유한다.
- 마지막 guard Drop은 heartbeat를 중단하고 자신의 owner row만 조건부 삭제한다.
- `initial_ui_preferences()` 임시 open은 반환 시 lease를 해제한 뒤 실제 app storage가 새 lease를 얻는다.
- 새 Prepared receipt는 해당 `runtime_owner_id`를 기록한다.
- live owner가 존재하면 같은 process의 두 번째 handle과 실제 child process 모두 거부한다.
- exclusive owner를 획득하기 전에는 Prepared 또는 unfinished turn을 변경하지 않는다.

## 2. corruption quarantine 축소

`should_quarantine()`은 다음만 허용한다.

- SQLite `DatabaseCorrupt`
- SQLite `NotADatabase`
- `PRAGMA integrity_check` 비정상 결과

busy, locked, read-only, permission, I/O, owner lease, receipt/turn recovery transaction 오류는 원본 DB/WAL/SHM을 보존하고 open을 fail-closed한다. 실제 invalid SQLite 파일은 기존 quarantine fixture에서 계속 격리된다.

## 3. orphan Streaming 및 문서 정합성

- exclusive startup owner는 runtime 작업이 없는 assistant `Pending`/`Streaming`을 `Failed { INTERRUPTED_BY_RESTART }`로 바꾸고 미완료 turn의 `completed_at`을 함께 기록한다.
- crash killpoint 후 DB를 다시 열면 Completed+empty trace나 영구 Streaming 대신 Failed+empty trace가 복원된다.
- `DEC-SEC-012`는 Completed만 `AgentFinished`까지 보류하고 Cancelled/Failed는 즉시 terminal로 닫는 실제 코드와 일치하도록 정정했다.
- `DEC-SEC-013`이 heartbeat lease, stale takeover, quarantine allowlist와 bootstrap 순서를 소유한다.

## 4. 요청 회귀 결과

| 회귀 | 결과 |
|---|---|
| 동일 process 두 handle | PASS — 두 번째 open 거부, live Prepared 유지 |
| 실제 child process owner | PASS — parent open `STORAGE_RUNTIME_OWNED` |
| live Prepared | PASS — second open이 OutcomeUnknown으로 변경하지 않음 |
| busy timeout | PASS — 5초 후 fail-closed, quarantine/fresh DB 0, 원 profile 보존 |
| recovery transaction 오류 | PASS — runtime table 오류에서 quarantine 0, 원 DB 보존 |
| true stale reopen | PASS — 120초 stale owner에서만 Prepared→OutcomeUnknown |
| crash orphan Streaming | PASS — reopen 후 `INTERRUPTED_BY_RESTART` Failed |
| true corrupt DB | PASS — corruption allowlist로 quarantine 유지 |

## 5. clean commit 전체 게이트

| 게이트 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 171 passed, 0 failed, 2 ignored |
| native credential ignored smoke | PASS |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, scan 104,411ms, peak 46,157,824 bytes < 128MiB |
| release build | PASS |
| 6-target build plan dry-run | PASS |
| AppData secret pattern / forbidden secret paths | PASS — 0 / 0 |
| Git | PASS — `a03da5d` clean |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |

`cargo audit` 실패는 기존 `SEC-F007` Accepted Risk로만 추적한다. 명령 실패를 PASS로 바꾸지 않는다.

## 6. 독립 재감사 요청

감사자는 다음을 독립적으로 재판정한다.

1. live owner가 있는 handle/process B가 owner row, Prepared receipt, unfinished turn을 변경하지 않는지 확인한다.
2. owner heartbeat와 stale threshold 사이 race에서 두 owner가 동시에 성공할 수 없는지 확인한다.
3. busy/locked/read-only/permission/recovery 오류가 quarantine/move/fresh DB로 이어지는 경로가 0인지 확인한다.
4. true stale/absent owner 획득 transaction에서만 receipt/turn recovery가 실행되는지 확인한다.
5. crash orphan turn이 Completed로 승격되지 않고 Failed로 복원되는지 확인한다.
6. report21의 atomic terminal 및 batch CAS와 43개 `29/9/5` 추적 상태가 유지되는지 확인한다.

이 요청서는 감사 PASS가 아니다. `AI_AUDIT_DOC_STANDARD.md`와 `audit_roadmap.md`에 따라 새 독립 보고서가 `SEC-CRUX-F003`, `DBG-CRUX-F003`, `DOC-CRUX-F002`, `SEC-F007`을 재판정해야 한다.
