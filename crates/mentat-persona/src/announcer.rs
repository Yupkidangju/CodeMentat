use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AnnouncerLevel {
    /// 0: 내부 진단 로그 (UI 미표시)
    Trace = 0,
    /// 1: 정상 진행 세부사항 (요청 시 표시)
    Info = 1,
    /// 2: 참고할 만한 정상 변화 (조용한 사건 피드)
    Notice = 2,
    /// 3: 작업 판단에 의미 있는 변화 (세션 피드 강조, 작업 흐름 중단 없음)
    Significant = 3,
    /// 4: 문서-코드 충돌 또는 광범위 영향 (비차단 펄스/배너)
    Warning = 4,
    /// 5: 외부 데이터 전송 승인 또는 보안 경계 (명시적 확인 시트)
    CriticalConfirmation = 5,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnouncerEvent {
    pub id: Uuid,
    pub level: AnnouncerLevel,
    pub title: String,
    pub message: String,
    pub requires_acknowledgement: bool,
}

pub struct AnnouncementPolicy;

impl AnnouncementPolicy {
    pub fn should_interrupt_user(level: AnnouncerLevel) -> bool {
        // Only level 5 requires direct user confirmation modal/sheet
        level == AnnouncerLevel::CriticalConfirmation
    }

    pub fn should_highlight_in_feed(level: AnnouncerLevel) -> bool {
        level >= AnnouncerLevel::Significant
    }
}
