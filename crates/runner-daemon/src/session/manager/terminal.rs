use std::collections::VecDeque;
use std::sync::mpsc;

use runner_core::protocol::terminal::{
    TerminalAttachment, TerminalFrame, TerminalMetadata, TerminalPalette, TerminalSubscription,
};
use runner_core::protocol::ClientError;
use runner_terminal::terminal::{HostResult, ModelOptions, TerminalHost, TerminalModel};

use super::*;

const FRAME_CAPACITY: usize = 64;

/// In-flight IME composition ownership, keyed by session and held by the
/// connection whose preedit the daemon last observed.
#[derive(Default)]
pub(super) struct ControlState {
    compositions: HashMap<String, u64>,
}

#[derive(Default)]
struct QueueState {
    frames: VecDeque<TerminalFrame>,
    closed: bool,
}
#[derive(Default)]
pub(super) struct FrameQueue {
    state: Mutex<QueueState>,
    ready: Condvar,
}
impl FrameQueue {
    fn push(&self, frame: TerminalFrame) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.closed {
            return false;
        }
        if state.frames.len() == FRAME_CAPACITY {
            state.frames.clear();
            state.frames.push_back(TerminalFrame::Resync);
            state.closed = true;
        } else {
            state.frames.push_back(frame);
        }
        self.ready.notify_one();
        !state.closed
    }
    #[cfg(test)]
    pub(super) fn drain(&self) -> Vec<TerminalFrame> {
        self.state.lock().unwrap().frames.drain(..).collect()
    }
    pub(super) fn close(&self) {
        self.state.lock().unwrap().closed = true;
        self.ready.notify_one();
    }
}
struct Frames(Arc<FrameQueue>);
impl Drop for Frames {
    fn drop(&mut self) {
        self.0.close();
    }
}
impl TerminalSubscription for Frames {
    fn cancellation(&self) -> Arc<dyn Fn() + Send + Sync> {
        let queue = self.0.clone();
        Arc::new(move || queue.close())
    }
    fn recv(&mut self) -> std::result::Result<TerminalFrame, ClientError> {
        let mut state = self.0.state.lock().unwrap();
        loop {
            if let Some(frame) = state.frames.pop_front() {
                return Ok(frame);
            }
            if state.closed {
                return Err(ClientError::msg("terminal stream closed"));
            }
            state = self.0.ready.wait(state).unwrap();
        }
    }
}

struct Host {
    manager: Weak<SessionManager>,
    pool: DbPool,
    events: Arc<dyn SessionEvents>,
    agent: bool,
    started_at: Option<String>,
}
impl TerminalHost for Host {
    fn write_input(&self, id: &str, bytes: &[u8]) -> HostResult<()> {
        let manager = self
            .manager
            .upgrade()
            .ok_or_else(|| Error::msg("session manager stopped"))?;
        Ok(manager.inject_direct_stdin(id, bytes, self.events.as_ref())?)
    }
    fn write_reply(&self, id: &str, bytes: &[u8]) -> HostResult<()> {
        let manager = self
            .manager
            .upgrade()
            .ok_or_else(|| Error::msg("session manager stopped"))?;
        Ok(manager.inject_stdin(id, bytes)?)
    }
    fn report_input_state(&self, id: &str, observation: InputObservation) {
        if let Some(manager) = self.manager.upgrade() {
            manager.report_input_state(id, observation);
        }
    }
    fn set_live_title(&self, id: &str, title: &str) -> HostResult<()> {
        if !self.agent {
            self.events.terminal_metadata_changed(id);
            return Ok(());
        }
        let conn = self.pool.get()?;
        if crate::repo::session::set_live_title(
            &conn,
            id,
            (!title.is_empty()).then_some(title),
            self.started_at.as_deref(),
        )? == 0
        {
            return Ok(());
        }
        let mission_id = crate::repo::session::get_row(&conn, id)?.and_then(|row| row.mission_id);
        drop(conn);
        self.events.updated(&SessionUpdatedEvent {
            session_id: id.to_owned(),
            mission_id,
        });
        Ok(())
    }
    fn resize(&self, id: &str, cols: u16, rows: u16) -> HostResult<()> {
        if let Some(manager) = self.manager.upgrade() {
            manager.resize(id, cols, rows, &Arc::new(self.pool.clone()))?;
        }
        Ok(())
    }
    fn is_local_host(&self, host: &str) -> bool {
        crate::shell_integration::is_local_host(host)
    }
}

impl SessionManager {
    pub fn discard_unlisted_terminal(&self, id: &str) {
        self.sessions.lock().unwrap().remove(id);
    }
    pub fn prepare_unlisted_terminal(
        self: &Arc<Self>,
        id: &str,
        size: (u16, u16),
        pool: &DbPool,
        events: &Arc<dyn SessionEvents>,
    ) -> Result<()> {
        let model = self.create_terminal(id, size.0, size.1, pool, events)?;
        let (input, worker) = self.start_terminal_input(id, Arc::clone(&model), Arc::clone(events));
        let state = self.session_state_or_insert(id);
        let mut state = state.lock().unwrap();
        for (_, queue) in state.subscribers.drain(..) {
            queue.close();
        }
        state.last_exit_was_killed = false;
        state.terminal = Some(model);
        state.terminal_input = Some(input);
        state.terminal_input_worker = Some(worker);
        Ok(())
    }
    pub(super) fn create_terminal(
        self: &Arc<Self>,
        id: &str,
        cols: u16,
        rows: u16,
        pool: &DbPool,
        events: &Arc<dyn SessionEvents>,
    ) -> Result<Arc<TerminalModel>> {
        let conn = pool.get()?;
        let runtime = crate::repo::session::effective_runtime(&conn, id)?
            .as_deref()
            .and_then(Runtime::parse);
        let row = crate::repo::session::get_row(&conn, id)?;
        let agent = runtime.is_some_and(|runtime| !runtime.is_shell());
        let cwd = row.as_ref().and_then(|row| row.cwd.clone());
        let title = row
            .as_ref()
            .filter(|_| agent)
            .and_then(|row| row.live_title.as_deref())
            .and_then(|title| crate::session::title::provider_title(title, cwd.as_deref()))
            .unwrap_or_default();
        let started_at = row
            .and_then(|row| row.started_at)
            .map(|time| time.to_rfc3339());
        drop(conn);
        let host = Arc::new(Host {
            manager: Arc::downgrade(self),
            pool: pool.clone(),
            events: Arc::clone(events),
            agent,
            started_at,
        });
        let model = TerminalModel::new(
            host,
            id.to_owned(),
            cols,
            rows,
            ModelOptions {
                codex_native_attention: runtime.is_some_and(|runtime| {
                    crate::runtimes::adapter(runtime)
                        .status_semantics()
                        .native_attention
                }),
                agent,
                shell: runtime.is_some_and(Runtime::is_shell),
                initial_title: title,
                cwd,
            },
        )
        .map_err(|error| Error::msg(error.to_string()))?;
        model.set_palette(*self.terminal_palette.lock().unwrap());
        Ok(model)
    }
    pub fn terminal_model(&self, id: &str) -> Result<Arc<TerminalModel>> {
        let state = self
            .session_state(id)
            .ok_or_else(|| Error::msg(format!("session not found: {id}")))?;
        let model = state
            .lock()
            .unwrap()
            .terminal
            .clone()
            .ok_or_else(|| Error::msg(format!("terminal not found: {id}")))?;
        Ok(model)
    }
    pub fn attach_terminal(&self, id: &str) -> Result<TerminalAttachment> {
        self.attach_terminal_with_history(id, usize::MAX)
    }
    pub fn attach_terminal_with_history(
        &self,
        id: &str,
        history: usize,
    ) -> Result<TerminalAttachment> {
        let state = self
            .session_state(id)
            .ok_or_else(|| Error::msg(format!("session not found: {id}")))?;
        let mut state = state.lock().unwrap();
        Self::attach_terminal_locked(&mut state, history)
    }
    fn attach_terminal_locked(
        state: &mut SessionState,
        history: usize,
    ) -> Result<TerminalAttachment> {
        let model = state
            .terminal
            .as_ref()
            .ok_or_else(|| Error::msg("terminal not found"))?;
        let snapshot = model.snapshot_with_history(state.output_seq, history);
        state.next_subscriber_id += 1;
        let subscriber_id = state.next_subscriber_id;
        let queue = Arc::new(FrameQueue::default());
        state.subscribers.push((subscriber_id, Arc::clone(&queue)));
        Ok(TerminalAttachment {
            subscriber_id,
            snapshot,
            frames: Box::new(Frames(queue)),
        })
    }
    pub fn detach_terminal(&self, id: &str, subscriber: u64) {
        if let Some(state) = self.session_state(id) {
            state.lock().unwrap().subscribers.retain(|(found, queue)| {
                if *found == subscriber {
                    queue.close();
                    false
                } else {
                    true
                }
            });
        }
    }
    #[cfg(test)]
    pub(crate) fn record_terminal_input(&self, id: &str) -> mpsc::Receiver<Vec<u8>> {
        let (tx, rx) = mpsc::channel();
        self.session_state(id)
            .unwrap()
            .lock()
            .unwrap()
            .terminal_input = Some(tx);
        rx
    }

    pub fn desktop_input(&self, id: &str, bytes: &[u8]) -> Result<()> {
        self.queue_terminal_input(id, bytes)
    }

    pub fn observe_terminal_input(
        &self,
        id: &str,
        connection: u64,
        input: runner_core::protocol::terminal::InputEvent,
    ) -> Result<()> {
        use runner_core::protocol::terminal::InputEvent;
        let mut control = self.terminal_control.lock().unwrap();
        let model = {
            let state = self
                .session_state(id)
                .ok_or_else(|| Error::msg("terminal not found"))?;
            let state = state.lock().unwrap();
            state
                .terminal
                .clone()
                .ok_or_else(|| Error::msg("terminal not found"))?
        };
        match input {
            InputEvent::Composing { composing: true } => {
                if control
                    .compositions
                    .get(id)
                    .is_some_and(|owner| *owner != connection)
                {
                    self.clear_composition(id, &mut control);
                }
                control.compositions.insert(id.to_owned(), connection);
            }
            InputEvent::Composing { composing: false } => {
                if control
                    .compositions
                    .get(id)
                    .is_some_and(|owner| *owner != connection)
                {
                    return Ok(());
                }
                control.compositions.remove(id);
            }
            _ => {}
        }
        model.observe_input(&input);
        Ok(())
    }

    fn clear_composition(&self, id: &str, control: &mut ControlState) {
        if control.compositions.remove(id).is_some() {
            if let Ok(model) = self.terminal_model(id) {
                model.observe_input(&runner_core::protocol::terminal::InputEvent::Composing {
                    composing: false,
                });
                self.notify_delivery_event(id, router::SessionDeliveryEvent::InputCleared);
            }
        }
    }

    pub(crate) fn clear_connection_compositions(&self, connection: u64) {
        let mut control = self.terminal_control.lock().unwrap();
        let ids: Vec<_> = control
            .compositions
            .iter()
            .filter(|(_, owner)| **owner == connection)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.clear_composition(&id, &mut control);
        }
    }

    fn queue_terminal_input(&self, id: &str, bytes: &[u8]) -> Result<()> {
        let state = self
            .session_state(id)
            .ok_or_else(|| Error::msg(format!("session not found: {id}")))?;
        let state = state.lock().unwrap();
        state
            .terminal_input
            .as_ref()
            .ok_or_else(|| Error::msg(format!("terminal input unavailable: {id}")))?
            .send(bytes.to_vec())
            .map_err(|_| Error::msg(format!("terminal input worker stopped: {id}")))
    }
    pub(super) fn start_terminal_input(
        &self,
        id: &str,
        model: Arc<TerminalModel>,
        events: Arc<dyn SessionEvents>,
    ) -> (mpsc::Sender<Vec<u8>>, thread::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<Vec<u8>>();
        let id = id.to_owned();
        let worker = thread::Builder::new()
            .name(format!("native-term-input-{id}"))
            .spawn(move || {
                while let Ok(bytes) = rx.recv() {
                    if let Err(error) = model.write_input(&bytes) {
                        events.input_error(&id, &error.to_string());
                    }
                }
            })
            .expect("spawn terminal input thread");
        (tx, worker)
    }
    pub(super) fn push_frame(state: &mut SessionState, frame: TerminalFrame) {
        state
            .subscribers
            .retain(|(_, queue)| queue.push(frame.clone()));
    }
    pub fn resize_terminal(
        &self,
        id: &str,
        _origin: u64,
        cols: u16,
        rows: u16,
        pool: &Arc<DbPool>,
    ) -> Result<()> {
        // The sender also adopts the accepted size from the ordered frame stream.
        self.resize(id, cols, rows, pool)
    }
    pub fn terminal_was_stopped(&self, id: &str) -> bool {
        self.session_state(id).is_some_and(|state| {
            let state = state.lock().unwrap();
            state.killed || state.last_exit_was_killed
        })
    }
}

pub fn metadata(core: &crate::AppCore, id: &str) -> Result<TerminalMetadata> {
    let state = core
        .sessions
        .session_state(id)
        .ok_or_else(|| Error::msg("terminal not found"))?;
    let state = state.lock().unwrap();
    let model = state
        .terminal
        .as_ref()
        .ok_or_else(|| Error::msg("terminal not found"))?;
    let (cols, rows) = model.size();
    Ok(TerminalMetadata {
        title: model.title(),
        live_cwd: model.live_cwd(),
        link_cwd: None,
        cols,
        rows,
        controller: Default::default(),
    })
}
pub fn link_cwd(core: &crate::AppCore, id: &str) -> Result<Option<PathBuf>> {
    let conn = core.db.get()?;
    let Some(row) = crate::repo::session::get_row(&conn, id)? else {
        return Ok(None);
    };
    if let Some(cwd) = row.cwd.filter(|cwd| !cwd.trim().is_empty()) {
        return Ok(Some(PathBuf::from(cwd)));
    }
    let Some(project_id) = row.project_id else {
        return Ok(None);
    };
    Ok(crate::repo::project::get(&conn, &project_id)?.map(|project| PathBuf::from(project.cwd)))
}
pub fn configure(core: &crate::AppCore, id: &str, scrollback: usize, shape: &str) -> Result<()> {
    core.sessions
        .terminal_model(id)?
        .configure_named(scrollback, shape);
    Ok(())
}
pub fn set_palette(core: &crate::AppCore, palette: TerminalPalette) -> Result<()> {
    if let Err(error) = crate::db::set_terminal_palette(&core.db, &palette) {
        log::warn!("persist terminal palette: {error}");
    }
    let palette = runner_terminal::terminal::palette_from_wire(palette);
    let previous = std::mem::replace(
        &mut *core.sessions.terminal_palette.lock().unwrap(),
        palette,
    );
    for id in core.sessions.live_session_ids() {
        if let Ok(model) = core.sessions.terminal_model(&id) {
            model.set_palette(palette);
        }
    }
    if previous != palette {
        core.events.emit("terminal/palette", &());
    }
    Ok(())
}
pub fn palette(core: &crate::AppCore) -> TerminalPalette {
    runner_terminal::terminal::palette_to_wire(*core.sessions.terminal_palette.lock().unwrap())
}
/// Sessions resumed at daemon start answer color queries before any app
/// connects, so they start from the palette the app last sent.
pub fn restore_palette(core: &crate::AppCore) {
    match crate::db::terminal_palette(&core.db) {
        Ok(Some(palette)) => {
            *core.sessions.terminal_palette.lock().unwrap() =
                runner_terminal::terminal::palette_from_wire(palette);
        }
        Ok(None) => {}
        Err(error) => log::warn!("restore terminal palette: {error}"),
    }
}

#[cfg(test)]
mod support;
#[cfg(test)]
mod tests;
