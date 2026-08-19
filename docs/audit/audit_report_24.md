# D3D 재감사 보고서 (Turn 24 / Re-audit #22)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-20
- 원 감사: `docs/audit/audit_report_23.md`
- 재감사 요청: `docs/audit/re_audit_request_24.md`
- 기준 commit: `4fcf47ea6206c349ad8c5e6f04ae6732bc29872e`
- 핵심 수정 commit: `6c6e9462baa6331ae9d2a2c59def41844b207a14`
- 감사 대상: process-lifetime ownership, 실제 crash recovery, stale 경합, heartbeat 제거
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **PASS WITH KNOWN RISKS**

## 1. 감사 요약

report23의 차단 finding은 해소됐다. `SqliteStorage::open()`은 DB metadata 조회, connection open, migration, integrity 검사, quarantine보다 먼저 sibling lock file의 OS exclusive handle을 획득한다. Windows는 `share_mode(0)`, Unix는 `flock(LOCK_EX | LOCK_NB)`를 사용하며 handle은 모든 `SqliteStorage` clone이 공유한다. heartbeat timestamp는 더 이상 ownership 권한이 아니고, 2초 SQLite heartbeat worker도 제거됐다.

현재 Windows에서 실제 child process force-kill 뒤 sleep 없는 첫 reopen이 성공했다. 첫 reopen transaction에서 Prepared receipt는 `OutcomeUnknown`, orphan Streaming turn은 `INTERRUPTED_BY_RESTART` Failed로 복구됐다. live owner가 lock을 잡은 상태에서 schema timestamp를 120초 과거로 바꿔도 contender process는 DB open 전에 거부되고 owner의 durable write만 성공했다. 같은 process 두 handle, live child process, lock Drop/reacquire, busy/recovery 오류, corruption-only quarantine도 모두 통과했다.

report21의 completed terminal + final GroundingTrace 원자 transaction과 report22의 동일-body receipt batch CAS도 회귀하지 않았다. heartbeat 제거로 직전 `PERF-CRUX-I001`의 시간당 1,800회 idle DB write 우려도 사라졌다.

미해결 Critical/Major/Minor는 없다. 기존 공급망 `SEC-F007`만 owner/expiry/review trigger가 있는 Accepted Risk로 유지하므로 최종 판정은 **PASS WITH KNOWN RISKS**다. 이 판정은 현재 구현된 29/43 CR 요구사항과 이번 remediation 범위의 판정이며, 추적표의 9 Partial·5 Not Implemented를 43/43 완료로 승격하지 않는다.

## 2. 감사 범위

### 포함

- `crates/mentat-platform/src/lib.rs`의 Windows/Unix process lock adapter
- `crates/mentat-storage/src/db.rs`의 lock-first bootstrap과 recovery transaction
- `crates/mentat-storage/src/lib.rs`의 force-kill/stale contender/busy/corruption 회귀
- `crates/mentat-storage/src/grounding_store.rs`의 live Prepared 불변과 batch CAS 회귀
- `crates/mentat-app/src/chat_app.rs`의 session-only fallback 및 durable egress 차단 연결
- `Cargo.toml`, `Cargo.lock`, CI 3-OS matrix
- `spec.md`, `README.md`, `BUILD_GUIDE.md`, `SYSTEM_ARCHITECTURE.md`, `SECURITY_PRIVACY.md`, `DESIGN_DECISIONS.md`, `IMPLEMENTATION_SUMMARY.md`, `CR-UX-001_TRACEABILITY.md`, `audit_roadmap.md`
- report21~23의 기존 finding과 Accepted Risk

### 제외 및 검증 한계

- 실제 유료 provider 송신
- Linux/macOS native runner의 이번 로컬 실행
- Windows aarch64 및 비호스트 5개 target의 실제 compile/link/run
- CR 추적표에 명시된 기존 9 Partial·5 Not Implemented 후속 구현

현재 설치된 Rust target은 `x86_64-pc-windows-msvc`뿐이고 WSL 배포판도 없어 Unix native 실행은 수행하지 못했다. 다만 Unix 구현은 `#[cfg(unix)]`로 격리된 `flock` non-blocking exclusive 경로이며, `.github/workflows/ci.yml`이 Ubuntu/Windows/macOS 각각에서 strict Clippy, workspace tests, current-platform release build를 실행한다. 해당 CI matrix 성공은 Linux/macOS 릴리스 전 필수 조건이며 실패 시 이 판정을 해당 플랫폼에 적용할 수 없다.

## 3. 실행 게이트

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| strict Clippy all targets/features | PASS |
| `cargo test --workspace --locked` | PASS — 176 passed, 0 failed, 2 ignored |
| process lock Drop/reacquire | PASS |
| same-process 두 handle / live child process | PASS — DB open 전 `STORAGE_RUNTIME_OWNED` |
| 실제 child force-kill immediate reopen | PASS — sleep 없이 첫 recovery |
| stale timestamp child contender | PASS — contender DB write 0, owner write 1 |
| Prepared/orphan recovery | PASS — OutcomeUnknown / `INTERRUPTED_BY_RESTART` |
| busy/recovery 오류 quarantine | PASS — quarantine 0, 원 DB 보존 |
| true corrupt DB | PASS — corruption allowlist quarantine |
| native credential ignored smoke | PASS — put/get/delete |
| 100k/2GiB ignored profile | PASS — 100,000 files, 2,147,483,648 bytes, scan 90,519ms, peak 46,723,072 bytes < 128MiB |
| `cargo build --release --locked -p mentat-app` | PASS |
| 6-target release build plan dry-run | PASS |
| `git diff --check` / 기준 commit 상태 | PASS / CLEAN |
| tracked secret path / secret literal scan | PASS — 0 / 0 |
| CR 추적 상태 | PASS — 29 Implemented+Verified / 9 Partial / 5 Not Implemented |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2 + unmaintained 2 |

`cargo tree --locked --target x86_64-pc-windows-msvc -i quick-xml@0.30.0`은 역의존성을 출력하지 않았다. 감사 명령의 exit 1은 숨기지 않고 `SEC-F007` Accepted Risk로만 처리한다.

## 4. Pass 1 — 구현 정합성 Findings

### [SEC-CRUX-F003 / Re-audit #2] process-lifetime single-owner hard boundary

- Pass: Implementation / Security
- Pattern: `SEC-005`, `DBG-001`
- Area: AppData SQLite ownership
- Severity: Major
- Status: **Verified**
- Evidence:
  - `db.rs:28-45`는 directory 확보 직후 `<db>.runtime.lock`을 열며 DB metadata/access는 이후에만 수행한다.
  - `mentat-platform/src/lib.rs:32-47`은 Windows `share_mode(0)` contention을 stable error로 분류한다.
  - `mentat-platform/src/lib.rs:49-75`는 Unix `flock(LOCK_EX | LOCK_NB)` handle을 RAII 수명으로 보관한다.
  - `db.rs:69-76`은 lock handle을 `Arc`로 storage clone 전체 수명 동안 유지한다.
  - stale timestamp contender와 실제 live child가 DB open/write 전에 거부된다.
- Expected / Actual: process A가 살아 있는 동안 timestamp와 무관하게 B의 DB 접근이 차단되어야 하며 실제 구현과 Windows 회귀가 이를 충족한다.
- Re-audit Method: 3-OS CI에서 동일 process contention, child contention, force-kill release를 계속 실행한다.

### [DBG-CRUX-F003 / Re-audit #2] 실제 crash 직후 orphan recovery

- Pass: Implementation / Debug
- Pattern: `DBG-001`, `TEST-001`
- Area: crash restart
- Severity: Minor
- Status: **Verified**
- Evidence:
  - `runtime_owner_force_kill_helper`가 Streaming turn, empty trace, Prepared receipt를 durable DB에 만든다.
  - parent test가 `Child::kill()`과 `wait()`로 정상 Drop 없이 process를 종료한다.
  - sleep 또는 stale timestamp 조작 없이 첫 `SqliteStorage::open()`이 성공한다.
  - 복원된 message는 `Failed { INTERRUPTED_BY_RESTART }`, receipt는 `OutcomeUnknown`이다.
- Expected / Actual: 첫 재실행에서 orphan과 uncertain egress가 정직한 terminal 상태로 복구되어야 하며 충족한다.
- Re-audit Method: force-kill fixture가 clean Drop으로 대체되지 않도록 유지한다.

### [DOC-CRUX-F002 / Re-audit #2] ownership·terminal 문서 동기화

- Pass: Implementation / Documentation
- Severity: Minor
- Status: **Verified**
- Evidence: spec, DEC-SEC-012/013, Security, Architecture, Build Guide와 README는 Completed 보류, OS lock-first bootstrap, session-only contention, crash recovery를 실제 코드와 같은 순서로 기술한다.

## 5. Pass 2 — 디버그·엔지니어링 품질 판정

- lock contention은 DB connection 생성 전에 반환되므로 live DB/WAL/SHM recovery 경로에 진입하지 않는다.
- process crash는 kernel handle 정리로 즉시 unlock되며 stale file 삭제나 wall-clock 판정이 없다.
- 정상 Drop 시 lock handle이 해제되고 즉시 재획득할 수 있다.
- busy, recovery table 오류, permission 계열 classification은 corruption quarantine과 분리된다.
- atomic terminal, batch receipt CAS, migration v1→v6, future schema, malformed decode 회귀가 유지된다.
- force-kill 테스트 성공 후 남은 `mentat_storage` helper process는 0건이었다.

### [PERF-CRUX-I001 / Re-audit #1] heartbeat idle I/O

- Pass: Debug / Performance
- Severity: Info
- Status: **Verified**
- Evidence: heartbeat worker, 2초 timer, 별도 SQLite connection/update 코드가 제거됐다. ownership은 장수 OS handle 하나로 유지된다.
- Impact: 직전 시간당 최대 1,800회 idle write attempt가 0으로 감소한다.

### 비차단 테스트 위생 관찰

force-kill parent는 ready marker가 나타난 정상 경로에서 child를 확실히 kill/wait한다. 다만 marker timeout assertion 이전에 child cleanup guard가 없어 helper가 marker 전 무한 대기하는 새로운 회귀가 생기면 child가 남을 수 있다. 현재 실행에서는 재현되지 않았고 production 코드에 영향이 없어 finding으로 승격하지 않았다. 후속 테스트 정리 시 RAII child guard를 고려할 수 있다.

## 6. Pass 3 — 보안 판정

- DB single-owner: Verified — current Windows runtime
- lock-first migration/quarantine ordering: Verified
- crash-release / immediate recovery: Verified — actual child force-kill
- stale timestamp dual writer: Verified — contender durable write 0
- exact provider-body seal / batch terminal CAS: Verified, 회귀 없음
- completed terminal + final GroundingTrace atomicity: Verified, 회귀 없음
- cloud repository egress without durable storage: 차단 유지
- secret path/literal: 0건
- dependency advisory: `SEC-F007` Accepted Risk 유지

## 7. Cross-Pass Conflicts

없음. OS lock은 crash recovery의 가용성과 single-writer 보안 경계를 동시에 충족하며 heartbeat idle I/O도 제거한다.

## 8. Accepted Risks

### `SEC-F007` — 유지

- 상태: Accepted Risk
- 내용: `quick-xml 0.30.0` High 2건(`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`)과 unmaintained `paste`, `ttf-parser`
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: eframe/accesskit/Linux release scope 또는 상위 패치 릴리스
- 현재 근거: Windows target 역의존성 비도달. Linux/macOS 릴리스 전 CI 및 reachability 재확인 필수.

## 9. Needs Spec Clarification

없음. 단일 process, live contention session-only, 자동 retry 없음, crash immediate recovery 정책이 문서에 확정됐다.

## 10. 상태 집계와 최종 판정

- Critical: 0
- Major: 0
- Minor: 0
- Info: 비차단 테스트 cleanup 관찰 1건
- Verified: `SEC-CRUX-F003`, `DBG-CRUX-F003`, `DOC-CRUX-F002`, `PERF-CRUX-I001`
- Accepted Risk: `SEC-F007` 1건
- 최종 판정: **PASS WITH KNOWN RISKS**

판정 경계:

1. 이번 remediation과 현재 구현된 29개 CR 요구사항은 통과했다.
2. 9 Partial·5 Not Implemented는 정직하게 후속 범위로 남으며 43/43 제품 완료를 주장할 수 없다.
3. Ubuntu/macOS CI matrix와 current-platform release build가 실패하면 해당 플랫폼에 대한 PASS는 철회된다.
4. `SEC-F007` 만료 또는 review trigger 발생 시 공급망 재감사가 필요하다.

## 11. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_24.md`의 재감사 결과는 PASS WITH KNOWN RISKS입니다.
필수 코드 수정 finding은 없습니다. process-lifetime OS lock, force-kill immediate recovery,
stale timestamp contender write 0, atomic terminal, receipt batch CAS 회귀를 그대로 유지하세요.
릴리스 전 Ubuntu/Windows/macOS CI matrix와 각 current-platform release build를 통과시키고,
SEC-F007의 owner/2026-11-30 expiry/review trigger를 계속 추적하세요.
9 Partial·5 Not Implemented를 43/43 완료로 표현하지 말고 기존 감사 보고서는 수정하지 마세요.
```
