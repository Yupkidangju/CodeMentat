use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusedShortcutAction {
    CollapseOnly,
}

pub fn focused_shortcut_action(_global_registered: bool) -> FocusedShortcutAction {
    FocusedShortcutAction::CollapseOnly
}

pub struct GlobalShortcutController {
    manager: Option<GlobalHotKeyManager>,
    registered: Vec<HotKey>,
    visibility_rx: Receiver<bool>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    status: String,
}

impl GlobalShortcutController {
    pub fn register(ctx: &egui::Context) -> Self {
        let requested = [
            (
                HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyM),
                "Ctrl+Alt+M",
            ),
            (HotKey::new(Some(Modifiers::ALT), Code::Space), "Alt+Space"),
        ];
        let (visibility_tx, visibility_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));

        let Ok(manager) = GlobalHotKeyManager::new() else {
            return Self {
                manager: None,
                registered: Vec::new(),
                visibility_rx,
                stop,
                worker: None,
                status: "전역 단축키 관리자를 초기화하지 못해 숨김 기능을 비활성화했습니다."
                    .to_string(),
            };
        };

        let mut registered = Vec::new();
        let mut registered_labels = Vec::new();
        for (hotkey, label) in requested {
            if manager.register(hotkey).is_ok() {
                registered.push(hotkey);
                registered_labels.push(label);
            }
        }
        if registered.is_empty() {
            return Self {
                manager: Some(manager),
                registered,
                visibility_rx,
                stop,
                worker: None,
                status: "전역 단축키가 충돌해 숨김 기능을 비활성화했습니다.".to_string(),
            };
        }

        let ids: HashSet<u32> = registered.iter().map(HotKey::id).collect();
        let worker_stop = stop.clone();
        let repaint = ctx.clone();
        let worker = std::thread::Builder::new()
            .name("mentat-global-hotkey".to_string())
            .spawn(move || {
                while !worker_stop.load(Ordering::Relaxed) {
                    match GlobalHotKeyEvent::receiver().recv_timeout(Duration::from_millis(50)) {
                        Ok(event)
                            if event.state == HotKeyState::Pressed && ids.contains(&event.id) =>
                        {
                            // Hidden winit 창은 self-show를 보장하지 않으므로 항상 표시/포커스만 요청한다.
                            repaint.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                            repaint.send_viewport_cmd(egui::ViewportCommand::Focus);
                            if visibility_tx.send(true).is_err() {
                                break;
                            }
                            repaint.request_repaint();
                        }
                        Ok(_) | Err(_) => {}
                    }
                }
            })
            .ok();

        if worker.is_none() {
            let _ = manager.unregister_all(&registered);
            registered.clear();
            registered_labels.clear();
        }

        let status = if registered.is_empty() {
            "전역 단축키 event thread를 시작하지 못해 숨김 기능을 비활성화했습니다.".to_string()
        } else {
            format!("전역 표시·포커스 활성화: {}", registered_labels.join(" / "))
        };

        Self {
            manager: Some(manager),
            registered,
            visibility_rx,
            stop,
            worker,
            status,
        }
    }

    pub fn is_registered(&self) -> bool {
        !self.registered.is_empty()
    }

    pub fn take_visibility_request(&self) -> Option<bool> {
        let mut requested = None;
        while let Ok(visible) = self.visibility_rx.try_recv() {
            requested = Some(visible);
        }
        requested
    }

    pub fn status(&self) -> &str {
        &self.status
    }
}

impl Drop for GlobalShortcutController {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(manager) = &self.manager {
            let _ = manager.unregister_all(&self.registered);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_or_colliding_registration_never_hides_the_window() {
        assert_eq!(
            focused_shortcut_action(false),
            FocusedShortcutAction::CollapseOnly
        );
        assert_eq!(
            focused_shortcut_action(true),
            FocusedShortcutAction::CollapseOnly
        );
    }
}
