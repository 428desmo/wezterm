use crate::activity::Activity;
use crate::domain::{alloc_domain_id, Domain, DomainId, DomainState, SplitSource};
use crate::pane::{Pane, PaneId};
use crate::tab::{SplitRequest, Tab, TabId};
use crate::tmux_commands::{
    ListAllPanes, ListAllWindows, ListCommands, NewWindow, SplitPane, TmuxCommand,
};
use crate::window::WindowId;
use crate::{Mux, MuxWindowBuilder};
use async_trait::async_trait;
use filedescriptor::FileDescriptor;
use parking_lot::{Condvar, Mutex};
use portable_pty::CommandBuilder;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write;
use std::sync::Arc;
use termwiz::tmux_cc::*;
use wezterm_term::TerminalSize;

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
pub enum AttachState {
    Init,
    Done,
}

#[derive(PartialEq, Eq, Debug, Copy, Clone)]
enum State {
    WaitForInitialGuard,
    Idle,
    WaitingForResponse,
    Exit,
}

#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct TmuxRemotePane {
    // members for local
    pub local_pane_id: PaneId,
    pub output_write: FileDescriptor,
    pub active_lock: Arc<(Mutex<bool>, Condvar)>,
    // members sync with remote
    pub session_id: TmuxSessionId,
    pub window_id: TmuxWindowId,
    pub pane_id: TmuxPaneId,
    pub cursor_x: u64,
    pub cursor_y: u64,
    pub pane_width: u64,
    pub pane_height: u64,
    pub pane_left: u64,
    pub pane_top: u64,
}

pub(crate) type RefTmuxRemotePane = Arc<Mutex<TmuxRemotePane>>;

/// As a remote TmuxTab, keeping the TmuxPanes ID
/// within the remote tab.
#[allow(dead_code)]
pub(crate) struct TmuxTab {
    pub tab_id: TabId, // local tab ID
    pub tmux_window_id: TmuxWindowId,
    pub layout_csum: String,
    pub position_key: String,
    pub panes: HashSet<TmuxPaneId>, // tmux panes within tmux window
}

pub(crate) type TmuxCmdQueue = VecDeque<Box<dyn TmuxCommand>>;

// Bound outstanding input commands without imposing a round trip per keystroke.
const MAX_IN_FLIGHT_INPUT: usize = 32;

struct InFlightCommand {
    command: Box<dyn TmuxCommand>,
    replies_left: usize,
    first_error: Option<Guarded>,
}

type InFlightQueue = VecDeque<InFlightCommand>;

fn can_send_command(command: &dyn TmuxCommand, in_flight: &InFlightQueue) -> bool {
    in_flight.is_empty()
        || (in_flight.len() < MAX_IN_FLIGHT_INPUT
            && command.is_input()
            && in_flight.iter().all(|sent| sent.command.is_input()))
}

pub(crate) struct TmuxDomainState {
    pub pane_id: PaneId,     // ID of the original pane
    pub domain_id: DomainId, // ID of TmuxDomain
    state: Mutex<State>,
    pub cmd_queue: Arc<Mutex<TmuxCmdQueue>>,
    in_flight: Mutex<InFlightQueue>,
    pub gui_window: Mutex<Option<MuxWindowBuilder>>,
    pub gui_tabs: Mutex<HashMap<TmuxWindowId, TmuxTab>>,
    pub remote_panes: Mutex<HashMap<TmuxPaneId, RefTmuxRemotePane>>,
    pub tmux_session: Mutex<Option<TmuxSessionId>>,
    pub support_commands: Mutex<HashMap<String, String>>,
    pub attach_state: Mutex<AttachState>,
    pending_splits: Mutex<VecDeque<promise::Promise<TmuxPaneId>>>,
    pub backlog: Mutex<HashMap<TmuxPaneId, Vec<u8>>>,
    pub pending_window_targets: Mutex<HashMap<TmuxWindowId, WindowId>>,
    // Snapshot at attach so a config reload cannot change the mapping mid-session.
    pub window_per_os_window: bool,
}

pub struct TmuxDomain {
    pub(crate) inner: Arc<TmuxDomainState>,
}

impl TmuxDomainState {
    pub fn advance(&self, events: Box<Vec<Event>>) {
        for event in events.iter() {
            let state = *self.state.lock();
            log::debug!("tmux: {:?} in state {:?}", event, state);
            match event {
                // Tmux generic events
                Event::Guarded(response) => match state {
                    State::WaitForInitialGuard => {
                        *self.state.lock() = State::Idle;
                    }
                    State::WaitingForResponse => {
                        let mut in_flight = self.in_flight.lock();
                        if let Some(first) = in_flight.front_mut() {
                            if response.error && first.first_error.is_none() {
                                first.first_error = Some(response.clone());
                            }
                            first.replies_left -= 1;
                            if first.replies_left == 0 {
                                let sent = in_flight.pop_front().unwrap();
                                let domain_id = self.domain_id;
                                if in_flight.is_empty() {
                                    *self.state.lock() = State::Idle;
                                }
                                let resp = sent.first_error.unwrap_or_else(|| response.clone());
                                let command = sent.command;
                                promise::spawn::spawn_into_main_thread(async move {
                                    if let Err(err) = command.process_result(domain_id, &resp) {
                                        log::error!(
                                            "Tmux processing command result error: {}",
                                            err
                                        );
                                    }
                                })
                                .detach();
                            }
                        }
                    }
                    State::Idle => {}
                    State::Exit => {}
                },

                // Tmux specific events
                Event::ConfigError { error } => {
                    // tmux config file error, not our fault, just log it and go
                    log::warn!("tmux configuration error: {error}");
                }
                Event::Exit { reason: _ } => {
                    *self.state.lock() = State::Exit;
                    let mut pane_map = self.remote_panes.lock();
                    for (_, v) in pane_map.iter_mut() {
                        let remote_pane = v.lock();
                        let (lock, condvar) = &*remote_pane.active_lock;
                        let mut released = lock.lock();
                        *released = true;
                        condvar.notify_all();
                    }
                    let mut cmd_queue = self.cmd_queue.as_ref().lock();
                    cmd_queue.clear();
                    self.in_flight.lock().clear();

                    // Force to quit the tmux mode
                    let pane_id = self.pane_id;
                    promise::spawn::spawn_into_main_thread_with_low_priority(async move {
                        if let Some(x) = Mux::get().get_pane(pane_id) {
                            let _ = write!(x.writer(), "\n\n");
                        }
                    })
                    .detach();

                    return;
                }
                Event::LayoutChange {
                    window,
                    layout,
                    visible_layout: _,
                    raw_flags: _,
                } => {
                    let mut cmd_queue = self.cmd_queue.as_ref().lock();
                    cmd_queue.push_back(Box::new(ListAllPanes {
                        window_id: *window,
                        prune: true,
                        layout_csum: if let Some(l) = layout.get(0..4) {
                            l.to_string()
                        } else {
                            "".to_string()
                        },
                    }));
                }
                Event::Output { pane, text } => {
                    let pane_map = self.remote_panes.lock();
                    if let Some(ref_pane) = pane_map.get(pane) {
                        let mut tmux_pane = ref_pane.lock();
                        if let Err(err) = tmux_pane.output_write.write_all(text) {
                            log::error!("Failed to write tmux data to output: {:#}", err);
                        }
                    } else {
                        // the output may come early then pane is ready, in this case we
                        // backlog it
                        self.backlog.lock().insert(*pane, text.to_vec());
                        log::debug!("Tmux pane {} haven't been attached", pane);
                    }
                }
                Event::SessionChanged { session, name: _ } => {
                    *self.tmux_session.lock() = Some(*session);
                    let mut cmd_queue = self.cmd_queue.as_ref().lock();
                    cmd_queue.push_back(Box::new(ListCommands));

                    self.subscribe_notification();
                    log::info!("tmux session changed:{}", session);
                }
                Event::WindowAdd { window } => {
                    // Only handle the new tab, the first empty window handled by sync_window_state
                    if !self.gui_window.lock().is_none() {
                        if let Some(session) = *self.tmux_session.lock() {
                            let mut cmd_queue = self.cmd_queue.as_ref().lock();
                            cmd_queue.push_back(Box::new(ListAllWindows {
                                session_id: session,
                                window_id: Some(*window),
                            }));
                            log::info!("tmux window add: {}:{}", session, window);
                        }
                    }
                }
                Event::WindowClose { window } => {
                    let _ = self.remove_detached_window(*window);
                }
                Event::WindowPaneChanged { window, pane } => {
                    // The tmux 2.7 WindowPaneChanged event comes early than WindowAdd, we need to
                    // skip it
                    if !self.check_window_attached(*window) {
                        continue;
                    }

                    // Split pane
                    if !self.check_pane_attached(*window, *pane) {
                        let mut pending_splits = self.pending_splits.lock();
                        if let Some(mut promise) = pending_splits.pop_front() {
                            promise.ok(*pane);
                        }
                    }
                    log::info!("tmux window pane changed: {}:{}", window, pane);
                }
                Event::WindowRenamed { window, name } => {
                    let gui_tabs = self.gui_tabs.lock();
                    if let Some(x) = gui_tabs.get(&window) {
                        let mux = Mux::get();
                        if let Some(tab) = mux.get_tab(x.tab_id) {
                            tab.set_title(&format!("{}", name));
                        }
                    }
                }
                Event::UnlinkedWindowClose { window } => {
                    let _ = self.remove_detached_window(*window);
                }
                _ => {}
            }
        }

        // send pending commands to tmux
        let cmd_queue = self.cmd_queue.as_ref().lock();
        if matches!(*self.state.lock(), State::Idle | State::WaitingForResponse)
            && !cmd_queue.is_empty()
        {
            TmuxDomainState::schedule_send_next_command(self.domain_id);
        }
    }

    /// Send input without waiting for each preceding input's reply. Keep all
    /// other commands serialized: their results may change local mux state.
    /// Must be called inside the main thread.
    fn send_next_command(&self) {
        let mut pending = self.cmd_queue.lock();
        let mut in_flight = self.in_flight.lock();
        if !matches!(*self.state.lock(), State::Idle | State::WaitingForResponse) {
            return;
        }
        let mux = Mux::get();
        let Some(pane) = mux.get_pane(self.pane_id) else {
            return;
        };
        let mut writer = pane.writer();
        while let Some(first) = pending.front() {
            if !can_send_command(first.as_ref(), &in_flight) {
                break;
            }
            let wire_command = first.get_command(self.domain_id);
            if wire_command.is_empty() {
                pending.pop_front();
                continue;
            }
            // Keep reply handlers in wire order, including failed tmux commands.
            // Hold in_flight until the write completes so a fast reply cannot
            // overtake registration of its handler.
            let command = pending.pop_front().unwrap();
            in_flight.push_back(InFlightCommand {
                // Resize emits two command lines, each with its own guard.
                replies_left: command.response_count(),
                command,
                first_error: None,
            });
            *self.state.lock() = State::WaitingForResponse;
            if let Err(err) = writer.write_all(wire_command.as_bytes()) {
                // A partial write cannot safely be retried on this transport.
                *self.state.lock() = State::Exit;
                pending.clear();
                in_flight.clear();
                log::error!("Writing tmux command failed: {err:#}");
                break;
            }
        }
    }

    /// schedule a `send_next_command` into main thread
    pub fn schedule_send_next_command(domain_id: usize) {
        promise::spawn::spawn_into_main_thread(async move {
            let mux = Mux::get();
            if let Some(domain) = mux.get_domain(domain_id) {
                if let Some(tmux_domain) = domain.downcast_ref::<TmuxDomain>() {
                    tmux_domain.send_next_command();
                }
            }
        })
        .detach();
    }

    /// create a standalone window for tmux tabs
    pub fn create_gui_window(&self) {
        if self.gui_window.lock().is_none() {
            let mux = Mux::get();
            let window_builder =
                if let Some((_domain, window_id, _tab)) = mux.resolve_pane_id(self.pane_id) {
                    MuxWindowBuilder {
                        window_id,
                        activity: Some(Activity::new()),
                        notified: false,
                    }
                } else {
                    mux.new_empty_window(
                        None, /* TODO: pass session here */
                        None, /* position */
                    )
                };

            log::info!("Tmux create window id {}", window_builder.window_id);
            {
                let mut window_id = self.gui_window.lock();
                *window_id = Some(window_builder); // keep the builder so it won't be purged
            }
        };
    }

    /// create a tmux window
    pub fn create_tmux_window(&self) {
        let mut cmd_queue = self.cmd_queue.as_ref().lock();
        cmd_queue.push_back(Box::new(NewWindow { target: None }));
        TmuxDomainState::schedule_send_next_command(self.domain_id);
    }

    /// split the tmux pane
    pub fn split_tmux_pane(
        &self,
        _tab: TabId,
        pane_id: PaneId,
        split_request: SplitRequest,
    ) -> anyhow::Result<()> {
        let tmux_pane_id = self
            .remote_panes
            .lock()
            .iter()
            .find(|(_, ref_pane)| ref_pane.lock().local_pane_id == pane_id)
            .map(|p| p.1.lock().pane_id);

        if let Some(id) = tmux_pane_id {
            let mut cmd_queue = self.cmd_queue.as_ref().lock();
            cmd_queue.push_back(Box::new(SplitPane {
                pane_id: id,
                direction: split_request.direction,
            }));
            TmuxDomainState::schedule_send_next_command(self.domain_id);
            return Ok(());
        } else {
            anyhow::bail!("Could not find the tmux pane peer for local pane: {pane_id}");
        }
    }
}

impl TmuxDomain {
    pub fn new(pane_id: PaneId) -> Self {
        let domain_id = alloc_domain_id();
        let cmd_queue = VecDeque::new();
        let inner = Arc::new(TmuxDomainState {
            domain_id,
            pane_id,
            // parser,
            state: Mutex::new(State::WaitForInitialGuard),
            cmd_queue: Arc::new(Mutex::new(cmd_queue)),
            in_flight: Mutex::new(VecDeque::new()),
            gui_window: Mutex::new(None),
            gui_tabs: Mutex::new(HashMap::default()),
            remote_panes: Mutex::new(HashMap::default()),
            tmux_session: Mutex::new(None),
            support_commands: Mutex::new(HashMap::default()),
            attach_state: Mutex::new(AttachState::Init),
            pending_splits: Mutex::new(VecDeque::default()),
            backlog: Mutex::new(HashMap::default()),
            pending_window_targets: Mutex::new(HashMap::default()),
            window_per_os_window: config::configuration().tmux_window_per_os_window,
        });

        Self { inner }
    }

    pub fn create_window(&self, target: Option<WindowId>) -> anyhow::Result<()> {
        anyhow::ensure!(
            *self.inner.state.lock() != State::Exit,
            "tmux connection is closed"
        );
        self.inner
            .cmd_queue
            .lock()
            .push_back(Box::new(NewWindow { target }));
        TmuxDomainState::schedule_send_next_command(self.inner.domain_id);
        Ok(())
    }

    pub fn position_key_for_tab(&self, tab: TabId) -> Option<String> {
        self.inner
            .gui_tabs
            .lock()
            .values()
            .find(|t| t.tab_id == tab)
            .map(|t| t.position_key.clone())
    }

    pub fn rename_window(&self, tab: TabId, name: &str) -> anyhow::Result<()> {
        anyhow::ensure!(
            !name.chars().any(char::is_control),
            "Title cannot contain control characters"
        );
        let window = self
            .inner
            .gui_tabs
            .lock()
            .values()
            .find(|t| t.tab_id == tab)
            .map(|t| t.tmux_window_id)
            .ok_or_else(|| anyhow::anyhow!("tmux tab not found"))?;
        self.inner
            .cmd_queue
            .lock()
            .push_back(Box::new(crate::tmux_commands::RenameWindow {
                window,
                name: name.to_string(),
            }));
        TmuxDomainState::schedule_send_next_command(self.inner.domain_id);
        Ok(())
    }

    fn send_next_command(&self) {
        self.inner.send_next_command();
    }
}

#[async_trait(?Send)]
impl Domain for TmuxDomain {
    async fn spawn(
        &self,
        _size: TerminalSize,
        _command: Option<CommandBuilder>,
        _command_dir: Option<String>,
        _window: WindowId,
    ) -> anyhow::Result<Arc<Tab>> {
        self.inner.create_tmux_window();
        // This is intention, we would not return a Tab, since we don't have now!
        // We use create_tmux_window to create back end tmux window, then the
        // Tmux WindowAdd event will triage us to do the rest things.
        anyhow::bail!("Intention: we use tmux command to do so");
    }

    async fn split_pane(
        &self,
        _source: SplitSource,
        tab: TabId,
        pane_id: PaneId,
        split_request: SplitRequest,
    ) -> anyhow::Result<Arc<dyn Pane>> {
        let mut promise = promise::Promise::new();
        if let Some(future) = promise.get_future() {
            {
                let mut pending_splits = self.inner.pending_splits.lock();
                let _ = self.inner.split_tmux_pane(tab, pane_id, split_request)?;
                pending_splits.push_back(promise);
            }

            if let Ok(id) = future.await {
                let pane = self.inner.split_pane(tab, pane_id, id, split_request);
                return pane;
            }
        }

        anyhow::bail!("Split_pane failed");
    }

    async fn spawn_pane(
        &self,
        _size: TerminalSize,
        _command: Option<CommandBuilder>,
        _command_dir: Option<String>,
    ) -> anyhow::Result<Arc<dyn Pane>> {
        anyhow::bail!("Spawn_pane not yet implemented for TmuxDomain");
    }

    fn domain_id(&self) -> DomainId {
        self.inner.domain_id
    }

    fn domain_name(&self) -> &str {
        "tmux"
    }

    async fn domain_label(&self) -> String {
        format!("tmux connection {}", self.inner.domain_id)
    }

    async fn attach(&self, _window_id: Option<crate::WindowId>) -> anyhow::Result<()> {
        Ok(())
    }

    fn detachable(&self) -> bool {
        false
    }

    fn detach(&self) -> anyhow::Result<()> {
        anyhow::bail!("detach not implemented for TmuxDomain");
    }

    fn state(&self) -> DomainState {
        DomainState::Attached
    }
}

#[cfg(test)]
mod input_pipeline_tests {
    use super::*;
    use crate::tmux_commands::SendKeys;

    fn sent(command: Box<dyn TmuxCommand>, replies_left: usize) -> InFlightCommand {
        InFlightCommand {
            command,
            replies_left,
            first_error: None,
        }
    }

    fn input() -> Box<dyn TmuxCommand> {
        Box::new(SendKeys {
            pane: 1,
            keys: vec![b'a'],
        })
    }

    #[test]
    fn input_overlaps_but_structural_commands_are_barriers() {
        let structural = ListCommands;
        let mut in_flight = VecDeque::new();
        assert!(can_send_command(&structural, &in_flight));
        in_flight.push_back(sent(input(), 1));
        assert!(can_send_command(input().as_ref(), &in_flight));
        assert!(!can_send_command(&structural, &in_flight));
        for _ in 1..MAX_IN_FLIGHT_INPUT {
            in_flight.push_back(sent(input(), 1));
        }
        assert!(!can_send_command(input().as_ref(), &in_flight));
        in_flight.pop_front();
        assert!(can_send_command(input().as_ref(), &in_flight));
        in_flight.clear();
        in_flight.push_back(sent(Box::new(ListCommands), 1));
        assert!(!can_send_command(input().as_ref(), &in_flight));
        assert!(!can_send_command(&structural, &in_flight));
    }

    #[test]
    fn guarded_replies_match_sent_commands_even_after_an_error() {
        #[derive(Debug)]
        struct Record(usize, Arc<Mutex<Vec<(usize, bool)>>>);
        impl TmuxCommand for Record {
            fn is_input(&self) -> bool {
                true
            }
            fn get_command(&self, _: DomainId) -> String {
                unreachable!()
            }
            fn process_result(&self, _: DomainId, result: &Guarded) -> anyhow::Result<()> {
                self.1.lock().push((self.0, result.error));
                Ok(())
            }
        }
        let executor = promise::spawn::ScopedExecutor::new();
        let domain = TmuxDomain::new(usize::MAX);
        let seen = Arc::new(Mutex::new(vec![]));
        for id in 0..3 {
            domain.inner.in_flight.lock().push_back(sent(
                Box::new(Record(id, seen.clone())),
                if id == 1 { 2 } else { 1 },
            ));
        }
        *domain.inner.state.lock() = State::WaitingForResponse;
        for id in 0..4 {
            domain.inner.advance(Box::new(vec![Event::Guarded(Guarded {
                timestamp: 0,
                number: id,
                flags: 1,
                error: id == 1,
                output: String::new(),
            })]));
            assert_eq!(
                domain.inner.in_flight.lock().len(),
                [2, 2, 1, 0][id as usize]
            );
            assert_eq!(
                *domain.inner.state.lock(),
                if id == 3 {
                    State::Idle
                } else {
                    State::WaitingForResponse
                }
            );
        }
        smol::block_on(executor.run(async {
            while seen.lock().len() < 3 {
                smol::future::yield_now().await;
            }
        }));
        assert_eq!(*seen.lock(), vec![(0, false), (1, true), (2, false)]);
    }
}
