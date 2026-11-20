# tmux native window prototype

## Implementation findings

Base commit: `630ba680e`. Initial checkout was clean; recursive submodules
were populated.

- `mux/src/localpane.rs`: `LocalPaneDCSHandler::handle_device_control` recognizes the control-mode DCS
  and creates `TmuxDomain`. `TmuxDomainState::advance` handles session changes.
- `mux/src/tmux_commands.rs`: `ListCommands::process_result` requests
  `ListAllWindows`; its `process_result` calls `sync_window_state`.
  Each remote window becomes a `Tab`, with remote pane mappings retained.
- `mux/src/tmux.rs`: `create_gui_window` resolves the transport pane's existing
  mux window (or creates a fallback). Previously all tabs were added there.
- `sync_window_state` now optionally creates one mux window per tab using
  `Mux::new_empty_window`, `add_tab_to_window` and builder notification.
  `wezterm-gui/src/frontend.rs` handles `WindowCreated` through
  `reconcile_workspace` and `TermWindow::new_window`. The transport stays open.
- `mux/src/tmux_pty.rs`: `TmuxPtyWriter::write` sends pane-specific `SendKeys`;
  `Resize` in `tmux_commands.rs` uses the remote window and pane IDs. These
  paths are unchanged. Modern tmux's `resize-window` is preferable to the
  older client-wide `refresh-client -C` fallback for independent sizes.

The existing `prefer_to_spawn_tabs` setting concerns new application instances,
not this tmux mapping. No existing config branch was found for this behavior.
The new boolean defaults to false and is sampled at control-mode entry.

## History findings

`ListAllWindows` reads `#{history_limit}`. For existing windows,
`sync_window_state` queues `CapturePane`, which requests
`capture-pane -p -t %ID -e -C -S -LIMIT`. `CapturePane::process_result`
decodes the text and writes it into the new pane's terminal stream, the same
terminal used for scrollback/search/copy. `create_pane` creates a fresh terminal;
`config/src/terminal.rs::scrollback_size` uses `scrollback_lines` (default 3500).

This reconstructs retained tmux history, rather than persisting the previous
client terminal. Lines evicted from tmux, or held only by the old WezTerm pane,
cannot be recovered this way. Capture also does not serialize the entire
terminal state or client scrollback metadata. No history changes were made.
Reattachment was tested on macOS: a marker printed before detach was present
in the new WezTerm pane's scrollback through `wezterm cli get-text` afterward.
GUI search and clipboard operations remain runtime verification items.

## Disposable smoke test (macOS, then Windows 11)

Build with the existing instructions in `docs/install/source.md`. Configure:

```lua
local wezterm = require 'wezterm'
local config = wezterm.config_builder()
config.tmux_window_per_os_window = true
config.scrollback_lines = 20000
return config
```

On a disposable local Linux machine or Mac with tmux, use an isolated socket:

```sh
tmux -L wezterm-ext-smoke -f /dev/null new-session -d -s smoke -n one
tmux -L wezterm-ext-smoke set-option -g history-limit 10000
tmux -L wezterm-ext-smoke new-window -t smoke -n two
tmux -L wezterm-ext-smoke new-window -t smoke -n three
tmux -L wezterm-ext-smoke -CC attach-session -t smoke
```

Run the last command in the modified WezTerm. On Windows, SSH to this disposable
host and run it there; do not use a production server.

1. Confirm three independent native windows plus the transport window. Move and
   resize each separately. In each run `echo "$TMUX_PANE"; stty size` and check
   distinct pane IDs and independent sizes. Type different commands in all three.
2. In one pane run `printf 'WEZTERM_HISTORY_MARKER_73\n'; seq 1 300`.
   Start a background job that writes to a disposable file after a delay.
3. Press `q` in the transport pane to detach, then attach again. Search for
   `WEZTERM_HISTORY_MARKER_73` using WezTerm search and copy/paste the matching
   line. Confirm that the background job completed.
4. Repeat with the SSH connection forcibly interrupted. Check job continuity,
   reattach and search/copy; note any stale windows left by the old connection.
5. Set the boolean to false and attach again: verify the original tab behavior.
6. Clean up only the isolated server: `tmux -L wezterm-ext-smoke kill-server`.

## Validation status

- macOS Apple Silicon (2026-11-20): Rust/cargo 1.100.0 and tmux 3.8 installed.
  `cargo check -p mux -p config`, `cargo fmt --all -- --check`, and
  `git diff --check` passed. Stable rustfmt warns about the existing nightly-only
  `imports_granularity` setting, but reports no formatting differences.
- `cargo build -p wezterm-gui -p wezterm` passed after supplying the C++ header
  location below. The initial build failed in bundled harfbuzz because this
  Mac's Command Line Tools did not search the SDK's libc++ headers. No system
  settings or repository build configuration were changed:

  ```sh
  CXXFLAGS='-isystem /Library/Developer/CommandLineTools/SDKs/MacOSX.sdk/usr/include/c++/v1' cargo build -p wezterm-gui -p wezterm
  ```

- Launched the built GUI with a separate temporary config and an isolated tmux
  socket. `wezterm cli list --format json` confirmed `one`, `two`, `three`
  mapped to window IDs 1, 2, 3 when enabled, and all to window ID 0 when disabled.
  The control transport is a separate pane in window 0 when enabled.
- A PTY-based tmux protocol smoke test passed: actual `-CC` attachment, distinct
  `send-keys -H` input to three panes, detach/reattach and `capture-pane` history
  retrieval. This tests tmux itself, not WezTerm keyboard event routing.
- In the built WezTerm, printed `WEZTERM_RECONNECT_MARKER_73` and 100 numbered
  lines into the disposable tmux session, detached, and launched a fresh
  WezTerm attachment. `wezterm cli get-text --pane-id 1 --start-line -200`
  returned the marker as a standalone line from restored client scrollback.
- Native GUI inspection/input was blocked by the computer-use tool's safety
  restriction on `com.github.wez.wezterm`. Visual window count, moving/resizing,
  keyboard input, GUI search and clipboard were not verified. SSH interruption
  and background-job survival were not exercised. Follow the smoke test above
  for these remaining checks; the performed tests used local tmux only.
- Windows build: not run. `.github/workflows/gen_windows.yml` uses Windows 2025,
  stable Rust, `x86_64-pc-windows-msvc`, recursive checkout and release builds for
  `wezterm`, `wezterm-gui`, and `wezterm-mux-server`, followed by nextest.
  Run this existing PR workflow from an authorized fork, or follow
  `docs/install/source.md` on Windows. Local GitHub CLI authentication is invalid,
  so CI could not be dispatched from this environment.
- Windows 11 GUI: no Windows execution environment was available. Perform the
  smoke test above using the Windows build before relying on the prototype.

Position persistence and validation of windows added after attach are deferred.
