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

Validation of windows added after attach is deferred.

## Follow-up: normal exit and desktop position restoration

The user confirmed three native windows and session survival after application
restart over SSH to their private VPS. They reported that application exit hung
and that desktop positions reset. The following changes address these reports:

- `sync_window_state` now notifies the transport's retained `MuxWindowBuilder`
  in native mode as well. Previously its `Activity` remained live, preventing
  last-window exit; its unnotified destructor could also call `Mux::get` while
  `Mux::shutdown` held the global mux lock. The added regression test exercises
  three remote windows in both modes and verifies that no Activity remains.
- `ListAllWindows` prefixes each result with a stable remote identity:
  hostname, socket path, server PID, session creation time and window ID.
  `mux/src/tmux_layout.rs` saves local desktop coordinates under that identity.
  Renaming or reordering windows does not change the key. A recreated server
  or session gets a new identity and does not inherit the old placement.
- macOS `windowDidMove:` and Windows `WM_WINDOWPOSCHANGED` emit client-area
  positions in screen pixels. Only native tmux windows register those positions.
  Saves are coalesced for 250 ms and normal mux shutdown flushes pending changes.
  The local file is `tmux-window-positions.json` in `config::DATA_DIR`.
- The existing native `set_window_position` API restores coordinates after
  window creation. This avoids virtual-desktop-origin offsets and differences
  in frame/client origins between platforms. If the saved top-left point is
  outside all current screens, the window keeps normal default placement.
  Maximized/fullscreen coordinates are ignored; sizes still come from tmux.
- File persistence is intended for one WezTerm process managing a layout.
  Concurrent processes can overwrite each other's layout file; per-process
  locking/merging and restoration after recreating the remote server are deferred.

Follow-up validation on macOS:

- Updated `wezterm-gui` and `wezterm` debug binaries built successfully with the
  same SDK C++ header override documented above.
- `cargo test -p mux --lib -- --test-threads=1`: all 7 tests passed, including
  the exit regression, file round-trip with negative coordinates and distinct
  remote identities, file replacement, and invalid-file fallback.
- A fresh disposable local tmux server with three windows attached through
  actual `-CC` to the rebuilt GUI. The official CLI reported three distinct
  mux windows. Closing all local panes with `wezterm cli kill-pane` caused
  WezTerm to exit normally with status 0 within the test timeout. The remote
  tmux server still had all three windows afterward. No external signal was
  required for that successful run; test resources were cleaned up.
- macOS Cmd+Q/menu confirmation and physical drag/restoration remain manual
  checks because the GUI automation tool does not allow this application.
  Windows build and Windows 11 GUI checks remain unperformed.

The user subsequently confirmed on macOS that both application exit and
window position restoration work with the updated build.

Manual follow-up with the existing SSH launch command:

1. Launch the rebuilt binary with `tmux_window_per_os_window=true` and attach
   to the existing disposable session. Move the three windows to distinct
   locations and wait at least one second after the last move.
2. Quit with Cmd+Q and accept the confirmation. Confirm that the terminal
   returns to its prompt without Ctrl+Z or killing the process.
3. Repeat the same launch command and confirm the three window positions.
4. Rename/reorder one tmux window and repeat; its placement should follow its
   window ID. Disconnect a monitor and repeat; off-screen saved positions
   should fall back to normal placement. Repeat these checks on Windows 11.

## Follow-up: physical display identity

Position storage now accepts both old `[x,y]` entries and new objects containing
the absolute coordinates, display ID and fractional position within that display.
macOS obtains the physical display UUID with the public
[`CGDisplayCreateUUIDFromDisplayID`](https://developer.apple.com/documentation/colorsync/cgdisplaycreateuuidfromdisplayid(_:))
function and enumerates NSScreen device descriptions. Its placement rectangles
are converted to the same top-left coordinate system and primary-screen scale
as the existing native position getter/setter; ordinary screen geometry APIs
are left unchanged. Other platforms use the existing WezTerm screen identifiers.
On Windows those include the display number, friendly name and adapter, so a
renumbering can invalidate the match; physical-monitor matching on Windows still
needs a Windows-specific implementation and runtime verification.

Restoration matches the display first, then computes its current desktop
coordinates from the saved fractions. If the display is missing, use normal
placement. Legacy absolute entries retain their prior behavior. Moving an old
window records the new metadata. Window sizes remain controlled by tmux.

macOS Spaces are not implemented. The inspected AppKit public interfaces expose
[`collectionBehavior`](https://developer.apple.com/documentation/appkit/nswindow/collectionbehavior-swift.struct)
and active-Space membership, but no destination-Space-ID operation was found.
No private Spaces APIs or additional accessibility permissions were introduced.
Windows has a public
[`IVirtualDesktopManager`](https://learn.microsoft.com/en-us/windows/win32/api/shobjidl_core/nn-shobjidl_core-ivirtualdesktopmanager)
interface for querying and moving a window's virtual-desktop membership; it is
not implemented here and would need separate Windows validation, including
handling a saved desktop that no longer exists.

Mac debug build succeeded. All 9 mux tests passed, including display identity
after monitor rearrangement/resolution change, missing-display fallback, and
loading the previous position-file format. Physical multi-monitor restoration
and Spaces membership have not been verified automatically. Manual check: move
the three windows to different displays, wait one second, quit and reattach;
then rearrange the displays and repeat. Test display removal as well. Spaces
membership should currently be managed manually. Windows build/GUI remain
unverified.

## Follow-up: mixed-DPI startup grid preservation

The user reported window width/height doubling after moving from Retina to a
lower-DPI display and restarting; moving back did not undo the enlargement.
The previous restore flow created the native window using default-display
pixel geometry and then moved it. Intermediate same-DPI size corrections could
change the terminal grid, and those grid dimensions could be sent to tmux and
become the next startup's source size.

`TermWindow` now retains the initial native tmux rows/columns until native
creation, positioning and outstanding `set_inner_size` requests complete. A
queued completion notification ends the startup guard; deliberate live resizing
also clears it. DPI-only changes in native tmux windows preserve the current
grid even if the generic pixel-geometry heuristic does not recognize a simple
scale change. Default windows continue to use their previous scaling policy.

- Mac debug binaries rebuilt successfully.
- Three focused GUI regression tests passed: transient doubled startup grids,
  a real resize after startup, and unchanged default scaling policy.
- A local disposable three-window tmux session was attached to the actual GUI
  four times using explicit DPI settings 144, 72, 144, 72. Official CLI reads
  confirmed all three local grids and all three remote tmux panes stayed at
  80 columns by 24 rows each time, and each process exited normally afterward.
  This is a configured-DPI test, not a physical mixed-display move test.
- Actual Retina/non-Retina drag-and-restart and Windows build/GUI remain
  unverified. Resize any already-enlarged window to the desired size once;
  repeat moving/restarting in both directions and check `stty size` before
  and after each restart. Existing remote grid corruption cannot reveal the
  original intended dimensions, so no automatic halving is attempted.

## Follow-up: deferred DPI update and control-window placement

The user reported that an enlarged low-DPI window snapped back to the desired
size only when starting a drag. Source inspection found that macOS
`did_change_screen` only set a flag, consumed by a future `draw_rect`, without
requesting that redraw. Programmatic position restoration now explicitly calls
the existing resize/DPI reporting path after changing the native origin and
invalidates the view. Screen-change notifications also mark the view for redraw.
This lets the startup grid guard receive the destination geometry rather than
waiting for mouse activity. A physical mixed-display test is still needed to
confirm that this addresses the reported symptom.

The SSH/tmux control window now gets a session-level placement key, formed from
the same remote host/socket/server/session identity with the session ID and a
`control` suffix rather than a remote window ID. Its identity is only available
after attachment, so the mux requests a late position restore. The frontend
retains that request until the native window is registered, then uses the same
placement and DPI-guard routine as remote tmux windows. Other terminal windows
are unaffected. The mux regression test now verifies the control key and the
one-shot restore request in native mode, and their absence in default mode.

Manual check with the rebuilt binary: first check the low-DPI window's size
immediately after attachment, without moving it. Then place the control window
at a distinct position, wait one second, quit, and reattach. Verify both its
placement and the low-DPI window's size before any mouse drag. Repeat with
Retina/non-Retina displays exchanged. Actual mixed-display and Windows GUI
validation remains unperformed.

## Follow-up: create directly on the destination screen

The user confirmed control-window placement, but the low-DPI window still
needed an interactive move to shrink. The previous deferred-resize change did
not resolve that symptom.

macOS tmux restoration now resolves the saved screen before constructing fonts
or calculating native window geometry. `PlacementScreen` supplies separate
effective font DPI and physical backing scale. `RequestedWindowGeometry` can
carry an initial placement hint. The macOS backend uses the screen-specific
`initWithContentRect:styleMask:backing:defer:screen:` initializer, divides the
requested physical pixel dimensions by the destination backing scale, and
places the window before attaching its view. Restored windows no longer take
the create-on-active-screen/then-move route. Default non-restored window creation
retains its original rounding/placement policy. A `viewDidChangeBackingProperties`
handler also requests screen/DPI reevaluation without waiting for user input.
The working late control-window restore is retained.

Validation on the actual Mac:

- Debug `wezterm-gui` and `wezterm` builds succeeded.
- Added `cargo run -p window --example placement-smoke`, which creates hidden
  windows directly on each connected display and checks the first reported
  backing pixel geometry without mouse movement. The real 2x display reported
  800x480 at DPI 144, and the real 1x display reported 400x240 at DPI 72, matching
  the same requested logical 400x240 size on both. This tests native creation;
  it is not a full saved-layout SSH/tmux restart test.
- A unit test checks backing-size conversion in both directions and distinguishes
  explicit font DPI from physical backing scale.
- The existing local three-window tmux attach/normal-exit smoke test and focused
  terminal-grid DPI tests are rerun with the updated binaries.
- User-visible saved SSH/tmux restart on the mixed displays still requires
  checking immediately after launch, before dragging. Windows remains untested.

## Follow-up: reject resize requests computed at a stale DPI

The user reported rapid 2x/0.5x size oscillation for restored remote tmux windows
on the low-DPI display. This was reproduced with a disposable local tmux server:
three 80x24 windows were placed on the actual 1x display, the GUI was closed, and
the same session was reattached. The remote terminal grids stayed 80x24 even
while native window sizes oscillated; grid-only checks therefore missed the bug.

Trace logging showed initial 72/144 DPI backing notifications being queued
while restored windows became visible. A 1148x850 request computed at DPI 144
was executed after the view had switched to DPI 72, enlarging its logical size.
A following 574x425 correction shrank it again. Creation geometry alone does
not prevent these deferred notifications.

Native tmux windows now include the calculation DPI with size requests. The
macOS backend checks it at execution time, rejects mismatched requests, and
reports current dimensions to let the GUI reconcile its state. Both accepted
and rejected requests deliver the completion notification, allowing the startup
grid guard and repaint queue to drain. Ordinary windows use the original resize
operation, and other backends retain their existing behavior through a default
trait implementation.

Validation on this Mac (actual 2x and 1x displays):

- Debug GUI build and the three `tmux_dpi_tests` passed.
- `cargo run -p window --example placement-smoke` now shows native windows and
  waits for backing notifications. On each screen it submits four stale-DPI
  requests and verifies they leave the physical size unchanged, submits valid
  resize requests and verifies their actual reported sizes, and checks all
  twelve completion notifications arrive. Both displays passed, without a
  mouse move.
- The local three-window saved-layout test ran an initial placement followed by
  three application restarts. Each run kept the GUI and remote grids at 80x24.
  On each restart, trace assertions confirmed that stale-DPI requests were
  rejected and that the low-DPI native resize events never reported the doubled
  1148x850 size. The final events settled at 574x425 / DPI 72. The GUI exited
  normally after each run; the disposable tmux server was removed afterward.
- The user's SSH session was not accessed. Please recheck their existing saved
  layout by restarting the rebuilt application and observing the low-DPI
  windows before moving them, then move one to Retina and back and restart.
- Windows build and Windows 11 GUI remain untested. On Windows, build using the
  repository's Windows instructions and repeat the three-window reconnect and
  mixed-scaling monitor placement check; this specific queue guard is macOS-only.


## Follow-up: persist the control window's terminal grid

The control connection now saves rows and columns locally under its existing
session-level control key. On attachment, the late restore notification loads
that grid and feeds it through the existing DPI-aware size calculation and
startup guard. Size recording begins only after that restore, so initial
connection geometry cannot overwrite the previous saved size. Restoring a size
does not require the saved display to be connected. Position changes preserve
the saved grid and size changes preserve the position. Remote tmux windows
continue to use their server-side terminal sizes.

The position-file reader retains coordinate-array and display-relative entry
compatibility; control entries with a size wrap their optional position. Missing
sizes use the initial grid, and zero row/column counts are ignored.

Validation:

- Five layout persistence tests passed, including independent position/size
  updates, disk reload, size-only entries and old-format compatibility.
- The macOS debug GUI build passed.
- With a disposable local three-window tmux server on the actual 1x display,
  the control window was resized from its default to 30 rows by 102 columns,
  then the GUI was closed and restarted twice. Each reattachment restored
  30x102; all three remote windows remained 24x80. The earlier doubled-size
  native-event assertion also continued to pass. Normal exit succeeded on
  each run and the temporary tmux server was removed. No VPS was contacted.
- Windows compilation and GUI testing remain unperformed. To check manually,
  resize the control window, wait one second, exit and reattach to the same
  tmux session; confirm its terminal rows/columns and position are restored.

## Follow-up: preserve the control grid throughout monitor transitions

The user reported a control-window grid change while moving between displays.
Previously the native-tmux DPI branch preserved cells only for the notification
whose DPI changed. Later same-DPI geometry notifications could still be treated
as ordinary resizing and could save their resulting control size. Native tmux
windows now retain the existing startup grid guard for a monitor transition as
well, until queued native size corrections complete. Live user resizing clears
that guard. During the guarded interval on macOS, transient `isZoomed` reports
are ignored so they cannot block the correction; fullscreen state is retained.

Validation: the four focused DPI tests passed, including staged same-DPI
notifications and explicit resize release. The Mac debug build passed. Using a
local disposable three-window tmux session, an automatically positioned control
window was resized to 46x194 and moved four times between the actual 1x and 2x
displays. Polling confirmed that its grid stayed 46x194 throughout the moves.
After normal GUI exit and reattachment it remained 46x194, with remote tmux
windows still 24x80. The earlier small-window automatic move test also passed
before this change, so these automated moves do not establish reproduction of
every notification sequence from an interactive mouse drag. Please check the
user's actual saved layout with a drag to each display, then deliberately
resize an edge and verify that the new grid persists. Windows remains untested.
