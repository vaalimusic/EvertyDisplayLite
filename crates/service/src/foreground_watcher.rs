use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::warn;
use windows::Win32::Foundation::{HMODULE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, PostThreadMessageW, EVENT_OBJECT_FOCUS, EVENT_SYSTEM_FOREGROUND,
    EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, MSG, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_QUIT,
};

#[derive(Debug, Clone, Copy)]
pub enum ForegroundEvent {
    Activated(isize),
    Focused(isize),
    MinimizeStarted(isize),
    MinimizeEnded(isize),
}

static EVENT_SENDER: OnceLock<Mutex<Option<SyncSender<ForegroundEvent>>>> = OnceLock::new();

unsafe extern "system" fn win_event_callback(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _object_id: i32,
    _child_id: i32,
    _event_thread: u32,
    _event_time: u32,
) {
    if hwnd.is_invalid() {
        return;
    }
    let event = match event {
        EVENT_SYSTEM_FOREGROUND => ForegroundEvent::Activated(hwnd.0 as isize),
        EVENT_OBJECT_FOCUS => ForegroundEvent::Focused(hwnd.0 as isize),
        EVENT_SYSTEM_MINIMIZESTART => ForegroundEvent::MinimizeStarted(hwnd.0 as isize),
        EVENT_SYSTEM_MINIMIZEEND => ForegroundEvent::MinimizeEnded(hwnd.0 as isize),
        _ => return,
    };
    if let Ok(sender) = EVENT_SENDER.get_or_init(Default::default).lock() {
        if let Some(sender) = sender.as_ref() {
            // Never block a Windows accessibility callback and never allow a
            // noisy application to grow the service queue without bounds.
            let _ = sender.try_send(event);
        }
    }
}

pub struct ForegroundWatcher {
    receiver: Receiver<ForegroundEvent>,
    thread_id: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
}

impl ForegroundWatcher {
    pub fn new() -> Option<Self> {
        let (event_sender, receiver) = sync_channel(256);
        let (ready_sender, ready_receiver) = sync_channel(1);
        let thread_id = Arc::new(AtomicU32::new(0));
        let worker_thread_id = thread_id.clone();

        let thread = thread::Builder::new()
            .name("everty-foreground-events".into())
            .spawn(move || unsafe {
                worker_thread_id.store(GetCurrentThreadId(), Ordering::Release);
                if let Ok(mut sender) = EVENT_SENDER.get_or_init(Default::default).lock() {
                    *sender = Some(event_sender);
                }

                let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
                let foreground_hook = SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    HMODULE::default(),
                    Some(win_event_callback),
                    0,
                    0,
                    flags,
                );
                let minimize_hook = SetWinEventHook(
                    EVENT_SYSTEM_MINIMIZESTART,
                    EVENT_SYSTEM_MINIMIZEEND,
                    HMODULE::default(),
                    Some(win_event_callback),
                    0,
                    0,
                    flags,
                );
                let focus_hook = SetWinEventHook(
                    EVENT_OBJECT_FOCUS,
                    EVENT_OBJECT_FOCUS,
                    HMODULE::default(),
                    Some(win_event_callback),
                    0,
                    0,
                    flags,
                );
                let installed = !foreground_hook.is_invalid()
                    && !minimize_hook.is_invalid()
                    && !focus_hook.is_invalid();
                let _ = ready_sender.send(installed);

                if installed {
                    let mut message = MSG::default();
                    while GetMessageW(&mut message, HWND::default(), 0, 0).0 > 0 {}
                }

                if !foreground_hook.is_invalid() {
                    let _ = UnhookWinEvent(foreground_hook);
                }
                if !minimize_hook.is_invalid() {
                    let _ = UnhookWinEvent(minimize_hook);
                }
                if !focus_hook.is_invalid() {
                    let _ = UnhookWinEvent(focus_hook);
                }
                if let Ok(mut sender) = EVENT_SENDER.get_or_init(Default::default).lock() {
                    *sender = None;
                }
            })
            .ok()?;

        match ready_receiver.recv_timeout(Duration::from_secs(2)) {
            Ok(true) => Some(Self {
                receiver,
                thread_id,
                thread: Some(thread),
            }),
            _ => {
                warn!("Could not install Windows foreground event hooks");
                let _ = thread.join();
                None
            }
        }
    }

    pub fn poll_event(&self) -> Option<ForegroundEvent> {
        self.receiver.try_recv().ok()
    }
}

impl Drop for ForegroundWatcher {
    fn drop(&mut self) {
        let thread_id = self.thread_id.load(Ordering::Acquire);
        if thread_id != 0 {
            unsafe {
                let _ = PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0));
            }
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_event_hooks_install_and_stop_cleanly() {
        let watcher = ForegroundWatcher::new();
        assert!(watcher.is_some());
        drop(watcher);
    }
}
