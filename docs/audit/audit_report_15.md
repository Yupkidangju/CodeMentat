# D3D 재감사 보고서 (Turn 15 / Re-audit #13)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_14.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `1d5de86d6861d17b8844d864edde6b98fca84717`
- 감사 대상: clean commit 전체
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. Audit Scope

### 확인 범위

- 문서: `CODE_MENTAT_SPEC.md`, `spec.md`, `designs.md`, `README.md`, `BUILD_GUIDE.md`, `IMPLEMENTATION_SUMMARY.md`, `DESIGN_DECISIONS.md`, `CHANGELOG.md`, `audit_roadmap.md`, `audit_report_14.md`
- 소스: AnswerBundle/egress, app query gate/UI, repository ignore/watcher/scanner, Gemini adapter, performance tests
- 검증 축: Implementation Compliance, Debug/Engineering Quality, Security/Privacy, Performance
- Git 상태: `master`가 `origin/master`보다 3 commits 앞서며 working tree clean

### 제외 범위

- 실제 OpenAI/Gemini 계정과 실 API key 통합 시험: 자격 증명 미제공
- Linux/macOS 패키지 실행 및 실제 watcher overflow/rescan 재현: 현재 Windows 환경
- Windows global hotkey 수동 smoke 재수행: 직전 독립 감사와 구현 기록 참조
- native GUI 육안 QA: 자동화 fixture 부재
- `target`, `.git`, 외부 vendor tree: 생성물 또는 제3자 코드. 단, notify event 계약 확인에 설치된 `notify 8.2.0`/`notify-types 2.1.0` 해당 소스만 제한적으로 대조

## 2. 재감사 요약

Re-audit #12의 `SEC-F004` redirect fail-open은 secure client를 `Option`으로 보존하고 초기화 실패 시 모든 operation을 `GEMINI_CLIENT_INIT_FAILED`로 종료하도록 수정됐다. `DBG-F003`은 Windows 기준 장비와 128MiB peak threshold를 문서화하고 실제 100k/2GiB test에서 강제한다. `IMP-F006`의 hotkey 용어도 display/focus/non-hide로 동기화됐다. Raw model `direct_answer`는 주요 UI에서 제거됐고, Stale query 차단과 egress live hash 비교, ignore-aware watcher도 추가됐다.

최신 clean commit에서 formatter, strict Clippy, 92개 기본 tests, Windows locked release build가 통과했다. 별도 100k/실제 2GiB profile도 통과했으며 scan 82,491ms, preview 3,358,720 bytes, peak working set 46,174,208 bytes로 128MiB 상한 이내였다. Baseline 47개 자동 대조도 mismatch 0이다.

그러나 verified answer projection은 `Unknown`이 아닌 모든 claim을 합성하면서 validator가 `Inferred`와 `Proposed`의 무증거 상태를 허용한다. Cloud `ConflictItem`도 evidence ID 검증 없이 `[CONFLICT]` UI로 전달된다. 또한 watcher는 `EventKind::Any`와 `Other`를 무시한다. notify 계약상 이 이벤트들은 unsupported/imprecise change 또는 queue overflow/rescan 신호일 수 있으므로 Linux/macOS에서 실제 변경을 놓칠 수 있다.

상태 집계는 **Critical 0, Major 2, Minor 0, Accepted Risk 1**이며 전체 판정은 **HOLD**다.

## 3. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 92 passed, 0 failed, 1 ignored |
| `cargo test --workspace --locked -- --list` | 93 tests 확인 |
| `cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture` | PASS — scan 82,491ms, 전체 test 132.95s |
| 100k/2GiB profile | 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, peak 46,174,208 bytes < 128MiB |
| Baseline 자동 대조 | PASS — FR/NFR/CON 47개 mismatch 0 |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `git diff --check` | PASS |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 2건 |
| `git status --short --branch` | CLEAN — `master...origin/master [ahead 3]` |

`cargo audit` 결과:

- `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`: `quick-xml 0.30.0`, CVSS 7.5 High.
- `RUSTSEC-2024-0436`: `paste 1.0.15` unmaintained.
- `RUSTSEC-2026-0192`: `ttf-parser 0.25.1` unmaintained.
- `DEC-SEC-004`의 Windows 비도달 분석, owner `@Yupkidangju`, expiry `2026-11-30`, upstream/Linux review trigger를 근거로 `SEC-F007` Accepted Risk를 유지한다.

## 4. Finding 재판정표

| Finding | Re-audit #13 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline 47개 mismatch 0, derived requirement 분리 유지 |
| IMP-F002 | Verified | package/document version 정책 유지 |
| IMP-F003 | Verified | global display/focus/non-hide wiring 유지 |
| IMP-F004 | Needs Fix (Major, 부분 개선) | raw narrative 격리, 무증거 Inferred/Proposed와 cloud ConflictItem 검증 누락 |
| IMP-F005 | Verified | stable repo/profile/snapshot 복원 유지 |
| IMP-F006 | **Verified** | roadmap hotkey 용어와 HOLD/Partial 상태 동기화 |
| DBG-F001 | Verified | UI blocking receive 제거 유지 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | ignore/stale/live hash 보완, unknown/rescan event 누락 |
| DBG-F003 | **Verified** | 실제 100k/2GiB, 8MiB preview, 128MiB peak threshold 통과 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | Verified | formatter/Clippy/build/wire tests 통과 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | Verified | async terminal 처리 유지 |
| DBG-F008 | Verified | watcher UI thread 비차단과 stop latency 유지 |
| SEC-F001 | Verified | canonical digest/tamper matrix 유지 |
| SEC-F002 | Verified | question/content redaction과 single wire copy 유지 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | **Verified** | redirect none, cross-origin zero-leak, secure client init fail-closed |
| SEC-F005 | Verified | timeout/cancel/SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review trigger 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 처리 유지 |
| SEC-F010 | Verified | Unicode assignment 처리 유지 |
| SEC-F011 | Verified | consent generation guard 유지 |

## 5. Pass 1: Implementation Compliance Finding

### [IMP-F004] Re-audit #13 — verified answer와 Conflict UI에 무증거 모델 출력이 남는다

- Pass: Implementation
- Pattern: `IMP-001`, `IMP-003`
- Area: cloud answer/evidence trust boundary
- Severity: Major
- Status: Needs Fix
- Summary: raw direct answer는 격리됐지만 canonical answer와 conflicts UI에 validator를 우회하는 모델 출력 경로가 남아 있다.
- Evidence:
  - `answer_bundle.rs:41` system contract는 evidence가 없으면 claim을 Unknown으로 분류하라고 명시한다.
  - `answer_bundle.rs:146-150`은 evidence를 필수로 요구하는 classification을 `Observed | Conflict`로만 제한한다.
  - 따라서 `Inferred` 또는 `Proposed` claim이 `evidence_ids: []`, valid confidence를 가지면 non-Unknown으로 유지된다.
  - `compose_verified_answer()`는 모든 non-Unknown claim을 “검증된 주장 기반 답변”에 포함한다.
  - `AnswerBundle.conflicts`의 `ConflictItem.evidence_ids`는 `validate_citations()`에서 검사되지 않는다.
  - 앱 `app.rs:1270-1277`은 cloud conflicts를 `[CONFLICT]`로 그대로 표시한다.
  - 현재 tests는 Observed/Conflict empty evidence와 invalid confidence만 다루며 Inferred/Proposed empty evidence 및 ConflictItem invalid evidence를 다루지 않는다.
- Expected: verified answer에 참여하는 모든 non-Unknown claim은 유효 evidence를 가져야 한다. Cloud conflict/recommendation의 evidence references도 동일 validator를 통과하거나 검증 불가 시 제거·Unknown 처리해야 한다.
- Actual: 무증거 Inferred/Proposed 문장과 검증되지 않은 ConflictItem이 evidence product의 신뢰 UI로 들어갈 수 있다.
- Impact: 모델이 classification만 바꿔 근거 없는 문장을 “검증된 답변” 또는 `[CONFLICT]`로 표시할 수 있다.
- Suggested Fix: 모든 non-Unknown claim에 최소 1개 유효 evidence를 요구하거나, evidence 없는 Proposed를 verified projection에서 제외해 별도 recommendation 영역으로 보낸다. `bundle.conflicts`의 evidence ID를 검증하고 실패한 항목은 제거하거나 Unknown claim으로 변환한다.
- Re-audit Method: Inferred/Proposed empty evidence, missing/duplicate conflict evidence, valid conflict evidence를 adapter→normalizer→app projection 경로로 검증한다.
- Owner: Coder

## 6. Pass 2: Debug / Engineering Quality Finding

### [DBG-F002] Re-audit #13 — notify rescan/unknown event를 무시해 변경을 놓칠 수 있다

- Pass: Debug
- Pattern: `DBG-002`, `TEST-001`
- Area: cross-platform watcher completeness
- Severity: Major
- Status: Needs Fix
- Summary: Access event 제외는 타당하지만 `EventKind::Any`와 `Other`까지 모두 무시하면 event loss/rescan 신호를 버린다.
- Evidence:
  - `watcher.rs:15-19`의 `event_kind_is_relevant()`는 Create/Modify/Remove만 true다.
  - `watcher.rs:85-87`은 그 외 event를 path 처리 전에 즉시 continue한다.
  - 현재 test는 `EventKind::Any`가 false라고 고정한다.
  - 설치된 `notify-types 2.1.0 event.rs:202-211`은 `Any`를 unsupported/unknown event의 catch-all이자 imprecise mode 기본으로 정의한다.
  - 같은 문서의 `Event::need_rescan()`은 event가 누락됐을 수 있으므로 filesystem representation을 refresh해야 한다고 명시한다.
  - `notify 8.2.0` Linux inotify queue overflow와 macOS FSEvents dropped-event 경로는 `EventKind::Other + Flag::Rescan`을 방출한다.
  - 이 이벤트는 현재 모두 무시되므로 queue overflow 이후 repository 변경을 STALE로 전환하지 못한다.
  - `.git/info/exclude` 변경도 generic `.git` 경로 차단보다 먼저 처리되지 않아 scan ignore scope 변경을 알리지 못한다.
- Expected: Access-only event는 무시하되 `event.need_rescan()`, `EventKind::Any`, 안전하게 분류할 수 없는 `Other`는 fail-closed STALE/full rescan으로 처리해야 한다. Ignore-rule 파일 변경은 snapshot scope 변경으로 간주해야 한다.
- Actual: 정상적인 모호 이벤트와 event-loss 신호를 irrelevant로 분류한다.
- Impact: Linux/macOS에서 이벤트가 유실되거나 backend가 모호 이벤트를 반환하면 변경된 저장소를 Ready로 유지할 수 있다.
- Suggested Fix: event kind filter 전에 `event.need_rescan()`을 확인해 즉시 STALE/full verification을 요청한다. `Any`는 relevant로 처리하고 `Other`는 flag/경로 기준 fail-closed 처리한다. `.git/info/exclude`를 `.git` 일반 제외보다 먼저 scope-control file로 인식한다.
- Re-audit Method: `Event::new(Other).set_flag(Rescan)`, `EventKind::Any`, Access, `.git/info/exclude` 변경 fixtures와 Linux/macOS target tests.
- Owner: Coder

## 7. Cross-Pass Conflicts

### [XPF-F001] verified projection 문구와 claim/conflict validator 범위가 충돌한다

- Related Findings: `IMP-F004`
- Conflict: UI는 “검증된 주장”이라고 표시하지만 validator는 모든 표시 경로에 evidence invariant를 적용하지 않는다.
- Resolution: projection과 conflict UI의 입력 집합을 evidence validation 결과로 제한한다.
- Gate Impact: Major implementation trust conflict.

### [XPF-F002] cross-platform watcher 지원과 rescan 신호 무시가 충돌한다

- Related Findings: `DBG-F002`
- Conflict: 문서는 Windows/macOS/Linux OS watcher를 지원한다고 하나 Linux/macOS event-loss 계약을 버린다.
- Resolution: notify `need_rescan`/Any/Other semantics를 fail-closed watcher state에 연결한다.
- Gate Impact: Major cross-platform correctness conflict.

## 8. Accepted Risks

### SEC-F007 — 유지

- 상태: Accepted Risk
- 범위: `quick-xml 0.30.0` High 2건의 현재 Windows runtime 비도달
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: `eframe 0.31.0`, 상위 `accesskit_unix` patch, Linux release scope 편입
- 참고: `cargo audit` 실패 자체를 PASS로 간주하지 않는다.

## 9. Needs Spec Clarification

- 없음. system contract와 notify dependency contract가 필요한 동작을 충분히 정의한다.

## 10. Required Fixes Before PASS

1. `IMP-F004`: verified projection과 cloud conflicts의 evidence invariant를 완결한다.
2. `DBG-F002`: `need_rescan`/Any/Other와 ignore-control file watcher 경계를 fail-closed로 연결한다.
3. clean commit에서 targeted regression 및 전체 gate를 재실행한다.

## 11. Re-audit Checklist

- [ ] Inferred/Proposed empty evidence가 verified answer에 들어가지 않음
- [ ] invalid/missing conflict evidence가 `[CONFLICT]` UI에 들어가지 않음
- [ ] valid conflict evidence는 유지됨
- [ ] `EventKind::Any`가 STALE/full verification을 유발함
- [ ] `Other + Flag::Rescan`이 event loss를 fail-closed 처리함
- [ ] Access-only event는 계속 무시됨
- [ ] `.git/info/exclude` 변경이 snapshot scope 변경으로 감지됨
- [ ] fmt, strict Clippy, 92+ regression tests, 100k/2GiB profile, locked release build, cargo audit
- [ ] clean commit 및 Git status 확인

## 12. 상태 집계

- Verified: 22건
  - `IMP-F001`, `IMP-F002`, `IMP-F003`, `IMP-F005`, `IMP-F006`
  - `DBG-F001`, `DBG-F003`, `DBG-F004`, `DBG-F005`, `DBG-F006`, `DBG-F007`, `DBG-F008`
  - `SEC-F001`, `SEC-F002`, `SEC-F003`, `SEC-F004`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`, `SEC-F011`
- Needs Fix: Critical 0건
- Needs Fix: Major 2건 — `IMP-F004`, `DBG-F002`
- Needs Fix: Minor 0건
- Accepted Risk: 1건 — `SEC-F007`
- 전체 판정: **HOLD**

## 13. Final Decision

**HOLD**

직전 security redirect, memory gate, roadmap drift는 해소됐고 clean commit과 실제 2GiB 실행 증거도 통과했다. 그러나 verified answer/conflict UI에 무증거 모델 출력이 남고 cross-platform watcher가 event loss/rescan 신호를 무시하므로 evidence 및 snapshot 신뢰 경계가 아직 닫히지 않았다.

## 14. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_15.md`의 Re-audit #13 결과를 기준으로 수정하세요.
IMP-F004에서 모든 verified claim과 cloud ConflictItem의 evidence invariant를 완결하세요.
DBG-F002에서는 notify event.need_rescan(), EventKind::Any/Other, Access 제외와 .git/info/exclude 변경을 fail-closed로 처리하세요.
수정 후 Inferred/Proposed empty evidence, invalid conflict evidence, Any, Other+Rescan, Access, exclude-control fixtures와
전체 fmt/clippy/test/100k-2GiB/release/audit 결과를 clean commit 기준으로 기록하세요. 기존 감사 보고서는 수정하지 마세요.
```
