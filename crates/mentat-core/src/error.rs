use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum MentatError {
    #[error("저장소 경로가 유효하지 않거나 읽을 수 없습니다: {0}")]
    InvalidRepositoryPath(String),

    #[error("외부 경로 접근 차단: {0}")]
    ExternalPathBlocked(String),

    #[error("저장소 격리 위반: {0}")]
    StorageIsolationViolation(String),

    #[error("외부 전송(Egress) 위반: {0}")]
    EgressViolation(String),

    #[error("앱 데이터 저장소 경로는 저장소 내부일 수 없습니다: {0}")]
    AppDataInsideRepository(String),

    #[error("저장소 읽기 중 I/O 오류가 발생했습니다: {0}")]
    IoError(String),

    #[error("플랫폼 오류: {0}")]
    PlatformError(String),

    #[error("인덱싱 오류: {0}")]
    IndexingError(String),

    #[error("추론 오류: {0}")]
    InferenceError(String),

    #[error("추론 백엔드 오류 [{code}]: {message}")]
    BackendError { code: String, message: String },

    #[error("작업이 취소되었습니다.")]
    Cancelled,
}
