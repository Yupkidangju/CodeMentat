use mentat_core::error::MentatError;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ModelDescriptor {
    pub model_path: PathBuf,
    pub architecture: String,
    pub context_length: usize,
    pub quantization: String,
}

#[derive(Debug, Clone)]
pub struct HardwareCapabilities {
    pub has_avx2: bool,
    pub has_neon: bool,
    pub has_metal: bool,
    pub has_cuda: bool,
    pub has_vulkan: bool,
    pub recommended_threads: usize,
}

impl HardwareCapabilities {
    pub fn detect() -> Self {
        Self {
            has_avx2: cfg!(target_arch = "x86_64"),
            has_neon: cfg!(target_arch = "aarch64"),
            has_metal: cfg!(target_os = "macos"),
            has_cuda: false,
            has_vulkan: false,
            recommended_threads: num_cpus_fallback(),
        }
    }
}

fn num_cpus_fallback() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}

/// Request-isolated context ensuring KV cache is explicitly cleared and permit is returned upon drop
pub struct IsolatedContextHandle {
    pub request_id: Uuid,
    pub is_kv_cleared: Arc<AtomicBool>,
    _permit_tracker: Arc<AtomicUsize>,
    _permit: OwnedSemaphorePermit,
}

impl IsolatedContextHandle {
    pub fn new(
        request_id: Uuid,
        permit_tracker: Arc<AtomicUsize>,
        permit: OwnedSemaphorePermit,
    ) -> Self {
        permit_tracker.fetch_add(1, Ordering::SeqCst);
        Self {
            request_id,
            is_kv_cleared: Arc::new(AtomicBool::new(false)),
            _permit_tracker: permit_tracker,
            _permit: permit,
        }
    }

    pub fn explicit_cleanup(&self) {
        self.is_kv_cleared.store(true, Ordering::SeqCst);
    }
}

impl Drop for IsolatedContextHandle {
    fn drop(&mut self) {
        self.is_kv_cleared.store(true, Ordering::SeqCst);
        self._permit_tracker.fetch_sub(1, Ordering::SeqCst);
        // _permit drops automatically, releasing permit back to Semaphore!
    }
}

/// Hardware concurrency limiter for native model weights
pub struct ConcurrencyGate {
    semaphore: Arc<Semaphore>,
    active_inferences: Arc<AtomicUsize>,
}

impl ConcurrencyGate {
    pub fn new(max_concurrent: usize) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            active_inferences: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub async fn acquire_context(
        &self,
        request_id: Uuid,
    ) -> Result<IsolatedContextHandle, MentatError> {
        let permit = self.semaphore.clone().acquire_owned().await.map_err(|e| {
            MentatError::BackendError {
                code: "SEMAPHORE_CLOSED".to_string(),
                message: e.to_string(),
            }
        })?;

        Ok(IsolatedContextHandle::new(
            request_id,
            self.active_inferences.clone(),
            permit,
        ))
    }

    pub fn active_count(&self) -> usize {
        self.active_inferences.load(Ordering::SeqCst)
    }

    pub fn available_permits(&self) -> usize {
        self.semaphore.available_permits()
    }
}
