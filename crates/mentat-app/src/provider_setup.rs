use mentat_inference::{BackendProfile, ModelCatalog, ModelVerification};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderSetupStage {
    Draft,
    ModelsDiscovered,
    ModelVerified,
    Active,
}

pub struct ProviderSetupState {
    pub draft_profile: BackendProfile,
    pub catalog: ModelCatalog,
    verified_profile: Option<BackendProfile>,
    active_profile: Option<BackendProfile>,
}

impl ProviderSetupState {
    pub fn new(mut draft_profile: BackendProfile) -> Self {
        // 저장된 모델 ID는 현재 자격 증명으로 다시 검색하기 전까지 신뢰하지 않는다.
        draft_profile.model.clear();
        Self {
            draft_profile,
            catalog: ModelCatalog::from_untrusted(Vec::new()),
            verified_profile: None,
            active_profile: None,
        }
    }

    pub fn stage(&self) -> ProviderSetupStage {
        if self.active_profile.as_ref() == Some(&self.draft_profile)
            && self.verified_profile.as_ref() == Some(&self.draft_profile)
        {
            ProviderSetupStage::Active
        } else if self.verified_profile.as_ref() == Some(&self.draft_profile) {
            ProviderSetupStage::ModelVerified
        } else if !self.catalog.models.is_empty() {
            ProviderSetupStage::ModelsDiscovered
        } else {
            ProviderSetupStage::Draft
        }
    }

    pub fn accept_catalog(
        &mut self,
        requested_profile: &BackendProfile,
        catalog: ModelCatalog,
    ) -> Result<(), String> {
        if requested_profile != &self.draft_profile {
            return Err("설정이 변경되어 이전 모델 목록을 폐기했습니다.".to_string());
        }
        if catalog.models.is_empty() {
            return Err("활성화할 수 있는 모델이 없습니다.".to_string());
        }
        self.catalog = catalog;
        self.draft_profile.model.clear();
        self.verified_profile = None;
        Ok(())
    }

    pub fn begin_discovery(&mut self) -> BackendProfile {
        self.catalog = ModelCatalog::from_untrusted(Vec::new());
        self.draft_profile.model.clear();
        self.verified_profile = None;
        self.draft_profile.clone()
    }

    pub fn verification_request(&self) -> Result<BackendProfile, String> {
        if self.draft_profile.model.is_empty()
            || !self
                .catalog
                .models
                .iter()
                .any(|model| model.id == self.draft_profile.model)
        {
            return Err("검색된 목록에서 모델을 먼저 선택해야 합니다.".to_string());
        }
        Ok(self.draft_profile.clone())
    }

    pub fn select_model(&mut self, model_id: &str) -> Result<(), String> {
        if !self.catalog.models.iter().any(|model| model.id == model_id) {
            return Err("현재 공급자에서 검색된 모델만 선택할 수 있습니다.".to_string());
        }
        self.draft_profile.model = model_id.to_string();
        self.verified_profile = None;
        Ok(())
    }

    pub fn accept_verification(
        &mut self,
        requested_profile: &BackendProfile,
        verification: ModelVerification,
    ) -> Result<(), String> {
        if requested_profile != &self.draft_profile {
            return Err("설정이 변경되어 이전 모델 검증 결과를 폐기했습니다.".to_string());
        }
        if !self
            .catalog
            .models
            .iter()
            .any(|model| model.id == self.draft_profile.model)
        {
            return Err("검색된 목록에서 모델을 선택해야 합니다.".to_string());
        }
        if !verification.compatible {
            return Err(verification.message);
        }
        self.verified_profile = Some(requested_profile.clone());
        Ok(())
    }

    pub fn activate(&mut self) -> Result<(), String> {
        if self.verified_profile.as_ref() != Some(&self.draft_profile) {
            return Err("현재 설정과 일치하는 모델 검증이 필요합니다.".to_string());
        }
        self.active_profile = Some(self.draft_profile.clone());
        Ok(())
    }

    pub fn active_profile(&self) -> Option<&BackendProfile> {
        self.active_profile.as_ref()
    }

    pub fn reconcile_edit(&mut self, previous: &BackendProfile) {
        let connection_changed = previous.provider != self.draft_profile.provider
            || previous.base_url != self.draft_profile.base_url
            || previous.api_key != self.draft_profile.api_key
            || previous.timeout_secs != self.draft_profile.timeout_secs;

        if connection_changed {
            self.catalog = ModelCatalog::from_untrusted(Vec::new());
            self.draft_profile.model.clear();
            self.verified_profile = None;
        } else if previous.model != self.draft_profile.model {
            self.verified_profile = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mentat_inference::{
        AvailableModel, BackendProfile, ModelCatalog, ModelVerification, ProviderKind,
    };

    fn draft() -> BackendProfile {
        BackendProfile {
            provider: ProviderKind::OpenAICompatible,
            base_url: "http://127.0.0.1:8080/v1".to_string(),
            api_key: Some("session-key".to_string()),
            model: String::new(),
            ..Default::default()
        }
    }

    #[test]
    fn activation_requires_catalog_selection_and_matching_verification() {
        let mut setup = ProviderSetupState::new(draft());
        assert_eq!(setup.stage(), ProviderSetupStage::Draft);
        assert!(setup.activate().is_err());

        let requested = setup.draft_profile.clone();
        setup
            .accept_catalog(
                &requested,
                ModelCatalog::from_untrusted(vec![AvailableModel::new("dynamic", "Dynamic")]),
            )
            .expect("catalog");
        assert_eq!(setup.stage(), ProviderSetupStage::ModelsDiscovered);

        setup.select_model("dynamic").expect("selection");
        let verified = setup.draft_profile.clone();
        setup
            .accept_verification(
                &verified,
                ModelVerification {
                    compatible: true,
                    message: "ok".to_string(),
                    latency_ms: Some(1),
                },
            )
            .expect("verification");
        assert_eq!(setup.stage(), ProviderSetupStage::ModelVerified);

        setup.activate().expect("activation");
        assert_eq!(setup.stage(), ProviderSetupStage::Active);
        assert_eq!(setup.active_profile().unwrap().model, "dynamic");
    }

    #[test]
    fn editing_draft_never_mutates_active_and_invalidates_verification() {
        let mut setup = ProviderSetupState::new(draft());
        let requested = setup.draft_profile.clone();
        setup
            .accept_catalog(
                &requested,
                ModelCatalog::from_untrusted(vec![AvailableModel::new("dynamic", "Dynamic")]),
            )
            .unwrap();
        setup.select_model("dynamic").unwrap();
        let verified = setup.draft_profile.clone();
        setup
            .accept_verification(
                &verified,
                ModelVerification {
                    compatible: true,
                    message: "ok".to_string(),
                    latency_ms: None,
                },
            )
            .unwrap();
        setup.activate().unwrap();

        let active = setup.active_profile().unwrap().clone();
        let before_edit = setup.draft_profile.clone();
        setup.draft_profile.base_url = "http://127.0.0.1:9090/v1".to_string();
        setup.reconcile_edit(&before_edit);

        assert_eq!(setup.active_profile(), Some(&active));
        assert_eq!(setup.stage(), ProviderSetupStage::Draft);
        assert!(setup.catalog.models.is_empty());
        assert!(setup.draft_profile.model.is_empty());
    }

    #[test]
    fn stale_async_results_cannot_verify_a_changed_draft() {
        let mut setup = ProviderSetupState::new(draft());
        let stale = setup.draft_profile.clone();
        setup.draft_profile.base_url = "http://127.0.0.1:9090/v1".to_string();

        assert!(setup
            .accept_catalog(
                &stale,
                ModelCatalog::from_untrusted(vec![AvailableModel::new("stale", "Stale")]),
            )
            .is_err());
        assert!(setup.catalog.models.is_empty());
    }
}
