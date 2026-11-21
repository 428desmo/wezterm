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

## Follow-up: return to terminal parsing after tmux exit

The escape parser previously kept its fast tmux-control path active after the
`%exit` record. A following DCS terminator and ordinary transport output (such
as SSH's connection-closed message) could then produce an `Unrecognized tmux cc
line` error. The bulk path now stops at `%exit`, delivers preceding events and
the exit event, clears control-mode state, and feeds remaining bytes through
VTParse. The byte-at-a-time DCS path also clears control mode at `%exit`, and
DCS unhook clears any remaining tmux state.

Validation: all 59 escape-parser tests with `tmux_cc,std` passed. The regression
test tries every two-chunk split of a control stream ending with `%exit`, its
DCS terminator, a synthetic connection-closed message and shell prompt, plus
single-byte input. It verifies delivery of guarded responses and exit, normal
printing of transport text, and cleared tmux state. The Mac debug GUI build
passed. In a disposable local three-window tmux session, a local shell wrapper
printed a synthetic SSH-style message after detach; the GUI's control pane
showed that text, no unrecognized-control-line error was logged, normal GUI
exit succeeded, and all three server-side windows remained. No SSH server was
contacted. Windows build/GUI remain untested.

The separate macOS `IMKCFRunLoopWakeUpReliable` diagnostic is not changed by
this parser fix. No input-method disabling or process-wide log suppression was
added. If desired, append `2>>"$HOME/Library/Logs/wezterm-launch.log"` to the
existing terminal launch command to retain native stderr in a file instead
of the launching terminal; this routes all stderr, not just that diagnostic.

## Follow-up: interrupted transport output during application quit

The user clarified that the remaining connection-closed parse error occurred
when confirming Cmd-Q, rather than explicitly detaching. The previous fix
required `%exit` and therefore did not cover ordinary transport text arriving
before (or without) that notification. The fast control path also bypassed
VTParse's handling of ST and DCS cancellation.

The fast path now yields to VTParse at a DCS terminator/cancellation. If a line
outside a guarded response is ordinary non-`%` text, the control parser returns
it for normal terminal parsing at debug severity rather than reporting an
unrecognized protocol command. Malformed `%` records retain error logging.
Both input paths reset to normal terminal parsing, replay the text and emit
DeviceControl Exit so the mux detaches the control domain. Events preceding
the fallback in the same read are retained, and unread trailing bytes are not
lost. This handles transport termination without matching an SSH hostname or
silencing arbitrary protocol errors.

Validation on Mac:

- All 60 escape-parser tests passed. The new regression covers missing `%exit`
  with no terminator, ESC-backslash ST and C1 ST at every two-chunk boundary and
  with single-byte reads. It checks normal text rendering, domain-exit delivery
  and preservation of preceding guarded responses.
- The debug GUI build passed.
- A local tmux client was forcibly killed while its tmux server remained alive;
  a shell wrapper then printed a synthetic SSH-style connection-closed message.
  The control pane displayed the text, detached its domain and logged no
  unrecognized-control-line error. Normal GUI exit and remote window survival
  were verified.
- A separate disposable GUI invoked QuitApplication with NeverPrompt, exercising
  the termination path without automating the confirmation UI. It exited with
  status 0, logged no control parse error and left all three tmux windows alive.
  An actual Cmd-Q/Yes interaction over the user's SSH connection remains for
  user verification; neither that SSH host nor the production server was used.

## Follow-up: GUI connection profiles and tab management

The optional `assets/tmux-desktop/wezterm.lua` profile provides saved SSH/tmux
connections, a startup selector and a `+` menu. The Mac app packager and Windows
shortcut script isolate it from existing user configuration. See
[the tmux Desktop guide](tmux-desktop.md) for usage and remaining limitations.

The mux Lua tab API now exposes `spawn_tmux_window(as_tab)`,
`move_to_window(optional_window_id)` and `rename_tmux_window(name)`.
New-window commands return their tmux ID and associate an optional destination
before the existing WindowAdd/list/sync path attaches the new tab. Whole-tab
reparenting preserves the tab/pane IDs and the tmux mapping. Renaming is sent
through the existing control connection; normal mux title handling updates the
GUI. The window list now places free-form names after fixed metadata, avoiding
space-delimited field corruption from a user-chosen title.

Japanese rename testing also exposed UTF-8 continuation byte 0x9c being mistaken
for raw C1 ST by the shutdown parser. The control parser now distinguishes that
continuation byte, and switches to control parsing immediately upon DCS entry
within a read. The regression checks Japanese titles at every read split,
while all previous notification-free shutdown tests remain in place.

Validation on Mac: GUI and CLI builds passed, all 61 escape-parser and 10 mux
tests passed, connection-list callback tests passed (including a simulated
Windows rename/replace path), and the disposable local GUI integration test
passed creation as a new OS window and as a tab, merge/separation with stable
IDs, server-side names containing spaces/quotes/Unicode and separate input
markers in the expected server windows. The packaged app was opened through
macOS LaunchServices with a temporary profile file and no SSH connection;
startup completed without Lua errors. Windows build and GUI execution are
still unverified. No production server was contacted. Grouping and local OS
window titles are not persisted across application restart in this change.


## Follow-up: direct merge targets and dragging a tmux tab

The user reported that choosing the merge option appeared to do nothing.
The main desktop menu now lists each eligible merge destination directly as
`このタブを統合 → ...` instead of requiring a second selector from a remote tab.
The callback resolves tab/window IDs again at activation time. A Lua callback
regression verifies the destination list excludes the source and control
window, and that selection moves the expected tab to the expected window.

Opt-in tmux tabs can now be dragged with the left mouse button to another
remote-only tmux window. The gesture preserves normal short clicks, indicates
eligible/ineligible targets, reuses whole-tab movement on release and focuses
the destination. A connection-level hit-test hook has Mac and Windows backends;
AppKit's public windowNumberAtPoint API respects z-order and other apps, while
Windows uses WindowFromPoint/GetAncestor. Other backends return no target.
The source/control window is excluded. No tab is split into new panes.

The Mac debug GUI build passed. The native placement smoke test now verifies
hit testing on both the 2x and 1x display, including a front overlapping window.
The desktop callback and disposable local tmux integration tests were rerun.
Actual mouse dragging and Windows build/GUI testing remain manual checks;
relaunch the rebuilt packaged app, drag a tab title onto another tmux window,
then confirm the selected tab still accepts input into the expected server pane.


### Drag a tmux tab onto exposed desktop

A drag onto exposed desktop now calls the same whole-tab move helper with a
new mux window and an explicit screen-coordinate position. On Mac, that
position takes precedence over saved placement and supplies the destination
screen's DPI/backing scale before font and native geometry creation. Mac
uses AppKit frontmost-window lookup plus CoreGraphics desktop-element exclusion;
Windows recognizes the desktop shell window, while other applications and
ineligible terminal windows reject the drop. Unsupported backends reject it.

Mac GUI/CLI build, configuration callbacks and disposable local tmux integration
passed. The native smoke test rejected desktop classification on the shown
terminal and passed initial-size/stale-DPI checks on both 2x and 1x displays.
No exposed desktop points were available in its sampled grid, so the positive
empty-desktop case and the physical drag gesture still require manual checking.
Quit and relaunch `target/WezTerm Tmux.app`, drag a remote tab title onto an
exposed desktop region on each display, and check placement, size and input.
Also drop over another application to verify no movement. Windows build and
GUI checks remain unperformed; use the existing Windows build and repeat those
steps, plus dragging onto another tmux window to merge.


### Expanded tear-off destinations and Escape cancellation

The desktop-only restriction rejected the other application's visible surface
in the user's screenshot. Tear-off now accepts any on-screen point without a
frontmost window owned by this WezTerm connection. A visible remote-only tmux
window remains a merge target; the source, control and local-shell windows
remain blocked. Thus an obscured WezTerm window does not intercept the drop.
Raw and translated Escape events cancel the pending drag before normal key
bindings or terminal input; subsequent movement and mouse release cannot commit
it. The next tab press starts a new drag.

Mac GUI/CLI build and disposable local tmux regression passed. Physical drag and
Escape interaction remain manual checks: drag onto another app to tear off,
onto a remote tmux window to merge, and press Escape before releasing for each.
Verify no window or tab changes on cancellation, then verify a new drag works.
Windows build and GUI execution remain unverified.


### Replace the desktop prototype with standard UI integration

The dedicated tmux-desktop Lua startup/menu hooks, JSON connection manager,
private app packager, Windows shortcut and associated mocked-UI test were
removed. The existing Launcher reads normal `launch_menu` entries; Shift+Enter
converts launch-menu/domain spawning into the existing NewWindow action.
The normal app/config identity is used by `ci/build-debug-app.py`.
`assets/tmux-launch-menu.lua` is an example, not an automatically loaded profile.
Existing user settings and the previous JSON file are left untouched.

Standard GUI SpawnTab/SpawnCommandInNewWindow now queue tmux window creation
without relying on Domain::spawn's placeholder error. tmux domains use stable
IDs in menu actions so multiple connections named tmux are not confused.
New built-in Window actions expose whole-tab movement and title prompts;
the latter reuse the existing PromptInputLine editor. No Lua menu replacement
is required. Command/cwd/environment overrides for new tmux windows are rejected
explicitly until supported.

Mac GUI/CLI build and example-config loading passed. The disposable local tmux
test now exercises standard spawn and move actions, checks stable identities,
server rename and independent pane input. Its readiness gate avoids reentrant
status events while native windows are still being created. Physical Launcher
selection/Shift+Enter, rename dialogs and the standard macOS menu remain manual
checks. Windows build and GUI execution remain unperformed: build using the
existing Windows workflow, use the same launch_menu configuration, and repeat
connection, new-tab/new-window, title and drag/cancel checks.


### Upstream conventions follow-up

Upstream tracking issue #336 describes tmux control mode as a multiplexer domain
rendered with native windows/tabs/panes. This supports reusing those abstractions,
but is not approval of this branch's UI or platform placement policy. The official
augment-command-palette documentation includes a Rename Tab recipe using
PromptInputLine and tab:set_title. The newly added RenameTab/RenameWindow actions
and prompt-editor refactor were removed in favor of that extension point.
The example builds palette entries synchronously and retains the original rename
target even if the active tab changes before the prompt completes.

Code inspection corrected the previous New Window explanation: SpawnCommand
derives Default and SpawnTabDomain defaults to CurrentPaneDomain. Cmd+N keeps
that routing; the local-shell and SSH-launch entries now explicitly select
DomainName=local. The GUI regression exercises Cmd+N from tmux and local launch
from the same tmux window, checking that the latter does not create a remote
window. Palette tests cover cancellation, original-target identity and local/
tmux title handling. Both tests passed on Mac.

The unused empty-desktop classification API was removed: the current gesture
only needs frontmost WezTerm hit testing. Dynamic move destinations exclude
inactive workspaces and empty windows. Shift+Enter, whole-tab move actions,
native drag and placement remain independent additions requiring separate
upstream review. Acceptance has not been established. Windows build/GUI remains
unverified; normal Launcher selection and physical mouse interaction remain
manual checks.

References: https://github.com/wezterm/wezterm/issues/336,
https://wezterm.org/config/lua/window-events/augment-command-palette.html,
https://github.com/wezterm/wezterm/issues/7057.


Native diagnostic caveat for this run: placement-smoke failed at its first
frontmost-hit assertion on the 2x display. Temporary diagnostics showed the
created test window visible and on the active Space, with the sampled point
inside its frame, but AppKit returned a different frontmost window number.
Explicit focus/activation and an elevated test-window level did not make that
assertion pass. These temporary diagnostics and test-level changes were removed.
This run therefore does not establish native hit-test or DPI-test success;
physical drag and screen-placement confirmation remains outstanding. The
successful standard-action/palette tests do not cover that assertion. No Windows
execution or upstream acceptance is claimed.


### Reorder tabs within one window by dragging

Existing MoveTab/MoveTabRelative actions reorder tabs, but the mouse path did
not use them. Tab presses now begin a drag for local and remote tabs. A drop
onto another tab in the source tab bar selects the original tab by ID and
reuses MoveTab; the order changes only on release. Escape therefore leaves
order intact. Cross-window movement/tear-off retains the existing tmux opt-in.
The GUI regression exercises MoveTab in both directions and checks identity;
physical gestures and Escape need manual checks on Mac and Windows. The earlier
native hit-test failure remains recorded and is not counted as a passing check.
