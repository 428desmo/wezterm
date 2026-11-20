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
to detach. Input, output, history capture and resizing use the existing tmux
implementation. Initially test with one pane per tmux window; runtime window
creation and split layouts have not been validated with this option.

On macOS and Windows, native window moves are saved locally in
`tmux-window-positions.json` in WezTerm's data directory (normally
`~/Library/Application Support/wezterm` on macOS). Positions are restored on
reattachment, including after restarting WezTerm. The remote hostname, socket
path, server PID, session creation time and window ID identify each window;
names and ordering are not used. Recreating the tmux server or session starts
a new layout. Desktop positions remain specific to the local machine.

Moves are saved after a short delay and pending changes are flushed on normal
application shutdown. Fullscreen and maximized positions are ignored. If a
saved top-left point is outside the currently connected screens, normal window
placement is used. Window size continues to come from tmux; this file stores
positions only. Multiple simultaneously running WezTerm processes share the
file; avoid changing layouts in multiple processes at once.

On attach, WezTerm captures each pane's history from tmux. Retention depends on
both tmux's `history-limit` and WezTerm's `scrollback_lines`. History retained
only in a previous WezTerm pane is not restored from that pane on reconnect.
