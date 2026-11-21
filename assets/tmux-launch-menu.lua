-- Example for ~/.wezterm.lua (merge into an existing configuration).
-- No startup hooks, custom connection database, or replacement menus.
local wezterm = require 'wezterm'
local config = wezterm.config_builder()
config.tmux_window_per_os_window = true
config.scrollback_lines = 20000
config.launch_menu = {
  { label = 'Local shell', domain = { DomainName = 'local' } },
  {
    label = 'Development server (tmux: smoke)',
    domain = { DomainName = 'local' },
    args = { 'ssh', '-tt', 'USER@HOST',
      "tmux -L 'wezterm-ext-test' -CC attach-session -t '=smoke'" },
  },
}
-- Rename Tab/Window and the tab context menu are built into the GUI.
return config
