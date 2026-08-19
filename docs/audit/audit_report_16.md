# D3D 재감사 보고서 (Turn 16 / Re-audit #14)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_15.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `a9a2395a5ca63d39ff29a0f4499f6e0b9669a771`
- 감사 대상: 기준 commit과 그 위의 미커밋 UI/문서 working tree
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. Audit Scope

- 확인 문서: master spec, active spec, designs, README, build/implementation/decision/changelog/roadmap, `audit_report_15.md`
- 확인 소스: AnswerBundle/conflicts validator, notify watcher, app shutdown/theme/layout, PillBar/settings UI
- 제외: 실제 외부 API 계정, Linux/macOS runtime, native GUI 육안 재실행
- Git 상태: `master...origin/master [ahead 4]`, tracked file 11개 수정 상태

## 2. 재감사 요약

직전 `IMP-F004`는 모든 non-Unknown claim에 evidence를 요구하고 invalid cloud conflicts를 제거하도록 보완됐다. `DBG-F002`도 `need_rescan`, Any/Other, Access, `.gitignore`, `.git/info/exclude`를 분류해 fail-closed 처리한다. 관련 domain/app/watcher tests가 통과한다.

미커밋 UI 변경은 불투명 고대비 light theme, 명시적 `종료 ×`와 `Ctrl+Q`, shutdown token 정리, 확대된 viewport와 typography를 추가했고 문서도 동기화했다. Formatter, strict Clippy, 100개 기본 tests, release build가 통과했다. 실제 100k/2GiB profile도 scan 82,278ms, peak 46,215,168 bytes로 128MiB 상한을 통과했다.

그러나 Tier 1은 단일 비래핑 horizontal layout이며 저장소 이름에 최대 폭/생략 처리가 없다. 종료 버튼은 가장 마지막에 배치되어 긴 저장소 이름 또는 최소 viewport에서 화면 밖으로 밀릴 수 있다. 이는 “Tier 1 우측 끝에 항상 보이는 종료”라는 새 spec을 만족하지 않는다. 또한 UI/문서 11개 파일, 374 additions/147 deletions가 미커밋 상태라 현재 결과는 commit으로 재현할 수 없다.

상태 집계는 **Critical 0, Major 2, Minor 0, Accepted Risk 1**이며 최종 판정은 **HOLD**다.

## 3. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 100 passed, 0 failed, 1 ignored |
| `cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture` | PASS — scan 82,278ms, 전체 135.99s |
| 100k/2GiB | peak 46,215,168 bytes < 128MiB |
| Baseline 47개 자동 대조 | PASS — mismatch 0 |
| `cargo build --release --locked -p mentat-app` | PASS |
| `git diff --check` | PASS — LF→CRLF 경고만 존재 |
| `cargo audit --file Cargo.lock` | 네트워크 fetch 실패 |
| `cargo audit --no-fetch --file Cargo.lock` | FAIL — 기존 High 2건, unmaintained 2건 |
| `git status --short --branch` | DIRTY — tracked 11개 수정 |

## 4. Finding 재판정표

| Finding | Re-audit #14 상태 | 근거 |
|---|---|---|
| IMP-F001 | Verified | baseline mismatch 0 |
| IMP-F002 | Verified | version 정책 유지 |
| IMP-F003 | **Needs Fix (Major, 회귀)** | unbounded repo label 뒤에 종료 버튼 배치 |
| IMP-F004 | **Verified** | 모든 non-Unknown evidence 및 cloud conflict 검증 |
| IMP-F005 | Verified | persistence 유지 |
| IMP-F006 | Verified | roadmap 상태/용어 유지 |
| DBG-F001 | Verified | nonblocking UI 유지 |
| DBG-F002 | **Verified** | Any/Other+Rescan/Access/ignore-control tests |
| DBG-F003 | Verified | 100k/2GiB/128MiB gate |
| DBG-F004 | Verified | semaphore 유지 |
| DBG-F005 | Verified | quality/wire tests |
| DBG-F006 | **Needs Fix (Major, 회귀)** | 대규모 UI/문서 working tree 미커밋 |
| DBG-F007 | Verified | terminal state 처리 |
| DBG-F008 | Verified | watcher UI 비차단 |
| SEC-F001 | Verified | canonical egress seal |
| SEC-F002 | Verified | redaction/single copy |
| SEC-F003 | Verified | AppData isolation |
| SEC-F004 | Verified | Gemini redirect/init fail-closed |
| SEC-F005 | Verified | timeout/cancel/SSE |
| SEC-F006 | Verified | canonical path |
| SEC-F007 | Accepted Risk | owner/expiry/review 유지 |
| SEC-F008 | Verified | R/O 표시 |
| SEC-F009 | Verified | Unicode token |
| SEC-F010 | Verified | Unicode assignment |
| SEC-F011 | Verified | generation guard |

## 5. Findings

### [IMP-F003] Tier 1 종료 버튼이 항상 보인다는 보장이 없다

- Pass: Implementation
- Severity: Major
- Status: Needs Fix
- Evidence:
  - `spec.md:126`은 Tier 1 우측 끝의 `종료 ×`가 항상 보여야 한다고 규정한다.
  - `pill_bar.rs:80`은 모든 항목을 하나의 `ui.horizontal`에 순서대로 배치한다.
  - `repo_label`은 실제 저장소 display name 전체를 사용하며 최대 폭, ellipsis, wrapping 제한이 없다.
  - query 폭 계산은 이미 렌더링된 leading controls 뒤의 `available_width`만 보고, 종료 버튼은 query·chip·pin·settings 뒤 가장 마지막에 렌더링된다.
  - 긴 저장소 이름 또는 `640px` 최소 viewport에서는 trailing 종료 버튼이 clip rect 밖으로 밀릴 수 있다.
  - 현재 layout test는 숫자 `available_width`만 검사하고 long repository name과 전체 control budget을 렌더링하지 않는다.
- Expected: 저장소 이름 길이와 viewport 폭에 관계없이 query 최소 폭과 종료 버튼 hit target이 유지되어야 한다.
- Actual: unbounded leading label이 safety-critical trailing control 공간을 소비할 수 있다.
- Impact: 프레임리스 창에서 사용자가 발견 가능한 종료 경로를 잃을 수 있다.
- Suggested Fix: 종료/settings/pin을 right-to-left 고정 영역에 먼저 배치하고 repo label을 최대 폭+ellipsis/tooltip으로 제한한다. 최소 viewport와 100자 repo name layout test를 추가한다.
- Re-audit Method: 640/760px, 짧은/100자/CJK repo name에서 close rect가 viewport 안에 있고 query ≥200pt인지 검증한다.
- Owner: Coder / Designer

### [DBG-F006] 현재 감사 대상이 commit으로 재현되지 않는다

- Pass: Debug / Build
- Severity: Major
- Status: Needs Fix
- Evidence: HEAD `a9a2395` 위에 source 5개와 문서 6개가 수정되어 있으며 diff는 374 additions/147 deletions다.
- Expected: 최종 감사 대상은 테스트된 소스·문서·자산이 하나의 clean commit으로 고정돼야 한다.
- Actual: 현재 release build와 100 tests는 미커밋 working tree에만 대응한다.
- Impact: 다른 환경에서 HEAD를 checkout하면 감사 결과와 다른 UI/문서를 받는다.
- Suggested Fix: IMP-F003 수정 후 관련 source/docs를 의도적인 commit으로 고정한다.
- Re-audit Method: commit hash, clean status, 동일 전체 gate 재실행.
- Owner: Coder

## 6. Accepted Risk

- `SEC-F007`: `quick-xml 0.30.0` High 2건의 Windows 비도달 위험 수용 유지
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: eframe/accesskit update 또는 Linux release 편입

## 7. 상태 집계

- Verified: 22건
- Needs Fix: Critical 0
- Needs Fix: Major 2 — `IMP-F003`, `DBG-F006`
- Needs Fix: Minor 0
- Accepted Risk: 1 — `SEC-F007`
- 판정: **HOLD**

## 8. Final Decision

**HOLD**

직전 evidence와 watcher Major는 해소됐고 모든 실행 gate가 통과했다. 다만 새 UI의 필수 종료 control 가시성이 동적 문자열에서 보장되지 않으며 현재 결과가 미커밋 상태이므로 PASS할 수 없다.

## 9. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_16.md`의 Re-audit #14 결과를 기준으로 수정하세요.
Tier 1의 종료/settings/pin을 고정 trailing 영역에 배치하고 repo name을 bounded ellipsis로 처리하세요.
640/760px 및 긴 CJK/ASCII 저장소명 layout regression을 추가한 뒤 source/docs를 clean commit으로 고정하세요.
전체 fmt/clippy/test/100k-2GiB/release/audit 결과를 새 commit 기준으로 기록하고 기존 감사 보고서는 수정하지 마세요.
```
