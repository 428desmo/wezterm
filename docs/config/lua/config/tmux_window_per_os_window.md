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
implementation. Window positions and sizes are not persisted. Initially test
with one pane per tmux window; runtime window creation and split layouts have
not been validated with this option.

On attach, WezTerm captures each pane's history from tmux. Retention depends on
both tmux's `history-limit` and WezTerm's `scrollback_lines`. History retained
only in a previous WezTerm pane is not restored from that pane on reconnect.
