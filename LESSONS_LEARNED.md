# Code Mentat 개발 교훈 및 회고 (LESSONS_LEARNED.md)

- **작성일:** 2026-08-18
- **마일스톤:** Phase 1 ~ Phase 5 전 단계 완수

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
