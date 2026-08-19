# D3D 재감사 보고서 (Turn 23 / Re-audit #21)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_22.md`
- 재감사 요청: `docs/audit/re_audit_request_23.md`
- 기준 commit: `74d8822080123c752a7d00370fc612c85dcbffbe`
- 핵심 수정 commit: `a03da5d6991d7dd7e186fa4e7bc51f1e9bfbb4c5`
- 감사 대상: report22 runtime ownership·quarantine·orphan turn·문서 finding 수정
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 감사 요약

report22의 수정은 정상 heartbeat가 유지되는 구간에서 유효하다. 같은 process의 두 번째 handle과 실제 child process open은 `STORAGE_RUNTIME_OWNED`로 거부되고 live `Prepared`를 변경하지 않는다. stale takeover transaction은 `Prepared → OutcomeUnknown`과 orphan Pending/Streaming → `INTERRUPTED_BY_RESTART`를 함께 처리한다. quarantine도 명시적인 SQLite corruption/integrity 오류만 허용하도록 좁혀져 busy 및 recovery transaction 오류가 원 DB를 이동하지 않는다. `DEC-SEC-012`의 Completed/Cancelled/Failed 설명도 실제 코드와 동기화됐다.

그러나 schema v6 row는 프로세스 수명 lock이 아니라 만료 가능한 advisory lease이며, lease를 잃은 기존 writer를 차단하는 fencing이 없다. heartbeat worker는 connection/update 오류를 모두 무시한다. 30초 이상 heartbeat가 갱신되지 않거나 wall clock이 앞으로 이동하면 process B가 owner row를 교체하고 recovery를 실행할 수 있지만, process A의 기존 `SqliteStorage`는 owner row를 재검증하지 않고 계속 read/write한다. worker가 `UPDATE ... WHERE owner_id`에서 0 rows를 받아 종료해도 앱과 storage에 lease-loss를 알리지 않는다. 따라서 문서가 주장하는 단일 process 경계와 달리 stale threshold 뒤 두 writer와 recovery가 공존할 수 있다.

실제 crash 직후 복구도 요청서 주장과 다르다. 감사자가 test helper가 lease를 얻은 직후 해당 child만 강제 종료하고 같은 DB로 즉시 두 번째 helper를 실행하자 exit 101과 `STORAGE_RUNTIME_OWNED`가 재현됐다. 현재 orphan 회귀는 `drop(storage)`로 guard를 정상 해제한 다음 reopen하므로 process crash를 모사하지 않는다. 실제 앱은 첫 open 실패를 재시도하지 않고 storage 없는 session으로 시작하므로 30초가 지나도 실행 중인 앱에서는 recovery가 일어나지 않는다.

따라서 transient quarantine과 terminal 문서 drift는 **Verified**, runtime ownership과 real-crash recovery는 부분 개선 상태다. Major 1건과 Minor 1건이 남아 전체 판정은 **HOLD**다.

## 2. 감사 범위

### 포함

- `crates/mentat-storage/src/db.rs`
- `crates/mentat-storage/src/grounding_store.rs`
- `crates/mentat-storage/src/lib.rs`
- `crates/mentat-app/src/chat_app.rs`의 storage bootstrap/fallback
- schema v6 migration, runtime owner acquisition, heartbeat worker, guard Drop
- 같은 handle/process, true stale fixture, busy timeout, recovery 오류, corruption fixture
- 실제 test helper 강제 종료 후 immediate reopen 실험
- `spec.md`, `BUILD_GUIDE.md`, `README.md`, `SYSTEM_ARCHITECTURE.md`, `SECURITY_PRIVACY.md`, `DESIGN_DECISIONS.md`, `IMPLEMENTATION_SUMMARY.md`, `CR-UX-001_TRACEABILITY.md`, `audit_roadmap.md`
- report21/22 finding의 회귀 유지 여부

### 제외

- 실제 유료 provider 송신
- Windows 절전/최대절전 또는 시스템 시계 변경을 수행하는 호스트 수준 시험
- Windows 외 타깃의 native compile/run과 OS별 파일 잠금 동작
- CR 추적표의 기존 `Partial`/`Not Implemented` 후속 기능 구현

절전·시계 이동은 실제 호스트를 변경하지 않았지만, owner 판정이 UTC timestamp만 사용하고 기존 writer fencing이 없다는 코드 경로로 판정했다.

## 3. 실행 게이트

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 171 passed, 0 failed, 2 ignored |
| same-process live handle / child process | PASS — 30초 미만 live owner open 거부 |
| stale Prepared / orphan turn | PASS — 정상 Drop 또는 수동 120초 stale row fixture |
| busy timeout / recovery error quarantine | PASS — quarantine 0, 원 DB 보존 |
| true corrupt DB | PASS — corruption allowlist quarantine 유지 |
| 실제 child 강제 종료 후 immediate reopen | **FAIL — exit 101, `STORAGE_RUNTIME_OWNED`, recovery 미실행** |
| native credential ignored smoke | PASS — put/get/delete |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, scan 104,471ms, peak 46,129,152 bytes < 128MiB |
| `cargo build --release --locked -p mentat-app` | PASS |
| 6-target release build plan dry-run | PASS |
| `git diff --check` / 기준 commit 상태 | PASS / CLEAN |
| tracked secret path / secret literal scan | PASS — 0 / 0 |
| 추적 상태 | PASS — 29 Implemented+Verified / 9 Partial / 5 Not Implemented |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |

Windows target `cargo tree --locked --target x86_64-pc-windows-msvc -i quick-xml@0.30.0`은 역의존성을 출력하지 않았다. `cargo audit` 실패는 기존 `SEC-F007` Accepted Risk로만 유지하며 PASS로 바꾸지 않는다.

## 4. Pass 1 — 구현 정합성 재판정

### [SEC-CRUX-F003 / Re-audit #1] startup ownership과 quarantine 경계

- Pass: Implementation / Security
- Pattern: `SEC-005`, `DBG-001`
- Area: SQLite runtime ownership / startup recovery
- Severity: Major
- Status: **Needs Fix — 부분 개선**
- Verified Portion:
  - `db.rs:413-435`의 Immediate transaction은 최근 heartbeat owner가 있으면 recovery 전 open을 거부한다.
  - `second_live_handle_is_rejected_without_touching_live_prepared_receipts`가 동일 process live Prepared 보존을 검증한다.
  - `second_process_is_rejected_while_runtime_owner_is_live`가 실제 child process의 live owner를 검증한다.
  - `db.rs:574-579`는 quarantine을 `STORAGE_CORRUPTION_DETECTED`와 `STORAGE_INTEGRITY_CORRUPT`로 제한한다.
  - busy timeout, runtime table 오류, 실제 invalid SQLite 회귀가 모두 의도대로 통과한다.
- Remaining Evidence:
  - stale 판정은 `db.rs:424-435`의 `Utc::now() - heartbeat_at >= 30초`뿐이며 stored `process_id`는 acquire, heartbeat, write에서 읽거나 검증하지 않는다.
  - stale takeover는 `db.rs:436-513`에서 owner row를 새 UUID로 교체하고 모든 Prepared와 orphan turn을 복구한다.
  - heartbeat worker는 `db.rs:524-540`에서 connection open 및 update 오류를 무시한다. owner row가 교체되어 update count가 0이면 thread만 종료한다.
  - `SqliteStorage::lock_conn()`과 각 write transaction은 current owner/fencing token을 검사하지 않는다(`db.rs:70-77`). 저장 crate에서 `runtime_owner_id()`의 소비는 새 receipt insert뿐이며 write 권한 fencing에는 사용되지 않는다.
  - stale race 회귀가 없다. 현재 child test는 5초 이내 live owner만 확인하고, stale fixture는 기존 guard를 정상 Drop한 뒤 과거 timestamp row를 수동 삽입한다.
- Expected: process A가 살아 있는 동안 B가 ownership을 취득할 수 없어야 한다. lease를 사용하는 경우 takeover 순간 이전 owner의 모든 후속 write가 fencing token으로 실패하고 앱도 lease-loss를 인지해 중단해야 한다.
- Actual: heartbeat가 30초 이상 실패하거나 wall clock이 이동하면 B는 takeover/recovery를 성공할 수 있고 A는 자신의 기존 connection으로 계속 쓸 수 있다.
- Impact:
  - B가 A의 live Prepared를 OutcomeUnknown, live Streaming을 `INTERRUPTED_BY_RESTART`로 바꾼 뒤 A가 다시 terminal/write를 시도할 수 있다.
  - 두 runtime의 conversation, prompt, receipt, preference write가 같은 DB에서 교차해 single-owner 및 egress audit 불변조건이 깨진다.
  - 문서의 “프로세스 수명 동안 단일 owner”가 hard boundary보다 강하게 표현된다.
- Suggested Fix:
  1. 권장: DB open/migration/quarantine보다 먼저 OS가 crash 시 자동 해제하는 exclusive file lock 또는 named mutex를 획득하고 process lifetime 동안 유지한다.
  2. DB lease를 유지한다면 monotonically increasing fencing token을 모든 mutating transaction에서 owner row와 함께 검증하고, heartbeat 실패/0-row update를 shared fatal lease-loss 상태로 전달해 기존 storage write를 즉시 차단한다.
  3. wall-clock 단독 판정과 silent heartbeat retry를 제거하거나, 실제 process-liveness/start identity와 명시적 takeover protocol을 결합한다.
  4. owner 확보 전 migration/quarantine도 다른 live process와 경합하지 않도록 bootstrap lock 범위를 포함한다.
- Re-audit Method:
  - A를 살아 있게 둔 채 heartbeat update를 30초 이상 실패시키거나 stale boundary를 주입하고 B open을 시도한다.
  - B가 거부되거나 B takeover 후 A의 모든 write가 stable `STORAGE_RUNTIME_LEASE_LOST`로 실패하는지 확인한다.
  - A와 B가 동시에 성공한 durable write가 0건인지 검증한다.
  - suspend/resume 또는 clock discontinuity fixture에서 owner가 중복되지 않는지 확인한다.
- Owner: Coder / Security / Storage

### [DOC-CRUX-F002 / Re-audit #1] terminal buffering 문서 정합성

- Pass: Implementation / Documentation
- Severity: Minor
- Status: **Verified**
- Evidence:
  - `DESIGN_DECISIONS.md`의 DEC-SEC-012는 Completed만 `AgentFinished`까지 보류한다고 정정했다.
  - Cancelled/Failed는 완료 근거를 주장하지 않아 즉시 terminal로 닫는다고 명시하며 `chat_app.rs:1595-1691`과 일치한다.
- Re-audit Method: terminal event별 UI/storage 상태 전이를 계속 1:1 대조한다.

## 5. Pass 2 — 디버그·엔지니어링 품질 Findings

### [DBG-CRUX-F003 / Re-audit #1] orphan turn recovery는 실제 crash 직후 실행되지 않는다

- Pass: Debug / Recovery
- Pattern: `DBG-001`, `TEST-001`
- Area: crash restart / app bootstrap
- Severity: Minor
- Status: **Needs Fix — 부분 개선**
- Verified Portion:
  - exclusive takeover를 획득하면 `db.rs:483-509`가 orphan Pending/Streaming message와 미완료 turn을 같은 transaction에서 `INTERRUPTED_BY_RESTART`로 닫는다.
  - terminal atomic killpoint 뒤 정상 owner 해제/reopen fixture에서 Failed+empty trace가 복원되고 Completed 승격이 차단된다.
- Evidence:
  - 해당 fixture는 `crates/mentat-storage/src/lib.rs:835-839`에서 killpoint 뒤 `drop(storage)`를 호출한다. 이는 guard Drop과 owner row 삭제를 실행하므로 process crash가 아니다.
  - 실제 감사 실험은 helper owner 획득 직후 child를 강제 종료했다. 즉시 같은 helper를 다시 실행한 결과 exit 101, ready marker 없음, `STORAGE_RUNTIME_OWNED`가 발생했다.
  - dead process의 row도 마지막 heartbeat가 30초 미만이면 `db.rs:424-433`에서 live로 간주된다. stored PID는 dead-process 판정에 사용되지 않는다.
  - 앱의 `initial_ui_preferences()`는 첫 open 오류를 버리고 기본값을 사용한다(`chat_app.rs:1978-1988`). `MentatChatApp::new()`의 다음 open도 실패하면 storage를 `None`으로 고정하며 재시도하지 않는다(`chat_app.rs:140-150`).
- Expected: process crash 후 첫 재실행이 DB ownership을 안전하게 회수하고 orphan turn을 Failed로 복원하거나, 명시적 bounded retry/대기 UI로 recovery를 완료해야 한다.
- Actual: 첫 재실행은 최대 30초 동안 최근 heartbeat를 dead owner와 구분하지 못하고 영속 storage 없이 시작한다. 실행 중 자동 recovery는 없다.
- Impact: 가장 흔한 즉시 재실행에서 과거 conversation과 provider 설정이 복원되지 않고 repository cloud tool path가 durable storage 부재로 차단된다. 사용자는 앱을 다시 종료하고 stale threshold 뒤 한 번 더 실행해야 한다.
- Suggested Fix: crash-release되는 OS lock을 사용하거나 dead-owner liveness를 안전하게 판정한다. lease 방식이면 앱 startup이 bounded retry 후에만 ephemeral mode를 선택하도록 하고, 실제 child process force-kill fixture로 검증한다.
- Re-audit Method: child helper가 owner와 orphan Streaming/Prepared를 만든 뒤 process를 강제 종료한다. 30초 sleep 없이 첫 reopen에서 Failed/OutcomeUnknown 복구와 기존 데이터 복원을 확인한다.
- Owner: Coder / Storage / App

### [PERF-CRUX-I001] 2초 heartbeat의 idle I/O 비용은 측정되지 않았다

- Pass: Debug / Performance
- Area: runtime heartbeat
- Severity: Info
- Status: Measurement Follow-up
- Evidence: `db.rs:524-536`은 2초마다 새 SQLite connection을 열고 autocommit UPDATE를 수행한다. 이는 idle 상태에서도 시간당 최대 1,800회, 하루 최대 43,200회의 DB write attempt다.
- Expected: 계속 heartbeat를 유지한다면 idle disk write, lock wait, 배터리 영향과 UI transaction latency를 대표 장비에서 측정하고 예산을 둔다.
- Actual: 100k/2GiB repository scan은 통과했지만 heartbeat idle I/O/lock contention benchmark는 없다.
- Impact: 현재 수치만으로 성능 회귀를 단정하지 않는다. 다만 외부 OS lock으로 전환하면 이 비용과 stale/fencing 복잡도를 함께 제거할 수 있다.
- Re-audit Method: 1시간 idle write count, DB bytes/fsync, UI write p95를 heartbeat on/off로 비교한다.
- Owner: Coder / Performance

## 6. Pass 3 — 보안 판정

- exact provider-body seal 및 batch terminal CAS: 회귀 없음
- completed terminal + final GroundingTrace atomic transaction: 회귀 없음
- live owner 30초 미만 second-open 차단: Verified
- corruption-only quarantine: Verified
- lease-expiry 이후 single-writer hard boundary: Needs Fix (`SEC-CRUX-F003`)
- secret path/literal scan: 0건
- dependency advisory: 기존 `SEC-F007` Accepted Risk 유지

## 7. Cross-Pass Conflict

### [XPF-CRUX-F004] availability용 stale takeover가 single-owner 보안 경계를 무효화한다

- Related Findings: `SEC-CRUX-F003`, `DBG-CRUX-F003`, `PERF-CRUX-I001`
- Conflict: 30초 takeover는 crash recovery를 위한 것이지만 실제 crash 직후에는 recovery를 막고, owner가 살아 있으나 heartbeat만 잃은 경우에는 기존 writer를 fence하지 않은 채 B를 허용한다.
- Resolution: crash-release되는 OS lifetime lock을 우선 사용한다. lease가 필수라면 fencing과 lease-loss propagation을 모든 write에 강제하고 앱 startup retry 정책을 문서화한다.
- Gate Impact: Major 1건으로 CR PASS 불가.

## 8. Accepted Risks

### `SEC-F007` — 유지

- 상태: Accepted Risk
- 내용: `quick-xml 0.30.0` High 2건(`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`)과 unmaintained `paste`, `ttf-parser`
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: eframe/accesskit/Linux release scope 또는 상위 패치 릴리스
- 이번 재감사: `cargo audit`는 실제로 exit 1이었으며 Windows target 역의존성은 계속 비도달이다.

## 9. Needs Spec Clarification

- 단일 process 정책 자체는 이번 문서에서 확정됐다.
- heartbeat lease 상실 시 기존 앱을 종료할지, 읽기 전용으로 내릴지, 재획득할지의 사용자-visible 정책은 정의되지 않았다.
- crash 직후 30초 동안 storage 없는 session을 허용할지 startup에서 기다릴지 문서화되지 않았다. 현재 구현은 오류를 표시하고 ephemeral session으로 계속한다.

## 10. 상태 집계와 최종 판정

- Critical: 0
- Major: 1 — `SEC-CRUX-F003`
- Minor: 1 — `DBG-CRUX-F003`
- Info: 1 — `PERF-CRUX-I001`
- Verified: `DOC-CRUX-F002`, corruption-only quarantine, report21 atomic terminal/batch CAS
- Accepted Risk: `SEC-F007` 1건
- 최종 판정: **HOLD**

## 11. 재감사 조건

1. process lifetime exclusive ownership 또는 모든 write에 대한 fencing token을 구현한다.
2. heartbeat 실패·owner 교체 시 기존 storage/app이 lease-loss를 감지하고 write를 중단한다.
3. 실제 child force-kill 뒤 sleep 없는 첫 reopen에서 orphan turn/Prepared recovery가 성공한다.
4. stale boundary에서 두 owner의 successful durable write가 0건임을 검증한다.
5. startup fallback/retry 정책과 single-owner hard boundary를 문서·코드·테스트에서 동기화한다.
6. clean commit에서 전체 171개 이상 test, strict Clippy, release build, ignored native/100k gate를 재실행한다.

## 12. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_23.md`의 최신 재감사 결과를 확인하세요.
report21/22의 atomic terminal, batch CAS, corruption-only quarantine, Interrupted 복구는 유지하세요.
SEC-CRUX-F003을 우선 수정해 process lifetime exclusive lock을 DB open/migration보다 먼저 확보하거나,
lease takeover 후 기존 owner의 모든 write를 막는 fencing token과 lease-loss 전파를 구현하세요.
현재 crash 회귀의 정상 `drop(storage)`를 실제 child force-kill fixture로 교체·보강하고,
sleep 없는 첫 reopen에서 Prepared→OutcomeUnknown 및 orphan→INTERRUPTED_BY_RESTART가 완료되는지 검증하세요.
stale threshold 경합에서 두 runtime의 successful durable write가 0건인지 추가로 잠그세요.
문서의 single-owner hard boundary와 startup fallback/retry 정책도 실제 구현에 맞게 동기화하세요.
기존 감사 보고서는 수정하지 마세요.
```
