//! Shared terminal-mirror test infrastructure: a recording daemon host plus
//! factories for attached chats, used by find, mission workspace and input tests.

use gpui::prelude::*;
use gpui::Context;
use runner_app::terminal_ime::TerminalInput;
use runner_app::ui::Scrollbar;
use runner_core::protocol::terminal::{
    TerminalAttachment, TerminalController, TerminalFrame, TerminalMetadata, TerminalSnapshot,
    TerminalSubscription,
};
use runner_core::protocol::{
    ClientError, DaemonClient, EventSubscription, Request, Response, Transport,
};
use runner_terminal::terminal::TerminalMirror;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use super::{TerminalFind, TerminalInteraction};
use crate::AttachedChat;

#[derive(Default)]
pub(crate) struct RecordingHost {
    pub(crate) calls: Mutex<Vec<Request>>,
    inputs: Mutex<Vec<Vec<u8>>>,
    pub(crate) sizes: Mutex<Vec<(u16, u16)>>,
    controller: Mutex<TerminalController>,
    stream: Mutex<Option<(mpsc::Sender<TerminalFrame>, mpsc::Receiver<TerminalFrame>)>>,
    processed: Mutex<Option<mpsc::Sender<()>>>,
    input_tracker: Mutex<Option<runner_terminal::input_state::InputTracker>>,
    input_observation: Mutex<Option<runner_terminal::input_state::InputObservation>>,
    composition_clear_failures: Mutex<usize>,
}
struct Frames(mpsc::Receiver<TerminalFrame>);
impl TerminalSubscription for Frames {
    fn recv(&mut self) -> Result<TerminalFrame, ClientError> {
        self.0.recv().map_err(|_| ClientError::msg("closed"))
    }
    fn cancellation(&self) -> Arc<dyn Fn() + Send + Sync> {
        Arc::new(|| {})
    }
}
impl Transport for RecordingHost {
    fn call(&self, request: Request) -> Result<Response, ClientError> {
        let response = match &request {
            Request::terminal_metadata { .. } => {
                if let Some(tx) = &*self.processed.lock().unwrap() {
                    let _ = tx.send(());
                }
                Response::terminal_metadata(Ok(TerminalMetadata::default()))
            }
            Request::terminal_observe_input { input, .. } => {
                if matches!(
                    input,
                    runner_core::protocol::terminal::InputEvent::Composing { composing: false }
                ) {
                    let mut failures = self.composition_clear_failures.lock().unwrap();
                    if *failures > 0 {
                        *failures -= 1;
                        return Err(ClientError::msg("transient composition clear failure"));
                    }
                }
                if let Some(tracker) = self.input_tracker.lock().unwrap().as_mut() {
                    if let Some(observation) = tracker.observe_input(
                        input,
                        std::time::Instant::now(),
                        &runner_terminal::replay::new_term(80, 24),
                    ) {
                        *self.input_observation.lock().unwrap() = Some(observation);
                    }
                }
                Response::terminal_observe_input(Ok(()))
            }
            Request::terminal_input_reset_guard { .. } => {
                Response::terminal_input_reset_guard(Ok(0))
            }
            Request::terminal_reset_input_state { .. } => {
                Response::terminal_reset_input_state(Ok(()))
            }
            _ => return Err(ClientError::msg("unexpected request")),
        };
        if !matches!(request, Request::terminal_metadata { .. }) {
            self.calls.lock().unwrap().push(request);
        }
        Ok(response)
    }
    fn subscribe(&self) -> Box<dyn EventSubscription> {
        unreachable!()
    }
    fn attach(&self, _: &str) -> Result<TerminalAttachment, ClientError> {
        let (_, rx) = self.stream.lock().unwrap().take().unwrap();
        Ok(TerminalAttachment {
            subscriber_id: 1,
            snapshot: TerminalSnapshot {
                seq: 0,
                cols: 80,
                rows: 24,
                controller: self.controller.lock().unwrap().clone(),
                bytes: Vec::new(),
                unfinished_len: 0,
                preceding_char: None,
            },
            frames: Box::new(Frames(rx)),
        })
    }
    fn input(&self, _: &str, bytes: &[u8]) -> Result<(), ClientError> {
        self.inputs.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }
    fn resize(&self, _: &str, _: u64, cols: u16, rows: u16) {
        self.sizes.lock().unwrap().push((cols, rows));
    }
}

pub(crate) fn mirror_named(
    id: &str,
    controller: TerminalController,
) -> (
    Arc<RecordingHost>,
    Arc<TerminalMirror>,
    mpsc::Sender<TerminalFrame>,
    mpsc::Receiver<()>,
) {
    mirror_waking(id, controller, Arc::new(|| {}))
}

pub(crate) fn mirror_waking(
    id: &str,
    controller: TerminalController,
    waker: Arc<dyn Fn() + Send + Sync>,
) -> (
    Arc<RecordingHost>,
    Arc<TerminalMirror>,
    mpsc::Sender<TerminalFrame>,
    mpsc::Receiver<()>,
) {
    let (tx, rx) = mpsc::channel();
    let host = Arc::new(RecordingHost {
        controller: Mutex::new(controller),
        stream: Mutex::new(Some((tx.clone(), rx))),
        ..Default::default()
    });
    let mirror = TerminalMirror::attach(DaemonClient::new(host.clone()), id.into(), waker).unwrap();
    let (ack, received) = mpsc::channel();
    *host.processed.lock().unwrap() = Some(ack);
    (host, mirror, tx, received)
}
fn mirror(
    controller: TerminalController,
) -> (
    Arc<RecordingHost>,
    Arc<TerminalMirror>,
    mpsc::Sender<TerminalFrame>,
    mpsc::Receiver<()>,
) {
    mirror_named("test", controller)
}
pub(crate) fn change(
    tx: &mpsc::Sender<TerminalFrame>,
    ack: &mpsc::Receiver<()>,
    seq: u64,
    controller: TerminalController,
) {
    tx.send(TerminalFrame::Resized {
        seq,
        cols: 91,
        rows: 31,
        controller,
    })
    .unwrap();
    ack.recv_timeout(Duration::from_secs(5)).unwrap();
}

pub(crate) fn attached<T: 'static>(
    terminal: Arc<TerminalMirror>,
    cx: &mut Context<T>,
) -> AttachedChat {
    let interaction = cx.new(|_| TerminalInteraction::new(terminal.clone()));
    let input = cx.new(|_| TerminalInput::new(terminal.clone()));
    let focus = cx.focus_handle();
    let find = cx.new(|cx| TerminalFind::new(terminal.clone(), focus.clone(), cx));
    AttachedChat {
        _terminal_view: terminal.view(),
        terminal_scrollbar: cx.new(|_| Scrollbar::terminal(terminal.clone())),
        terminal,
        terminal_interaction: interaction,
        _terminal_input_subscription: cx.observe(&input, |_, _, _| {}),
        _terminal_focus_subscription: Some(cx.observe(&input, |_, _, _| {})),
        terminal_input: input,
        terminal_focus: focus,
        terminal_find: find,
    }
}

#[test]
fn failed_composition_clear_retries_without_output_and_cannot_clear_a_new_preedit() {
    use runner_terminal::input_state::{InputState, InputTracker};
    for restart in [false, true] {
        let (host, terminal, _tx, _ack) = mirror(TerminalController::Desktop);
        let now = std::time::Instant::now();
        let mut tracker = InputTracker::new(now);
        *host.input_observation.lock().unwrap() = Some(tracker.initial_observation(now));
        *host.input_tracker.lock().unwrap() = Some(tracker);
        let mut input = TerminalInput::new(terminal.clone());
        input.replace_and_mark_text(None, "拼", None);
        *host.composition_clear_failures.lock().unwrap() = 1;
        input.cancel_composition();
        assert!(host.input_observation.lock().unwrap().unwrap().composing);
        if restart {
            input.replace_and_mark_text(None, "音", None);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while !restart && host.input_observation.lock().unwrap().unwrap().composing {
            assert!(
                std::time::Instant::now() < deadline,
                "failed clear was never retried"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        if restart {
            std::thread::sleep(Duration::from_millis(250));
        }
        let observation = host.input_observation.lock().unwrap().unwrap();
        assert_eq!(observation.composing, restart);
        assert_eq!(
            observation.state,
            if restart {
                InputState::Drafting
            } else {
                InputState::Idle
            }
        );
        assert!(host.inputs.lock().unwrap().is_empty());
    }
}
