# Code Mentat 감사 로드맵 (audit_roadmap.md)

- **문서 버전:** 1.1.0
- **참조 표준:** `AI_AUDIT_DOC_STANDARD.md`
- **최신 독립 감사:** `docs/audit/audit_report_14.md` — Re-audit #12, **HOLD**
- **적용 대상:** Code Mentat 10-Crate Workspace
- **갱신 기준:** Re-audit #12 remediation working tree, clean commit 재검증 전

---

## 1. 감사 프레임워크

| Pass | 핵심 질문 | 게이트 통과 기준 |
|---|---|---|
| Implementation Compliance | baseline 원문, derived 요구사항, 코드와 UI가 충돌하지 않는가? | baseline 1:1 보존, 문서-코드 drift 없음, 최신 감사 Major 0건 |
| Debug / Engineering Quality | scan, watcher, 취소, 메모리와 빌드가 재현 가능한가? | 기본 suite 전부 통과, 명시적 ignored profile 별도 통과, strict Clippy 0 warning |
| Security & Privacy | egress 승인, 비밀정보, 경로와 외부 응답 경계가 fail-closed인가? | canonical seal tamper matrix, redirect zero-leak, secret scan, read-only 검증 통과 |

독립 감사가 HOLD인 동안 개별 Phase의 구현 완료만으로 전체 PASS를 선언하지 않는다. 코더 검증은 재감사 PASS를 대신하지 않는다.

## 2. Phase 게이트 상태

| Phase | 현재 상태 | 근거와 남은 게이트 |
|---|---|---|
| Phase 1 — Workspace / Read-Only | Verified | 저장소 경계, AppData 격리, canonical direct-open 테스트 유지 |
| Phase 2 — Detection / Evidence / Watcher | Remediated, Re-audit Pending | verified answer projection, ignore-aware watcher, Stale query gate, live hash lineage 검증 필요 |
| Phase 3 — Provider / Streaming / Egress | Remediated, Re-audit Pending | canonical seal, Gemini redirect zero-leak와 secure client fail-closed. 실계정 시험은 자격 증명 부재로 미실행 |
| Phase 4 — Persona / Storage | Partial | persona 사실 보존과 SQLite 테스트는 통과하나 전체 세션 대화 복원·키체인 연동은 미완료 |
| Phase 5 — Native Local / Stabilization | HOLD | 100k/2GiB profile은 통과했으나 네이티브 llama 실행 엔진은 비활성 계약만 존재하고 clean commit 전체 gate가 남음 |

## 3. Re-audit #12 remediation gate

| Finding | 코더 상태 | 재감사 증거 |
|---|---|---|
| SEC-F001 canonical egress seal | Remediated | question/validation/snapshot/ref/profile tamper matrix |
| IMP-F001 baseline 원문 복구 | Remediated | FR-013/FR-017 baseline 원문 1:1 대조, `DR-FR-001` 분리 |
| DBG-F003 incomplete snapshot / memory | Remediated | Incomplete DB/query 차단, 8MiB preview, 128MiB Windows peak threshold, 100k/2GiB profile |
| DBG-F002 watcher / lineage | Remediated | ignore/event scope, changed-path full hash, Stale query 차단, egress live hash 일치 |
| IMP-F004 answer trust | Remediated | claim invariant와 verified claim-derived direct answer, raw narrative 격리 |
| SEC-F004 Gemini redirect | Remediated | 302 target key 수신 0회, secure client builder failure network 0회 |
| IMP-F003 global hotkey | Verified by Re-audit #12 | register/display/focus/non-hide fallback/unregister lifecycle과 collision fallback |
| IMP-F006 roadmap 과대 판정 | Remediated | 본 문서의 HOLD/Partial 및 risk record 동기화 |

모든 `Remediated` 표시는 구현자 판정이며, 독립 재감사에서 Verified로 재판정되기 전까지 release gate는 HOLD다.

## 4. Accepted Risk

### SEC-F007 — 유지

- **범위:** Windows 현재 runtime에서 비도달인 `quick-xml 0.30.0` High advisory 2건
- **Owner:** `@Yupkidangju`
- **Expiry:** 2026-11-30
- **Review Trigger:** `eframe 0.31.0`, 상위 `accesskit_unix` patch, Linux release scope 편입 중 하나가 발생할 때
- **결정:** 위 조건까지 Windows 비도달 근거로만 수용하며 `cargo audit` 실패 자체를 PASS로 표시하지 않는다.

### 대형 저장소 메모리 — Accepted Risk 아님

- 100k/2GiB sparse corpus, peak working set, scan 시간, preview bytes를 ignored profile에서 측정한다.
- 2026-08-19 Re-audit #12 remediation 실행 결과: 100,000 files, 2,147,483,648 bytes, preview 3,358,720 bytes, scan 81,582ms, peak working set 46,170,112 bytes — 128MiB threshold PASS.
- Windows 기준 장비와 허용 peak working set 상한은 `DEC-PERF-001`에 따라 128MiB이며 테스트가 초과를 실패시킨다.
- 향후 측정이 실행되지 않거나 상한을 넘으면 Phase 5는 HOLD이며 risk 수용으로 우회하지 않는다.

## 5. Clean commit 전체 게이트

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings`
3. `cargo test --workspace --locked`
4. `cargo test -p mentat-repository --locked test_dbg_f003_100k_2gib_benchmark_profile -- --ignored --nocapture`
5. `cargo build --release --locked -p mentat-app`
6. `cargo audit --file Cargo.lock`
7. Windows global hotkey display/focus/non-hide fallback lifecycle smoke
8. `git diff --check`
9. 수정 commit 생성 후 `git status --short --branch` clean 확인

실패한 gate는 숨기거나 PASS로 바꾸지 않고 명령, 결과, Accepted Risk 또는 남은 blocker와 함께 `IMPLEMENTATION_SUMMARY.md`에 기록한다.
