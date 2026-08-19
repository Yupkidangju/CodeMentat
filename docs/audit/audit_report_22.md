# D3D 재감사 보고서 (Turn 22 / Re-audit #20)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_21.md`
- 재감사 요청: `docs/audit/re_audit_request_22.md`
- 기준 commit: `545171da09470fbb021032d002ea248a161e4a75`
- 핵심 수정 commit: `b0ef4255a28d551f821c56c0e6abda41d87c164f`
- 감사 대상: report21 내구성 finding 수정과 해당 변경이 포함된 clean commit
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. 감사 요약

report21의 두 Major finding 자체는 닫혔다. `AgentEvent::Completed`는 `pending_completion`에만 보류되고, `AgentFinished`의 최종 `GroundingTrace`가 도착한 뒤 `finish_turn_with_grounding`이 trace/tool/source와 Advisor 또는 Audit terminal/result를 하나의 `BEGIN IMMEDIATE` transaction으로 확정한다. commit 직전 killpoint에서는 message가 Completed가 되지 않고 final source도 남지 않는다. 동일 exact provider body의 여러 receipt도 사전 전수 검증과 batch CAS를 하나의 transaction에서 수행하며, 두 번째 update 직전 오류에서 첫 update까지 rollback된다.

그러나 새 startup reconciliation은 stale ownership을 판별하지 않는다. `SqliteStorage::open()`을 호출할 때마다 모든 `Prepared` row를 조건 없이 `OutcomeUnknown`으로 바꾼다. 앱은 단일 인스턴스나 DB lifetime lock을 강제하지 않으므로, 두 번째 프로세스가 같은 AppData DB를 여는 동안 첫 번째 프로세스의 실제 provider 송신이 진행 중이면 살아 있는 receipt도 terminal로 바뀐다. 더 심각하게 이 복구 transaction의 busy/locked/read/update/commit 오류는 현재 `should_quarantine()`에서 일반 `StorageError`로 분류되어 DB/WAL/SHM 격리와 새 DB 생성 경로에 들어간다. Windows에서는 live 파일 이동 실패로 두 번째 실행이 실패할 수 있고, 파일 rename이 허용되는 플랫폼에서는 실행 중 DB와 새 경로 DB가 갈리는 split-brain 위험이 있다.

또한 crash killpoint 재개방 테스트가 unfinished message를 계속 `Streaming`으로 남기며, startup에서 이를 Interrupted/Failed로 정리하거나 재개하는 경로가 없다. 결정 문서는 Completed뿐 아니라 Cancelled/Failed까지 outcome 도착 전 보류한다고 기술하지만 실제 코드는 Cancelled/Failed를 즉시 terminal로 저장한다.

따라서 기존 Major 2건은 **Verified**로 전환하지만, 새 Major 1건과 Minor 2건 때문에 전체 판정은 **HOLD**다.

## 2. 감사 범위

### 포함

- `crates/mentat-app/src/chat_app.rs`
- `crates/mentat-app/src/tool_egress_gate.rs`
- `crates/mentat-analysis/src/agent_loop.rs`
- `crates/mentat-storage/src/conversation.rs`
- `crates/mentat-storage/src/grounding_store.rs`
- `crates/mentat-storage/src/db.rs`
- 관련 unit/killpoint/reopen 회귀
- `spec.md`, `CR-UX-001_TRACEABILITY.md`, `SYSTEM_ARCHITECTURE.md`, `SECURITY_PRIVACY.md`, `DESIGN_DECISIONS.md`, `IMPLEMENTATION_SUMMARY.md`, `ROADMAP.md`, `CHANGELOG.md`, `audit_roadmap.md`
- report21과 `re_audit_request_22.md`

### 제외

- 실제 유료 provider 계정을 사용한 live 송신
- 실제 두 GUI 프로세스를 같은 운영 AppData DB에 붙이는 파괴 가능성 있는 수동 시험
- Windows 외 타깃의 native compile/run 및 GUI 시각·키보드 전체 smoke
- CR-UX-001 추적표에서 정직하게 `Partial` 또는 `Not Implemented`로 남긴 후속 범위의 신규 구현 감사

제외 범위는 코드 경로, 결정적 fixture, 현재 문서 상태로 보완했지만 실제 다중 프로세스 ownership은 검증 증거가 없다.

## 3. 실행 게이트

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 165 passed, 0 failed, 2 ignored |
| atomic terminal killpoint | PASS — rollback 후 Completed 0, final source 0, 정상 retry 후 동시 확정 |
| receipt batch 두 번째 update killpoint | PASS — 두 receipt 모두 Prepared로 rollback |
| mixed-body batch / restart reconciliation | PASS — 부분 update 0 / 닫힌 프로세스 fixture에서 OutcomeUnknown |
| native credential ignored smoke | PASS — put/get/delete |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, 83,907ms, peak 46,182,400 bytes < 128MiB |
| `cargo build --release --locked -p mentat-app` | PASS |
| 6-target release build plan dry-run | PASS |
| `git diff --check` / 기준 상태 | PASS / CLEAN |
| tracked secret path / secret literal scan | PASS — 0 / 0 |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |

Windows target에서 `cargo tree --locked --target x86_64-pc-windows-msvc -i quick-xml@0.30.0`은 역의존성을 출력하지 않았다. Linux 및 all-target 역트리는 offline cache에 `block-padding`/`apple-native-keyring-store`가 없어 완료하지 못했다. 이 한계는 기존 `SEC-F007`의 Windows 비도달 근거를 바꾸지 않지만, 타깃별 실제 빌드 검증을 대체하지 않는다.

## 4. Pass 1 — 구현 정합성 재판정

### [DBG-CRUX-F001 / Re-audit #1] 완료 terminal과 최종 GroundingTrace 원자성

- Pass: Implementation / Debug
- Severity: Major
- Status: **Verified**
- Evidence:
  - `chat_app.rs:1595-1598`은 Completed를 `pending_completion`에만 저장한다.
  - `chat_app.rs:1717-1734`는 terminal과 final trace ID 결속을 검사한다.
  - `chat_app.rs:1765-1798`은 storage commit 성공 후에만 UI message를 Completed로 전환한다.
  - `conversation.rs:326-349`는 final trace와 terminal update를 하나의 Immediate transaction으로 쓴다.
  - `conversation.rs:529-676`에서 Advisor message/turn 및 Audit result/message/turn이 동일 transaction helper를 사용한다.
  - `terminal_and_final_grounding_killpoint_roll_back_together`가 DB 재개방 뒤 partial durable state 0건을 검증한다.
- Expected / Actual: Completed와 full trace가 함께 확정되어야 하며, 실제 구현과 killpoint가 이를 충족한다.
- Re-audit Method: 현재 회귀를 유지하고, transaction 중간/commit 직전 오류에서 Completed+empty trace가 생기지 않는지 계속 검사한다.

### [SEC-CRUX-F002 / Re-audit #1] 동일 provider body receipt terminal 원자성

- Pass: Implementation / Security
- Severity: Major
- Status: **Verified**
- Evidence:
  - `tool_egress_gate.rs:160-168`은 receipt별 반복 대신 batch API를 한 번 호출한다.
  - `grounding_store.rs:291-399`는 빈 집합·중복 ID·누락 ID·상태·exact-body digest를 확인한 뒤 같은 Immediate transaction에서 모두 갱신한다.
  - 두 번째 update killpoint, mixed-body 거부, 정상 2-receipt Sent 회귀가 모두 통과했다.
- Expected / Actual: 동일 body의 receipt가 모두 terminal이 되거나 모두 rollback되어야 하며 실제 구현이 충족한다.
- Re-audit Method: batch 크기 2 이상, expected 충돌, missing ID, duplicate, mixed digest fixture를 유지한다.

## 5. Pass 2 — 디버그·엔지니어링 품질 Findings

### [DBG-CRUX-F003] crash 후 unfinished turn이 영구 Streaming 상태로 복원된다

- Pass: Debug / Recovery
- Pattern: `DBG-001`, `TEST-001`
- Area: conversation startup recovery
- Severity: Minor
- Status: Needs Fix
- Evidence:
  - `terminal_and_final_grounding_killpoint_roll_back_together`는 commit 전 crash를 모사하고 DB를 다시 연 뒤 `messages[1].status == MessageStatus::Streaming`을 명시적으로 기대한다(`crates/mentat-storage/src/lib.rs:834-846`).
  - production startup에는 `conversation_turns.completed_at IS NULL` 및 Pending/Streaming message를 Interrupted/Failed로 전환하거나 resume token에 결속하는 reconciliation이 없다.
  - UI의 `active_turn`은 메모리 상태이므로 재실행된 앱에는 실제 실행 작업이 없다.
- Expected: crash 뒤 재실행 시 unfinished turn은 명시적인 Interrupted/Failed 상태로 닫히거나 실제 재개 가능한 작업에 결속되어야 한다.
- Actual: 화면/DB에는 Streaming이 남지만 이를 진행할 AgentLoop가 없다.
- Impact: 사용자가 종료된 요청을 계속 진행 중인 것으로 오해하고, history 상태가 실제 runtime과 일치하지 않는다. Completed+empty trace 무결성 문제는 재발하지 않는다.
- Suggested Fix: DB lifetime ownership을 확보한 startup transaction에서 미완료 turn/message를 안정 오류 코드의 Failed 또는 새 Interrupted 상태로 정리하고, trace가 있더라도 Completed로 승격하지 않는다.
- Re-audit Method: commit 직전 process-kill fixture 후 앱 재개방에서 active runtime 0, Streaming/Pending 0, Completed+empty trace 0을 함께 확인한다.
- Owner: Coder / Storage / App

## 6. Pass 3 — 보안 Findings

### [SEC-CRUX-F003] startup receipt reconciliation이 live ownership과 transient DB 오류를 구분하지 않는다

- Pass: Security / Durability
- Pattern: `SEC-005`, `DBG-001`
- Area: tool egress audit receipt / SQLite recovery
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `db.rs:384-387`은 모든 `SqliteStorage::open()`에서 verification 직후 receipt reconciliation을 실행한다.
  - `db.rs:404-417`은 owner, process/session epoch, heartbeat, age 또는 exclusive lifetime lock 없이 `WHERE status = 'Prepared'` 전체를 `OutcomeUnknown`으로 바꾼다.
  - production `main.rs`와 `SqliteStorage`에는 같은 AppData DB를 두 프로세스가 여는 것을 차단하는 lifetime single-instance lock이 없다.
  - `db.rs:31-43`은 `open_and_migrate()`의 일반 `StorageError`를 quarantine 경로로 전달한다.
  - `db.rs:421-426`의 `should_quarantine()`은 `STORAGE_SCHEMA_FUTURE`와 `STORAGE_BACKUP_FAILED` 외 모든 storage error를 손상 후보로 본다. 따라서 새 `TOOL_EGRESS_RECOVERY_BEGIN_FAILED`, `...READ_FAILED`, `...FAILED`, `...COMMIT_FAILED`도 격리 대상이다.
  - 현재 restart 회귀는 원 storage를 `drop`한 뒤 reopen하므로 stale row만 검증하며, live first handle/process가 남아 있는 경우를 검사하지 않는다(`grounding_store.rs:966-981`).
- Expected: startup recovery는 DB의 독점 runtime ownership을 확보한 뒤 이전 owner의 stale Prepared만 OutcomeUnknown으로 닫아야 한다. busy/locked/permission/transient recovery 오류는 corruption으로 간주해 live DB를 이동하거나 새 DB로 교체해서는 안 된다.
- Actual: 두 번째 실행은 첫 번째 실행의 live Prepared를 OutcomeUnknown으로 바꿀 수 있다. recovery transaction 오류가 5초 busy timeout 뒤 일반 StorageError가 되면 live DB/WAL/SHM quarantine을 시도한다.
- Impact:
  - 실제 송신 중인 첫 프로세스의 definitive Sent/Failed CAS가 충돌해 보안 감사 로그가 불필요하게 OutcomeUnknown으로 고착될 수 있다.
  - Windows에서는 live DB 이동 실패와 startup 실패가 가능하다.
  - rename이 허용되는 플랫폼에서는 첫 프로세스가 이동된 DB를 계속 사용하고 두 번째 프로세스가 새 경로 DB를 생성하는 split-brain 및 사용자 데이터 분기 위험이 있다.
- Suggested Fix:
  1. 첫 storage open 전에 프로세스 수명 전체를 덮는 single-instance/file lock을 확보하거나 receipt에 runtime owner/session epoch와 명시적 stale 판정을 도입한다.
  2. reconciliation은 exclusive ownership이 증명된 이전 owner row에만 적용한다.
  3. `should_quarantine()`을 명시적인 integrity/corruption error allowlist로 좁히고 busy/locked/permission/recovery transaction 오류는 원본 DB를 보존한 채 fail-closed한다.
  4. `initial_ui_preferences()`와 실제 앱 storage open의 ownership 순서도 하나의 bootstrap owner 아래 정리한다.
- Re-audit Method:
  - handle/process A가 같은 DB에서 Prepared receipt와 live session을 유지한 상태로 B가 open하는 fixture를 만든다.
  - B가 single-instance로 거부되거나 live row를 건드리지 않는지 확인한다.
  - A가 write transaction을 유지해 busy timeout을 발생시킨 뒤 DB/WAL/SHM 이동 0, fresh DB 생성 0, 기존 데이터 보존을 확인한다.
  - A가 실제 종료된 다음 reopen에서만 Prepared가 OutcomeUnknown으로 닫히는지 확인한다.
- Owner: Coder / Security / Storage

## 7. 문서 정합성 Finding

### [DOC-CRUX-F002] terminal buffering 결정 문서가 실제 코드보다 넓게 기술돼 있다

- Pass: Implementation / Documentation
- Pattern: `DOC-BACKFILL-001`
- Area: `DESIGN_DECISIONS.md`
- Severity: Minor
- Status: Needs Documentation Recovery
- Evidence:
  - `DESIGN_DECISIONS.md:285`는 `AgentEvent::Completed/Cancelled/Failed`를 모두 active turn에 보류한다고 기술한다.
  - 실제 `chat_app.rs:1595-1598`만 Completed를 보류한다.
  - `chat_app.rs:1599-1691`의 Cancelled/Failed는 즉시 UI terminal과 `finish_turn()`을 수행하고 active turn을 해제한다.
- Expected: 결정 문서는 근거가 필요한 Completed 경로와 근거 없이 즉시 닫을 수 있는 Cancelled/Failed 경로를 구분해야 한다.
- Actual: 문서만 읽으면 세 terminal event가 모두 `AgentFinished`를 기다리는 것으로 해석된다.
- Impact: 후속 구현자와 감사자가 취소/실패 durable ordering을 잘못 복구하거나 불필요하게 변경할 수 있다.
- Suggested Fix: 코드가 의도라면 DEC-SEC-012를 Completed 전용 보류로 정정한다. 세 event 모두 보류가 의도라면 먼저 계약을 명확히 한 뒤 코드와 회귀를 바꾼다. startup stale/live 전제도 같은 결정에 명시한다.
- Re-audit Method: 문서 상태 전이와 `apply_agent_event`/`finish_agent_outcome`의 각 terminal 경로를 1:1 대조한다.
- Owner: Architect / Coder

## 8. Cross-Pass Conflicts

### [XPF-CRUX-F003] batch atomicity는 통과했지만 startup recovery가 audit state ownership을 약화한다

- Related Findings: `SEC-CRUX-F002`, `SEC-CRUX-F003`
- Conflict: runtime의 동일-body batch CAS는 원자적이지만, 별도 open이 live Prepared를 먼저 OutcomeUnknown으로 만들 수 있어 definitive terminal 기록을 방해한다.
- Resolution: `SEC-CRUX-F002`의 transaction 구현은 유지하고, startup ownership/recovery/quarantine 경계만 별도로 보강한다.
- Gate Impact: Major 1건으로 CR 재감사 PASS 불가.

## 9. Accepted Risks

### `SEC-F007` — 유지

- 상태: Accepted Risk
- 내용: `quick-xml 0.30.0` High 2건(`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`)과 unmaintained `paste`, `ttf-parser`
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: eframe/accesskit/Linux release scope 또는 상위 패치 릴리스
- 이번 재감사: `cargo audit` 실패를 PASS로 바꾸지 않았으며, Windows target 역의존성은 계속 비도달이다.

## 10. Needs Spec Clarification

- desktop app의 동시 실행을 지원할지 단일 인스턴스로 강제할지 문서에 명시되어 있지 않다.
- 어느 선택이든 현재의 무조건 Prepared 전환과 일반 recovery error quarantine은 안전하지 않으므로 수정은 필요하다.
- crash 후 unfinished turn의 공식 사용자 상태를 `Failed`, `Interrupted`, 또는 resumable 중 무엇으로 정의할지 확정해야 한다.

## 11. 상태 집계와 재감사 조건

- Critical: 0
- Major: 1 — `SEC-CRUX-F003`
- Minor: 2 — `DBG-CRUX-F003`, `DOC-CRUX-F002`
- Verified: `DBG-CRUX-F001`, `SEC-CRUX-F002`
- Accepted Risk: `SEC-F007` 1건
- 최종 판정: **HOLD**

재감사 전 필요한 조건:

1. DB runtime ownership을 단일 인스턴스 lock 또는 owner-aware lease로 확정한다.
2. live Prepared와 stale Prepared를 구분하는 다중 handle/process 회귀를 추가한다.
3. transient busy/locked/recovery 오류에서 quarantine/move/new DB 생성이 0건임을 검증한다.
4. crash 후 orphan Pending/Streaming 상태를 정리하고 terminal 정책을 문서와 동기화한다.
5. 전체 165개 이상 test, strict Clippy, release build, dry-run, ignored native/100k 게이트를 clean commit에서 재실행한다.

## 12. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_22.md`의 최신 재감사 결과를 확인하세요.
report21의 원자 terminal 및 batch CAS 수정은 유지하고, SEC-CRUX-F003을 우선 수정하세요.
동일 AppData DB에 대한 runtime ownership을 명시적으로 강제하거나 owner-aware stale recovery를 구현하고,
busy/locked/permission/recovery transaction 오류가 DB quarantine으로 이어지지 않게 corruption 판정을 좁히세요.
그다음 crash 후 orphan Streaming 상태와 DEC-SEC-012 문서 drift를 정리하세요.
두 동시 handle/process, live Prepared, busy timeout, true stale reopen 회귀를 추가한 뒤 전체 gate를 clean commit에서 실행하세요.
기존 감사 보고서는 수정하지 마세요.
```
