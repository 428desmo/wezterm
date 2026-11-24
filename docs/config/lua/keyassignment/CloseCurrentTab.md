# `CloseCurrentTab`

Closes the current tab, terminating all contained panes.  If that was the last
tab, closes that window.  If that was the last window, wezterm terminates.

```lua
config.keys = {
  {
    key = 'w',
    mods = 'CMD',
    action = wezterm.action.CloseCurrentTab { confirm = true },
  },
}
```

When `confirm` is true, an overlay will render over the tab to ask you to
confirm whether you want to close it.  See also
[skip_close_confirmation_for_processes_named](../config/skip_close_confirmation_for_processes_named.md).


If `confirm` is false then this action will immediately close
the tab and terminates its panes without prompting.


For tmux control-mode tabs, closing the tab deletes the corresponding window
on the tmux server, including its panes and jobs. With `confirm = true`, the
confirmation prompt explains this and can be cancelled with No or Escape.
Closing an OS window or quitting WezTerm preserves the remote tmux windows.
