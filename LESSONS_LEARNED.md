# Code Mentat 개발 교훈 및 회고 (LESSONS_LEARNED.md)

- **작성일:** 2026-08-18
- **마일스톤:** Phase 1 ~ Phase 5 전 단계 완수

---

## 2026-08-19 — AgentLoop production 연결

- semantic tool result만 seal해서는 provider별 JSON 직렬화 이후 bytes를 보장할 수 없다. 최종 body를 만드는 adapter가 provider 중립 gate를 호출하고, 앱이 consent/seal/storage를 조율해야 의존 방향과 송신 직전 검증을 함께 지킬 수 있다.
- Grounding receipt는 provider 송신 전에 FK 대상 trace가 durable해야 한다. 빈 trace를 먼저 만들고 같은 ID로 tool/source를 완결하는 upsert 흐름이 재시작 복원과 receipt 선영속을 양립시킨다.
- capability는 모델 이름으로 추정하지 않고 실제 read-only tool probe로 확인해야 한다. probe 실패는 전체 모델 활성화 실패가 아니라 정직한 chat-only 강등으로 처리한다.

---

## 1. 주요 성공 요인 (What Went Well)

1. **엄격한 포트/어댑터 계층 분리 (Hexagonal Architecture):**
   - 저장소 읽기(`mentat-repository`), 분석(`mentat-analysis`), 추론(`mentat-inference`), UI(`mentat-app`)를 10개의 독립 crate로 분리하여, 저장소에 쓰기 권한이 실수로 유입되는 것을 컴파일 타임에 원천 차단함.
2. **3단계 점진적 확장(Progressive Disclosure) 위젯 설계:**
   - 대형 IDE 패널 대신 화면 상단에 떠있는 미니멀 스마트 알약(Smart Pill)으로 시작하여, 질문 시 카드, 증거 탐색 시 인스펙터로 확장되는 플로우가 개발자 작업 화면 방해를 최소화함.
3. **멀티 프로바이더 프리셋 (Google Gemini + OpenRouter + OpenAI):**
   - 단일 공통 추론 계약 하에 Gemini REST/SSE와 OpenAI 호환 SSE를 통합하여 다양한 모델 전환 편의성을 극대화함.
4. **프롬프트 인젝션 및 프라이버시 방어 (Egress Filter):**
   - 저장소 파일 내 악성 지시문을 단순 텍스트 데이터로만 취급하고, `.env` 등 민감정보 파일을 자동 제외하는 쉴드 구축.

---

## 2. 향후 과제 (Next Milestones)

1. **Native llama.cpp FFI 링크:** `mentat-inference-llama`에 봉인된 계약 테스트를 바탕으로 실제 `libllama` FFI 연동 및 로컬 GGUF 모델 추론 활성화.
2. **언어별 Tree-Sitter AST 정밀 파서:** Phase 2의 범용 텍스트/행 기반 분석을 넘어선 심볼 단위 참조 그래프 고도화.

---

## 3. CR-UX-001 전환 교훈

1. **감사 내부 모델이 기본 사용자 경험을 점령하면 안 된다.**
   - AnswerBundle, Claim, confidence, hash는 검증과 Audit Mode에는 유용하지만 일반 대화 본문을 강제로 대체하면 사용자는 자연스러운 설명과 후속 문맥을 잃는다.
   - 이후 설계는 자유 Markdown과 `GroundingTrace`를 분리하고 evidence 구조는 요청 시 펼친다.
2. **Persona는 후처리 장식이 아니라 생성 전 prompt 계약이다.**
   - 고정 intro/outro는 실제 설명 높이·호칭·대화 태도를 바꾸지 못한다.
   - 사실과 권한은 immutable Kernel/capability로 지키고 style은 editable Persona Prompt로 전달한다.
3. **자율 조사는 권한 확대가 아니라 bounded read-only capability 추가다.**
   - blanket tool 금지는 shell/write 금지와 read-only inspection을 구분하지 못했다.
   - tool enum, canonical path, budget, consent, receipt를 한 gateway에 모아야 한다.
4. **컴팩트함은 사용자 resize를 무시하는 근거가 아니다.**
   - 강제 Tier resize는 장문 대화와 접근성을 훼손했다.
   - 좁은 기본 크기와 사용자가 선택한 크기 보존을 동시에 만족해야 한다.
