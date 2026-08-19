# D3D 재감사 보고서 (Turn 17 / Re-audit #15)

- 프로젝트: Code Mentat
- 감사 일시: 2026-08-19
- 원 감사: `docs/audit/audit_report_16.md`
- 감사 기준: `AI_AUDIT_DOC_STANDARD.md`, `audit_roadmap.md`
- 기준 commit: `a63989657c28a63f8907de01f249f236add224f8`
- 감사 대상: clean commit 전체
- 변경 제한: 소스 코드, 테스트, 설정, 기존 구현 문서 수정 없음
- 최종 판정: **PASS WITH KNOWN RISKS**

## 1. 재감사 요약

`IMP-F003` Tier 1 trailing 불변조건은 우측 180pt 고정 영역, 저장소 버튼 120pt cap, truncate/tooltip, 640/760px 긴 ASCII/CJK 실제 egui geometry tests로 해소됐다. `DBG-F006`도 UI/source/docs/audit report가 `a639896` commit에 포함되고 working tree가 clean하여 해소됐다.

이전 `IMP-F004`는 모든 non-Unknown claim의 evidence를 강제하고 invalid cloud conflicts를 UI에서 제거한다. `DBG-F002`는 notify `need_rescan`, Any/Other, Access, `.gitignore`, `.git/info/exclude`를 fail-closed 분류한다. 관련 회귀도 유지된다.

전체 기본 tests 102개, formatter, strict Clippy, Windows locked release build가 통과했다. 실제 100k/2GiB profile은 scan 83,408ms, preview 3,358,720 bytes, peak working set 48,795,648 bytes로 128MiB 상한을 통과했다. Baseline 47개 정의/수용 기준 mismatch는 0이다.

Critical/Major/Minor finding은 0건이다. `cargo audit`의 High 2건은 현재 Windows runtime 비도달, owner, expiry, review trigger가 기록된 `SEC-F007` Accepted Risk이므로 **PASS WITH KNOWN RISKS**로 판정한다.

## 2. 검증 결과

| 검사 | 결과 |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | PASS |
| `cargo test --workspace --locked` | PASS — 102 passed, 0 failed, 1 ignored |
| 100k/2GiB ignored gate | PASS — scan 83,408ms, peak 48,795,648 bytes < 128MiB |
| `cargo build --release --locked -p mentat-app` | PASS |
| Baseline 47개 대조 | PASS — mismatch 0 |
| `git diff --check` | PASS |
| `git status --short --branch` | CLEAN — `master...origin/master [ahead 5]` |
| `cargo audit --no-fetch --file Cargo.lock` | High 2건, unmaintained 2건 — SEC-F007 적용 |

## 3. 최종 Finding 상태

- Verified: 24건
  - `IMP-F001`~`IMP-F006`
  - `DBG-F001`~`DBG-F008`
  - `SEC-F001`~`SEC-F006`, `SEC-F008`~`SEC-F011`
- Accepted Risk: 1건 — `SEC-F007`
- Needs Fix: Critical 0, Major 0, Minor 0

## 4. SEC-F007 Accepted Risk

- 대상: `quick-xml 0.30.0`
- Advisory: `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`
- 현재 범위: Windows runtime에서 Linux `atspi/accesskit_unix` 경로 비도달
- Owner: `@Yupkidangju`
- Expiry: 2026-11-30
- Review Trigger: `eframe 0.31.0`, 상위 `accesskit_unix` patch, Linux release scope 편입
- 주의: Linux release 전 reachability와 upgrade 가능성을 다시 감사해야 한다.

## 5. Final Decision

**PASS WITH KNOWN RISKS**

요청된 재감사 finding은 모두 해소됐고 clean commit과 전체 실행 증거가 일치한다. 현재 배포 판단에 남는 항목은 공식 추적 중인 SEC-F007 공급망 위험뿐이다.

## 6. Coder Handoff

```text
`C:/LocalDev/rust/CodeMentat/docs/audit/audit_report_17.md`의 Re-audit #15는 PASS WITH KNOWN RISKS입니다.
추가 수정 필수 finding은 없습니다. SEC-F007은 2026-11-30 이전 또는 eframe/accesskit 업데이트,
Linux release scope 편입 시 다시 감사하세요. 이 보고서를 commit에 포함하고 기존 감사 보고서는 수정하지 마세요.
```
