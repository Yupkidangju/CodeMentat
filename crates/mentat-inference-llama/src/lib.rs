pub mod contract;

pub use contract::{ConcurrencyGate, HardwareCapabilities, IsolatedContextHandle, ModelDescriptor};

pub struct NativeLlamaAdapter;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::Ordering;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_native_llama_contract_isolated_context_and_kv_cleanup() {
        let gate = ConcurrencyGate::new(2);
        let req1 = Uuid::new_v4();
        let req2 = Uuid::new_v4();
        let req3 = Uuid::new_v4();

        let ctx1 = gate.acquire_context(req1).await.unwrap();
        assert_eq!(gate.active_count(), 1);
        assert_eq!(gate.available_permits(), 1);
        assert!(!ctx1.is_kv_cleared.load(Ordering::SeqCst));

        let ctx2 = gate.acquire_context(req2).await.unwrap();
        assert_eq!(gate.active_count(), 2);
        assert_eq!(gate.available_permits(), 0);

        // Explicit KV cleanup on completion
        ctx1.explicit_cleanup();
        assert!(ctx1.is_kv_cleared.load(Ordering::SeqCst));

        // Drop ctx1, releasing permit back
        drop(ctx1);
        assert_eq!(gate.active_count(), 1);
        assert_eq!(gate.available_permits(), 1);

        // Third context can now be acquired without deadlock!
        let ctx3 = gate.acquire_context(req3).await.unwrap();
        assert_eq!(gate.active_count(), 2);
        assert_eq!(gate.available_permits(), 0);

        drop(ctx2);
        drop(ctx3);
        assert_eq!(gate.active_count(), 0);
        assert_eq!(gate.available_permits(), 2);
    }

    #[test]
    fn test_hardware_capability_detection() {
        let caps = HardwareCapabilities::detect();
        assert!(caps.recommended_threads > 0);
    }

    #[test]
    fn test_model_descriptor_contract() {
        let desc = ModelDescriptor {
            model_path: PathBuf::from("models/qwen2.5-coder-7b-instruct.gguf"),
            architecture: "qwen2".to_string(),
            context_length: 32768,
            quantization: "Q4_K_M".to_string(),
        };
        assert_eq!(desc.architecture, "qwen2");
        assert_eq!(desc.context_length, 32768);
    }
}
