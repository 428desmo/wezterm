-- Optional app-local bindings. Load from ~/.wezterm.lua with dofile(...).apply(config).
local wezterm = require 'wezterm'
local M = {}
function M.apply(config, options)
  options = options or {}
  if not wezterm.target_triple:find('windows', 1, true) then return config end
  local modifier = options.copy_modifier or 'ALT'
  assert(modifier == 'ALT' or modifier == 'SUPER', 'copy_modifier must be ALT or SUPER')
  local keys = config.keys or {}
  local act = wezterm.action
  table.insert(keys, { key = 'c', mods = modifier, action = act.CopyTo 'Clipboard' })
  table.insert(keys, { key = 'v', mods = modifier, action = act.PasteFrom 'Clipboard' })
  table.insert(keys, { key = 'c', mods = 'CTRL', action = act.SendKey { key = 'c', mods = 'CTRL' } })
  -- Preserve terminal Ctrl-D/EOF by default. Editing is normally a shell concern.
  if options.literal_delete then
    table.insert(keys, { key = 'd', mods = 'CTRL', action = act.SendKey { key = 'Delete' } })
    table.insert(keys, { key = 'D', mods = 'CTRL|SHIFT', action = act.SendKey { key = 'd', mods = 'CTRL' } })
  end
  config.keys = keys
  return config
end
return M
