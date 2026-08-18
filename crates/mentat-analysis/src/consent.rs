use crate::egress::{EgressPacket, IncludedFileRef};
use std::path::PathBuf;

/// [SEC-F011] Fail-closed consent reassembly: exclusion changes bump a generation,
/// drop the approvable packet immediately, and reject stale assembler results.
#[derive(Debug, Default)]
pub struct ConsentAssemblyState {
    pub generation: u64,
    pub rebuilding: bool,
    pub pending_packet: Option<EgressPacket>,
    pub last_display_packet: Option<EgressPacket>,
    pub pending_query: Option<String>,
    pub user_excluded_files: Vec<PathBuf>,
}

impl ConsentAssemblyState {
    pub fn reset(&mut self) {
        *self = Self {
            generation: self.generation,
            ..Self::default()
        };
    }

    /// Starts a new assembly generation and invalidates any approvable packet.
    pub fn begin_assembly(&mut self, query: String) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.rebuilding = true;
        self.pending_packet = None;
        self.pending_query = Some(query);
        self.generation
    }

    /// Updates the exclusion set and starts a new generation. The previous packet
    /// cannot be approved while the matching result is outstanding.
    pub fn apply_exclusion_toggle(&mut self, path: PathBuf, exclude: bool) -> u64 {
        if exclude {
            if !self.user_excluded_files.contains(&path) {
                self.user_excluded_files.push(path);
            }
        } else {
            self.user_excluded_files.retain(|p| p != &path);
        }
        let query = self.pending_query.clone().unwrap_or_default();
        self.begin_assembly(query)
    }

    pub fn accept_packet(&mut self, generation: u64, packet: EgressPacket) -> bool {
        if generation != self.generation {
            return false;
        }
        self.last_display_packet = Some(packet.clone());
        self.pending_packet = Some(packet);
        self.rebuilding = false;
        true
    }

    pub fn can_approve(&self) -> bool {
        !self.rebuilding && self.pending_packet.is_some()
    }

    pub fn should_show_sheet(&self) -> bool {
        self.rebuilding || self.pending_packet.is_some() || self.last_display_packet.is_some()
    }

    pub fn take_approved_packet(&mut self) -> Option<(EgressPacket, String)> {
        if !self.can_approve() {
            return None;
        }
        let packet = self.pending_packet.take()?;
        let query = self.pending_query.take().unwrap_or_default();
        self.last_display_packet = None;
        self.rebuilding = false;
        Some((packet, query))
    }

    pub fn cancel(&mut self) {
        self.pending_packet = None;
        self.last_display_packet = None;
        self.pending_query = None;
        self.rebuilding = false;
    }

    pub fn preview_refs(&self) -> &[IncludedFileRef] {
        self.pending_packet
            .as_ref()
            .or(self.last_display_packet.as_ref())
            .map(|p| p.included_file_refs.as_slice())
            .unwrap_or(&[])
    }

    pub fn display_packet(&self) -> Option<&EgressPacket> {
        self.pending_packet
            .as_ref()
            .or(self.last_display_packet.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::egress::IncludedFileRef;
    use uuid::Uuid;

    fn dummy_packet(name: &str) -> EgressPacket {
        EgressPacket {
            packet_id: Uuid::new_v4(),
            packet_hash: name.to_string(),
            included_files: vec![PathBuf::from(name)],
            included_file_refs: vec![IncludedFileRef {
                relative_path: PathBuf::from(name),
                line_start: 1,
                line_end: 2,
                line_count: 2,
            }],
            excluded_sensitive_files: vec![],
            redacted_secret_occurrences: 0,
            estimated_tokens: 4,
            prompt_context: name.to_string(),
            snapshot_id: Uuid::nil(),
            redacted_user_question: String::new(),
            included_file_texts: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_sec_f011_exclusion_invalidates_old_packet_until_new_generation() {
        let mut state = ConsentAssemblyState::default();
        let g1 = state.begin_assembly("explain structure".to_string());
        assert!(!state.can_approve());
        assert!(state.pending_packet.is_none());
        assert!(state.rebuilding);

        let old = dummy_packet("secrets.rs");
        assert!(state.accept_packet(g1, old.clone()));
        assert!(state.can_approve());
        assert!(state
            .pending_packet
            .as_ref()
            .unwrap()
            .included_files
            .contains(&PathBuf::from("secrets.rs")));

        let g2 = state.apply_exclusion_toggle(PathBuf::from("secrets.rs"), true);
        assert_ne!(g1, g2);
        assert!(!state.can_approve());
        assert!(state.pending_packet.is_none());
        assert!(state.rebuilding);
        assert!(state
            .user_excluded_files
            .contains(&PathBuf::from("secrets.rs")));
        assert!(state.take_approved_packet().is_none());

        assert!(!state.accept_packet(g1, old));
        assert!(!state.can_approve());
        assert!(state.pending_packet.is_none());

        let fresh = dummy_packet("README.md");
        assert!(state.accept_packet(g2, fresh));
        assert!(state.can_approve());
        let (approved, query) = state.take_approved_packet().expect("new generation only");
        assert_eq!(query, "explain structure");
        assert!(!approved
            .included_files
            .contains(&PathBuf::from("secrets.rs")));
        assert!(approved
            .included_files
            .contains(&PathBuf::from("README.md")));
    }

    #[test]
    fn test_sec_f011_approve_blocked_while_rebuilding_without_packet() {
        let mut state = ConsentAssemblyState::default();
        let _g = state.begin_assembly("q".to_string());
        assert!(state.should_show_sheet());
        assert!(!state.can_approve());
        assert!(state.preview_refs().is_empty());
    }
}
