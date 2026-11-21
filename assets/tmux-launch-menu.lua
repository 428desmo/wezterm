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
-- Standard extension point, following the documented Rename Tab recipe.
-- This hook is synchronous: only use synchronous mux APIs while building entries.
wezterm.on('augment-command-palette', function(window, pane)
  local tab = window:active_tab()
  local mux_window = window:mux_window()
  local entries = {
    {
      brief = 'Rename window',
      action = wezterm.action.PromptInputLine {
        description = 'Enter window title (empty for automatic)',
        initial_value = mux_window:get_title(),
        action = wezterm.action_callback(function(_, _, line)
          if line ~= nil then mux_window:set_title(line) end
        end),
      },
    },
  }
  if tab then
    local active = tab:active_pane()
    local tmux = active and active:get_domain_name() == 'tmux'
    table.insert(entries, {
      brief = tmux and 'Rename tmux window' or 'Rename tab',
      action = wezterm.action.PromptInputLine {
        description = tmux and 'Enter tmux window name (saved on server)' or 'Enter tab title',
        initial_value = tab:get_title(),
        action = wezterm.action_callback(function(_, _, line)
          if line ~= nil then
            if tmux then tab:rename_tmux_window(line) else tab:set_title(line) end
          end
        end),
      },
    })
  end
  return entries
end)
return config
