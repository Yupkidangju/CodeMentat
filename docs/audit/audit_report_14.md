# D3D 재감사 보고서 (Turn 14 / Re-audit #12)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_13.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `52a40bb057f5d35fba93456c04faab3e0cb0d116`
- 감사 대상: clean commit 전체
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **HOLD**

## 1. Audit Scope

### 확인 범위

- 문서: `CODE_MENTAT_SPEC.md`, `spec.md`, `designs.md`, `README.md`, `BUILD_GUIDE.md`, `IMPLEMENTATION_SUMMARY.md`, `DESIGN_DECISIONS.md`, `CHANGELOG.md`, `audit_roadmap.md`, `audit_report_13.md`
- 소스: analysis/egress/AnswerBundle, app scan/provider/hotkey/query UI, repository scanner/session/watcher, inference adapters, storage
- 검증 축: Implementation Compliance, Debug/Engineering Quality, Security/Privacy, Performance
- Git 상태: `master`가 `origin/master`보다 2 commits 앞서며 working tree clean

### 제외 범위

- 실제 OpenAI/Gemini 계정과 실 API key 통합 시험: 자격 증명 미제공
- Linux/macOS 패키지 실행 및 hotkey 시험: 현재 Windows 환경
- Windows global hotkey 수동 smoke 재수행: 구현 문서의 수행 기록과 정적 wiring만 확인
- GUI 육안 레이아웃 QA: native GUI 자동 캡처 fixture 부재
- `target`, `.git`, 외부 vendor tree: 생성물 또는 제3자 코드

## 2. 재감사 요약

Re-audit #11의 Critical `SEC-F001`은 canonical digest와 tamper matrix로 해소됐다. FR-013/FR-017 baseline 원문도 복구됐고 provider state machine은 `DR-FR-001`로 분리됐다. Claim evidence/confidence invariant, Incomplete snapshot/query/DB gate, repository switch cancel, 8MiB preview budget, notify watcher, 실제 global hotkey wiring, Gemini redirect 차단, roadmap HOLD/Partial 상태도 확인됐다.

최신 clean commit에서 formatter, strict Clippy, 88개 기본 tests, Windows locked release build가 통과했다. 별도 100k/실제 2GiB profile도 통과했으며 scan 84,984ms, preview 3,358,720 bytes, peak working set 46,141,440 bytes를 기록했다.

그러나 cloud `direct_answer`는 claims/citations와 무관한 모델 원문을 그대로 주요 답변으로 표시한다. Watcher는 `.gitignore`와 event kind를 적용하지 않고, 앱은 Stale snapshot에서도 신규 분석을 허용한다. 그 결과 live file body와 이전 `FileRecord.content_hash`가 한 citation packet에 섞여도 validator가 이를 검출하지 못할 수 있다. Gemini redirect 차단 client도 builder 실패 시 기본 redirect client로 되돌아가는 fail-open 경로가 남았다.

상태 집계는 **Critical 0, Major 3, Minor 2, Accepted Risk 1**이며 전체 판정은 **HOLD**다.

## 3. 재실행 검증

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 88 passed, 0 failed, 1 ignored |
| `cargo test --workspace --locked -- --list` | 89 tests 확인 |
| `cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture` | PASS — scan 84,984ms, 전체 test 135.40s |
| 100k/2GiB profile | 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, peak working set 46,141,440 bytes |
| `cargo build --release --locked -p mentat-app` | PASS — Windows x86_64 |
| `git diff --check` | PASS |
| `cargo audit --file Cargo.lock` | FAIL — High 2건, unmaintained 2건 |
| `git status --short --branch` | CLEAN — `master...origin/master [ahead 2]` |

`cargo audit` 결과:

- `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`: `quick-xml 0.30.0`, CVSS 7.5 High.
- `RUSTSEC-2024-0436`: `paste 1.0.15` unmaintained.
- `RUSTSEC-2026-0192`: `ttf-parser 0.25.1` unmaintained.
- `DEC-SEC-004`의 Windows 비도달 분석, owner `@Yupkidangju`, expiry `2026-11-30`, upstream/Linux review trigger를 근거로 `SEC-F007` Accepted Risk를 유지한다.

## 4. Finding 재판정표

| Finding | Re-audit #12 상태 | 핵심 근거 |
|---|---|---|
| IMP-F001 | **Verified** | baseline 47개 정의/수용 기준 mismatch 0, derived requirement 분리 |
| IMP-F002 | Verified | package/document version 정책 유지 |
| IMP-F003 | **Verified** | OS global registration/event/unregister wiring과 non-hide fallback |
| IMP-F004 | Needs Fix (Major, 부분 개선) | claim invariant는 보완, unverified direct answer가 주요 UI에 노출 |
| IMP-F005 | Verified | stable repo/profile/snapshot 복원 유지 |
| IMP-F006 | Needs Fix (Minor, 부분 개선) | roadmap은 HOLD로 보정됐으나 hide/show wording이 non-hide 구현과 불일치 |
| DBG-F001 | Verified | UI blocking receive 제거 유지 |
| DBG-F002 | Needs Fix (Major, 부분 개선) | full changed-path hash 추가, ignore/event scope와 stale/live lineage 미완료 |
| DBG-F003 | Needs Fix (Minor, 부분 개선) | 실제 profile 통과, peak memory acceptance threshold 미정 |
| DBG-F004 | Verified | semaphore permit 유지 |
| DBG-F005 | Verified | formatter/Clippy/build/wire tests 통과 |
| DBG-F006 | Verified | Git/lockfile/CI 유지 |
| DBG-F007 | Verified | async terminal 처리 유지 |
| DBG-F008 | Verified | watcher UI thread 비차단과 stop latency 유지 |
| SEC-F001 | **Verified** | canonical digest와 question/map/snapshot/ref/endpoint/model tamper matrix |
| SEC-F002 | Verified | question/content redaction과 single wire copy 유지 |
| SEC-F003 | Verified | AppData 격리 유지 |
| SEC-F004 | Needs Fix (Major, 부분 개선) | redirect none/test 추가, builder failure fallback은 redirect 허용 |
| SEC-F005 | Verified | timeout/cancel/SSE 유지 |
| SEC-F006 | Verified | canonical direct-open 유지 |
| SEC-F007 | Accepted Risk | owner/expiry/review trigger 유지 |
| SEC-F008 | Verified | R/O badge 유지 |
| SEC-F009 | Verified | Unicode token 처리 유지 |
| SEC-F010 | Verified | Unicode assignment 처리 유지 |
| SEC-F011 | Verified | consent generation guard 유지 |

## 5. Pass 1: Implementation Compliance Findings

### [IMP-F004] Re-audit #12 — 검증되지 않은 direct answer가 주요 답변으로 표시된다

- Pass: Implementation
- Pattern: `IMP-001`, `IMP-003`
- Area: cloud answer trust boundary
- Severity: Major
- Status: Needs Fix
- Summary: structured claims의 evidence invariant는 강화됐지만 모델이 작성한 `direct_answer`는 별도 검증 없이 답변 본문으로 표시된다.
- Evidence:
  - `answer_bundle.rs:78`은 JSON parsing 실패 시 `direct_answer`에 모델 원문 전체를 넣으면서 claim만 `Unknown/UNSTRUCTURED_RESPONSE`로 만든다.
  - structured JSON에서도 `direct_answer`와 validated claims/evidence 사이의 일치 조건이 없다.
  - `app.rs:837`, `app.rs:1242-1245`는 `rendered.direct_answer`를 claims보다 먼저 주요 답변으로 표시한다.
  - 따라서 모델이 schema를 무시해 근거 없는 결론을 쓰더라도 사용자는 원문을 정상 분석 답변처럼 먼저 본다.
- Expected: 검증된 claim/evidence에서 답변을 구성하거나, raw/unstructured direct answer를 명확한 `UNVERIFIED MODEL NARRATIVE`로 격리해 핵심 답변으로 취급하지 않아야 한다.
- Actual: claim은 Unknown으로 강등돼도 동일 모델 원문이 주요 답변에 그대로 남는다.
- Impact: evidence validation을 우회한 환각·오도 문장이 근거 기반 조언처럼 노출된다.
- Suggested Fix: validated claims에서 canonical answer를 합성하고 raw response는 접힌 진단 영역으로 이동한다. 최소한 unstructured/invalid bundle의 direct answer를 경고 문구로 교체하고 원문은 별도 필드로만 보존한다.
- Re-audit Method: schema 위반 응답과 valid JSON 안의 claims-불일치 direct answer가 주요 UI에 검증된 답변으로 표시되지 않는지 app-level test로 확인한다.
- Owner: Coder

### [IMP-F006] Re-audit #12 — roadmap hotkey 표현의 국소 drift

- Pass: Implementation
- Pattern: `IMP-004`
- Area: audit roadmap wording
- Severity: Minor
- Status: Needs Fix
- Summary: roadmap의 gate 문구는 `hide/show` smoke를 요구하지만 실제 결정은 창을 숨기지 않는 display/focus + Tier 1 fallback이다.
- Evidence: `audit_roadmap.md:41,70`은 hide/show를 사용하고 `designs.md:116`, `hotkeys.rs`, `app.rs:916-929`는 non-hide 정책을 사용한다.
- Expected: roadmap gate가 실제 display/focus/non-hide lifecycle을 표현한다.
- Actual: 과거 hide/show 용어가 남아 있다.
- Impact: 수동 QA 절차가 잘못 해석될 수 있다.
- Suggested Fix: `hide/show`를 `display/focus/non-hide fallback`으로 정정한다.
- Re-audit Method: roadmap, designs, implementation summary의 hotkey 용어 대조.
- Owner: Coder / Auditor

## 6. Pass 2: Debug / Engineering Quality Findings

### [DBG-F002] Re-audit #12 — watcher 범위와 stale evidence lineage가 일치하지 않는다

- Pass: Debug
- Pattern: `DBG-002`, `TEST-001`
- Area: watcher scope / stale analysis / evidence TOCTOU
- Severity: Major
- Status: Needs Fix
- Summary: notify watcher는 changed path를 full hash하지만 scan과 같은 ignore 범위를 사용하지 않고 Stale snapshot에서도 신규 분석을 허용한다.
- Evidence:
  - `watcher.rs:78-106`은 모든 notify event path를 처리하며 `.git` component만 제외한다. `.gitignore`, global ignore, app exclusions와 `event.kind` filter는 없다.
  - 루트 `.gitignore`의 `/target`, `*.pdb`, `.env`, `*.db` 변경도 watcher notification이면 STALE을 발생시킬 수 있다.
  - `app.rs:63-64`와 회귀 test는 `SnapshotStatus::Stale`을 analysis 허용 상태로 명시한다.
  - egress assembly는 `egress.rs:886`에서 live file을 다시 읽지만 catalog에는 이전 scan의 `file.content_hash`를 사용한다.
  - live content와 old hash가 섞인 packet에서도 citation validator는 old FileRecord hash와 새 `included_file_texts`를 각각 비교하므로 상호 불일치를 검출하지 않는다.
- Expected: watcher는 scanner와 동일 ignore matcher 및 의미 있는 event kind를 사용해야 한다. 신규 분석은 Ready snapshot에만 허용하거나 immutable snapshot content를 사용해야 하며 live read hash는 FileRecord hash와 다시 일치해야 한다.
- Actual: 제외 파일 변경이 false STALE을 만들고, 실제 tracked 변경 후에는 old snapshot ID/hash와 live body가 혼합될 수 있다.
- Impact: 빈번한 false stale, 또는 변경된 코드에 이전 content hash가 붙은 citation을 유효한 근거로 표시하는 lineage 오류.
- Suggested Fix: shared ignore matcher를 watcher에 주입하고 Access/irrelevant event를 제외한다. Stale 상태 신규 query를 차단하고 egress read 직후 원문 SHA-256을 FileRecord와 비교해 불일치 시 assembly를 실패 폐쇄한다.
- Re-audit Method: ignored `target/`/`.env` 변경 no-stale, tracked change stale, stale query block, scan→assembly 사이 mutation hash mismatch rejection tests.
- Owner: Coder

### [DBG-F003] Re-audit #12 — peak memory는 측정하지만 회귀 상한을 강제하지 않는다

- Pass: Debug / Performance
- Pattern: `DBG-002`, `TEST-001`
- Area: large repository performance gate
- Severity: Minor
- Status: Needs Fix
- Summary: 실제 100k/2GiB profile과 8MiB preview budget은 확인됐지만 peak working set은 출력만 하고 acceptance threshold를 assert하지 않는다.
- Evidence:
  - 본 재실행 결과는 scan 84,984ms, peak 46,141,440 bytes로 양호하다.
  - `tests.rs:423-452`는 peak before/after를 출력하지만 peak upper bound assertion이 없다.
  - `spec.md` NFR-003은 구체 상한을 기준 장비와 함께 기록하도록 요구하지만 문서에는 관측값만 있고 허용 상한/기준 장비가 없다.
- Expected: 측정 환경과 허용 peak working set 상한이 문서화되고 test 또는 benchmark gate가 초과를 실패시킨다.
- Actual: 향후 memory regression이 발생해도 elapsed/preview 조건만 맞으면 test가 PASS할 수 있다.
- Impact: 대형 저장소 메모리 회귀를 자동으로 차단하지 못한다.
- Suggested Fix: 기준 장비/OS와 허용 peak threshold를 정하고 Windows assertion 또는 benchmark 결과 판정 스크립트에 연결한다.
- Re-audit Method: threshold 이하 PASS와 의도적 초과 fixture/계측 결과의 gate 실패 확인.
- Owner: Coder / Performance

## 7. Pass 3: Security Findings

### [SEC-F004] Re-audit #12 — redirect 차단 client 생성 실패가 fail-open이다

- Pass: Security
- Pattern: `SEC-001`, `SEC-005`
- Area: Gemini credential redirect boundary
- Severity: Major
- Status: Needs Fix
- Summary: 정상 client는 redirect를 차단하지만 builder 실패 시 기본 redirect client로 대체한다.
- Evidence:
  - `gemini_adapter.rs:58-60`은 `Policy::none()`을 설정한다.
  - 이어지는 `gemini_adapter.rs:62`의 `unwrap_or_else(|_| reqwest::Client::new())`는 동일 보안 정책이 없는 기본 client를 만든다.
  - cross-origin test는 정상 builder 경로만 검증하며 fallback 경로를 다루지 않는다.
  - fallback client가 만들어지는 환경에서는 `x-goog-api-key`를 가진 discovery/verify/health/inference 요청이 다시 자동 redirect 정책을 사용한다.
- Expected: secret-bearing client 초기화가 실패하면 adapter 초기화 또는 요청이 실패 폐쇄되어야 한다.
- Actual: 보안 정책 적용 실패가 더 약한 client로 조용히 대체된다.
- Impact: 드문 초기화 실패 환경에서 redirect key zero-leak hard boundary가 사라질 수 있다.
- Suggested Fix: client build error를 저장·반환하거나 startup을 실패시킨다. 어떤 경우에도 `Client::new()`로 보안 정책을 낮추지 않는다.
- Re-audit Method: client factory/build failure를 주입해 모든 Gemini 작업이 network request 없이 명시적 오류로 끝나는지 검증한다.
- Owner: Coder / Security

## 8. Cross-Pass Conflicts

### [XPF-F001] claim 검증 PASS와 unverified direct answer UI가 충돌한다

- Related Findings: `IMP-F004`
- Conflict: domain claim validator는 fail-closed지만 UI의 가장 눈에 띄는 본문은 validator 밖의 model text다.
- Resolution: verified claim-derived answer와 unverified raw response를 UI/모델에서 분리한다.
- Gate Impact: evidence product의 주요 표시 경계가 어긋나므로 HOLD.

### [XPF-F002] watcher full-hash 주장과 snapshot lineage가 충돌한다

- Related Findings: `DBG-F002`
- Conflict: changed path 자체는 full hash하지만 egress가 그 hash를 current snapshot/FileRecord와 대조하지 않고 Stale query도 허용한다.
- Resolution: watcher scope, query gate, egress live hash check를 하나의 snapshot lineage로 닫는다.
- Gate Impact: Major correctness finding.

## 9. Accepted Risks

### SEC-F007 — 유지

- 상태: Accepted Risk
- 범위: `quick-xml 0.30.0` High 2건의 현재 Windows runtime 비도달
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: `eframe 0.31.0`, 상위 `accesskit_unix` patch, Linux release scope 편입
- 참고: `cargo audit` 실패 자체를 PASS로 간주하지 않는다.

## 10. Needs Spec Clarification

- `DBG-F003` peak working set의 정식 acceptance threshold와 기준 장비가 아직 결정되지 않았다. 구현은 현재 46.1MB를 기록했지만 어떤 값까지 PASS인지 명세화가 필요하다.

## 11. Required Fixes Before PASS

1. `IMP-F004`: raw/unstructured `direct_answer`를 검증된 답변 UI에서 분리한다.
2. `DBG-F002`: watcher ignore/event scope, Stale query gate, egress live hash 검증을 연결한다.
3. `SEC-F004`: Gemini client build error에서 redirect-enabled fallback을 제거한다.
4. `DBG-F003`: peak memory threshold와 기준 장비를 정해 회귀 gate로 만든다.
5. `IMP-F006`: roadmap hotkey 용어를 non-hide 구현과 맞춘다.
6. clean commit에서 전체 gate와 targeted regression을 다시 실행한다.

## 12. Re-audit Checklist

- [ ] unstructured/raw direct answer가 verified answer로 표시되지 않음
- [ ] ignored path와 irrelevant event가 STALE을 만들지 않음
- [ ] Stale snapshot 신규 local/cloud query 차단
- [ ] scan 이후 file mutation이 egress hash mismatch로 차단
- [ ] Gemini secure client builder failure가 fail-closed
- [ ] peak working set threshold와 기준 장비 기록
- [ ] roadmap hotkey 용어 동기화
- [ ] fmt, strict Clippy, 88+ regression tests, 100k/2GiB profile, locked release build, cargo audit
- [ ] clean commit 및 Git status 확인

## 13. 상태 집계

- Verified: 19건
  - `IMP-F001`, `IMP-F002`, `IMP-F003`, `IMP-F005`
  - `DBG-F001`, `DBG-F004`, `DBG-F005`, `DBG-F006`, `DBG-F007`, `DBG-F008`
  - `SEC-F001`, `SEC-F002`, `SEC-F003`, `SEC-F005`, `SEC-F006`, `SEC-F008`, `SEC-F009`, `SEC-F010`, `SEC-F011`
- Needs Fix: Critical 0건
- Needs Fix: Major 3건 — `IMP-F004`, `DBG-F002`, `SEC-F004`
- Needs Fix: Minor 2건 — `IMP-F006`, `DBG-F003`
- Accepted Risk: 1건 — `SEC-F007`
- 전체 판정: **HOLD**

## 14. Final Decision

**HOLD**

직전 Critical과 baseline·claim·scan·memory·hotkey의 주요 수정은 유효했고, clean commit과 실제 2GiB 실행 증거도 확보됐다. 그러나 모델 원문이 검증된 답변 UI를 우회하고, watcher/Stale/live-read 경계가 snapshot lineage를 깨뜨릴 수 있으며, Gemini redirect hard boundary에 fail-open fallback이 남아 있어 PASS할 수 없다.

## 15. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_14.md`의 Re-audit #12 결과를 기준으로 수정하세요.
우선 IMP-F004의 unverified direct_answer UI 경계와 DBG-F002의 ignore/stale/live-hash lineage를 닫으세요.
다음으로 SEC-F004의 redirect-enabled fallback을 제거하고, DBG-F003 peak memory threshold 및 IMP-F006 roadmap 용어를 동기화하세요.
수정 후 raw response UI, ignored event, stale query, scan-to-egress mutation, client builder failure 회귀 테스트와
전체 fmt/clippy/test/100k-2GiB/release/audit 결과를 clean commit 기준으로 기록하세요. 기존 감사 보고서는 수정하지 마세요.
```
