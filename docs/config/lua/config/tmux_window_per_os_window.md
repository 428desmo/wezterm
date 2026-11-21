# `tmux_window_per_os_window`

Controls how tmux control-mode windows are presented. Defaults to `false`:
tmux windows become tabs in the window containing the control connection.

```lua
config.tmux_window_per_os_window = true
```

With this option enabled, each tmux window gets a separate native window in
the control connection's workspace. Set this before attaching with
`tmux -CC attach-session -t SESSION`. The value is read when control mode starts;
changing it requires detaching and attaching again.

The original control connection remains open. Press `q` in that original pane
to detach. Its native window also saves its display/position once the remote
session is identified. Move it once after attaching to record its placement;
the next attachment to that session restores the already-open control window.
Its terminal rows and columns are also saved locally when resized and restored
on reattachment. Pixel dimensions are recalculated for the destination DPI.
Input, output, history capture and resizing use the existing tmux
implementation. Initially test with one pane per tmux window. Runtime window
creation is covered by the local Mac GUI smoke test; split layouts remain
unvalidated. The [standard Launcher integration guide](../../../tmux-desktop.md)
describes SSH connections in `launch_menu` and standard GUI actions for creating, grouping and separating
tmux tabs and changing titles.

On macOS and Windows, native window moves are saved locally in
`tmux-window-positions.json` in WezTerm's data directory (normally
`~/Library/Application Support/wezterm` on macOS). Positions are restored on
reattachment, including after restarting WezTerm. The remote hostname, socket
path, server PID, session creation time and window ID identify each window;
names and ordering are not used. Recreating the tmux server or session starts
a new layout. Desktop positions remain specific to the local machine.

Newly saved positions also include a display identifier and a fractional
position within that display. macOS uses the display UUID from ColorSync;
other backends use WezTerm's screen identifiers (Windows currently includes
the display name/number and adapter). This follows the display if its desktop
coordinates change, and scales the relative position if its resolution changes.
If the display is absent, default placement is used. Existing coordinate-only
files remain readable; move each window once to record display information.
Windows display numbering changes may still require repositioning.

macOS Spaces and Windows virtual-desktop membership are not saved or restored.

Native tmux window creation/position restoration preserves the remote terminal
rows and columns while initial backing-scale events settle. Moving between
different-DPI displays also preserves the grid through the subsequent native
size-correction notifications; deliberate resizing still changes it. If an earlier build already enlarged the tmux grid, resize the
window once to the desired size before testing subsequent restarts.
On macOS, saved tmux windows are created directly on their destination NSScreen
using that screen's font DPI and physical backing scale, rather than being
created on the active screen and moved later. Backing-property changes also
request a redraw so DPI updates do not depend on mouse activity.

Moves are saved after a short delay and pending changes are flushed on normal
application shutdown. Fullscreen and maximized positions are ignored. If a
saved top-left point is outside the currently connected screens, normal window
placement is used. Remote tmux window sizes continue to come from tmux; the
control window's size is stored locally alongside its placement. Existing files
without a control size use the initial size until a size is saved. Multiple
simultaneously running WezTerm processes share the file; avoid changing layouts
in multiple processes at once.

On attach, WezTerm captures each pane's history from tmux. Retention depends on
both tmux's `history-limit` and WezTerm's `scrollback_lines`. History retained
only in a previous WezTerm pane is not restored from that pane on reconnect.
