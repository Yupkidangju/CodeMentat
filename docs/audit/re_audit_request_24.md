# CR-UX-001 Process Lifetime Lock 독립 재감사 요청서 (Request 24)

- 요청일: 2026-08-19
- 감사 대상 clean commit: `6c6e9462baa6331ae9d2a2c59def41844b207a14`
- 원 감사: `docs/audit/audit_report_23.md`
- 요청 상태: `INDEPENDENT RE-AUDIT REQUESTED — PASS 미선언`
- 기존 감사 보고서 수정: 없음

## 1. SEC-CRUX-F003 hard boundary 수정

heartbeat/stale-threshold takeover를 ownership 결정에서 제거하고 DB보다 먼저 OS lock을 획득한다.

```text
create AppData directory
→ acquire <db>.runtime.lock
   Windows: OpenOptionsExt share_mode(0)
   Linux/macOS: flock(LOCK_EX | LOCK_NB)
→ DB metadata/open
→ migration / integrity / corruption-only quarantine
→ schema owner metadata 교체 + startup recovery transaction
→ SqliteStorage clone 전체 수명 동안 lock handle 공유
→ final Drop 또는 process crash/force-kill에서 kernel unlock
```

- live contention은 DB connection을 만들기 전에 `STORAGE_RUNTIME_OWNED`로 종료한다.
- lock file은 삭제하지 않아 stale-file unlink 경쟁이 없다.
- schema v6 owner UUID/PID/timestamp는 복구 metadata일 뿐 takeover 권한이 아니다.
- heartbeat worker와 2초 idle SQLite connection/update를 제거했다.
- lock 획득 뒤에만 Prepared→OutcomeUnknown과 orphan→INTERRUPTED_BY_RESTART를 수행한다.
- report21/22의 atomic terminal, batch CAS, corruption-only quarantine, Interrupted recovery는 유지한다.

## 2. 실제 force-kill 회귀

`force_killed_owner_recovers_prepared_and_orphan_on_first_reopen_without_sleep`:

1. child test process가 OS lock과 storage owner를 획득한다.
2. child가 Streaming turn, empty trace, Prepared receipt를 실제 DB에 만든다.
3. marker 기록 뒤 parent가 `Child::kill()`로 child process를 강제 종료한다.
4. sleep/stale timestamp 조작 없이 parent가 즉시 같은 DB를 연다.
5. 첫 reopen에서 receipt는 OutcomeUnknown, message는 `INTERRUPTED_BY_RESTART` Failed가 된다.

이는 정상 `drop(storage)`가 아니라 Windows `TerminateProcess`/Unix kill에 대응하는 실제 process 종료 경계를 사용한다.

## 3. stale-threshold 경합 회귀

`stale_timestamp_cannot_create_two_successful_runtime_writers`:

- process A가 OS lock을 보유한 채 DB owner timestamp를 120초 과거로 변경한다.
- 별도 child contender B가 같은 DB open과 durable profile write를 시도한다.
- B는 timestamp와 무관하게 OS lock에서 거부되고 durable write를 만들지 않는다.
- A의 durable write만 성공한다.

추가 회귀:

- 같은 process 두 handle 거부 + live Prepared 불변
- 실제 child live owner 동안 parent open 거부
- process lock Drop 후 즉시 재획득
- busy timeout/recovery transaction/permission 분류에서 quarantine 0
- true corrupt DB만 quarantine

## 4. startup fallback/retry 정책

- live lock contention: 자동 retry 없음, session-only UI와 오류 안내
- session-only 상태: conversation persistence와 cloud repository egress 비활성
- crash/force-kill: kernel lock이 즉시 해제되므로 stale 대기 없이 첫 재실행에서 recovery
- lock file I/O/permission 실패: `STORAGE_RUNTIME_LOCK_FAILED`, quarantine 금지
- DB busy/locked/permission/recovery 실패: 원 DB 보존, quarantine 금지

## 5. clean commit 전체 게이트

| 게이트 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 176 passed, 0 failed, 2 ignored |
| 실제 child force-kill immediate reopen | PASS |
| stale timestamp child contender durable write | PASS — B write 0 |
| native credential ignored smoke | PASS |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, scan 107,113ms, peak 46,141,440 bytes < 128MiB |
| release build | PASS |
| 6-target build plan dry-run | PASS |
| AppData secret pattern / forbidden secret paths | PASS — 0 / 0 |
| Git | PASS — `6c6e946` clean |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |

`cargo audit` 실패는 기존 `SEC-F007` Accepted Risk로만 추적하며 PASS로 바꾸지 않는다.

## 6. 독립 재감사 요청

감사자는 다음을 독립적으로 재판정한다.

1. OS lock 획득이 DB open/migration/quarantine보다 실제로 앞서는지 확인한다.
2. force-kill 후 kernel handle 해제와 sleep 없는 첫 recovery를 재현한다.
3. live process가 있는 동안 wall clock/owner timestamp 변화가 ownership에 영향을 주지 않는지 확인한다.
4. lock contention 경로에서 DB/WAL/SHM write·move·quarantine이 0건인지 확인한다.
5. Windows share-mode 0과 Unix flock 구현이 지원 target에서 compile/run 가능한지 확인한다.
6. session-only fallback이 durable repository egress를 열지 않는지 확인한다.
7. report21/22의 atomicity 및 43개 `29/9/5` 추적 상태가 회귀하지 않았는지 확인한다.

이 요청서는 감사 PASS가 아니다. 새 독립 보고서가 `SEC-CRUX-F003`, `DBG-CRUX-F003`, `PERF-CRUX-I001`, `SEC-F007`을 재판정해야 한다.
