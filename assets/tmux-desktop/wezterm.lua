local w = require 'wezterm'
local act = w.action
local config = w.config_builder()
config.tmux_window_per_os_window = true
config.scrollback_lines = 20000
config.hide_tab_bar_if_only_one_tab = false
config.check_for_updates = false
local profiles_path = os.getenv('WEZTERM_TMUX_CONNECTIONS_FILE') or (w.home_dir .. '/.wezterm-tmux-connections.json')

local function notice(win, text)
  win:toast_notification('tmux Desktop', text, nil, 8000)
end
local function guarded(win, fn)
  local ok, err = pcall(fn)
  if not ok then notice(win, tostring(err)); w.log_error(tostring(err)) end
end
local function read_profiles()
  local f = io.open(profiles_path, 'r')
  if not f then return {} end
  local text = f:read('*a'); f:close()
  local value = w.json_parse(text)
  assert(value.version == 1 and type(value.connections) == 'table', '接続先ファイルの形式が不正です')
  return value.connections
end
local function save_profiles(profiles)
  local temp = profiles_path .. '.tmp'
  local f = assert(io.open(temp, 'w'))
  assert(f:write(w.json_encode { version = 1, connections = profiles }))
  assert(f:close())
  local ok, err = os.rename(temp, profiles_path)
  if not ok then
    -- Windows rename cannot replace an existing file. Retain a rollback copy.
    local backup = profiles_path .. '.bak'
    os.remove(backup)
    local moved = os.rename(profiles_path, backup)
    assert(moved, err)
    local replaced, replace_error = os.rename(temp, profiles_path)
    if not replaced then os.rename(backup, profiles_path); error(replace_error) end
    os.remove(backup)
  end
end
local function prompt(win, pane, label, initial, callback)
  win:perform_action(act.PromptInputLine {
    description = label, initial_value = initial or '',
    action = w.action_callback(function(window, p, text)
      if text ~= nil then guarded(window, function() callback(window, p, text) end) end
    end),
  }, pane)
end
local function select(win, pane, title, choices, callback)
  win:perform_action(act.InputSelector {
    title = title, fuzzy = true, choices = choices,
    action = w.action_callback(function(window, p, id)
      if id then guarded(window, function() callback(window, p, id) end) end
    end),
  }, pane)
end
local function quote(value)
  return "'" .. value:gsub("'", "'\\''") .. "'"
end
local function attach(win, profile)
  assert(profile.target:match('^[%w_@.:%-]+$') and not profile.target:match('^-'), 'SSH先はUSER@HOSTまたはSSH設定の別名で指定してください')
  assert(profile.session ~= '' and not profile.session:find('[%z\r\n]'), 'セッション名が不正です')
  local remote = 'tmux'
  if profile.socket ~= '' then remote = remote .. ' -L ' .. quote(profile.socket) end
  remote = remote .. ' -CC attach-session -t ' .. quote('=' .. profile.session)
  -- Authentication remains in the visible SSH terminal; no passwords are saved.
  local _, _, muxwin = w.mux.spawn_window {
    args = { 'ssh', '-tt', profile.target, remote },
    workspace = win:mux_window():get_workspace(),
  }
  muxwin:set_title(profile.name .. ' — 接続元')
end
local connections
local function add_profile(win, pane)
  local profile = {}
  prompt(win, pane, '接続先の表示名', '', function(win, pane, name)
    assert(name ~= '', '表示名を入力してください'); profile.name = name
    prompt(win, pane, 'SSH先（USER@HOST または ~/.ssh/config の別名）', '', function(win, pane, target)
      assert(target:match('^[%w_@.:%-]+$') and not target:match('^-'), 'SSH先が不正です'); profile.target = target
      prompt(win, pane, '既存のtmuxセッション名', 'smoke', function(win, pane, session)
        assert(session ~= '' and not session:find('[%z\r\n]'), 'セッション名が不正です'); profile.session = session
        prompt(win, pane, 'tmuxソケット名（-L）。通常は空欄', '', function(win, pane, socket)
          assert(not socket:find('[%z\r\n]'), 'ソケット名が不正です'); profile.socket = socket
          local profiles = read_profiles()
          local replaced = false
          for i, old in ipairs(profiles) do
            if old.name == profile.name then profiles[i] = profile; replaced = true; break end
          end
          if not replaced then table.insert(profiles, profile) end
          save_profiles(profiles)
          connections(win, pane)
        end)
      end)
    end)
  end)
end
connections = function(win, pane)
  guarded(win, function()
    local profiles = read_profiles()
    local choices = { { id = 'add', label = '＋ 接続先を登録（同じ表示名で更新）' } }
    for i, p in ipairs(profiles) do
      table.insert(choices, { id = tostring(i), label = p.name .. '  —  ' .. p.target .. ' / ' .. p.session })
    end
    select(win, pane, '保存済みtmux接続先', choices, function(win, pane, id)
      if id == 'add' then add_profile(win, pane) else attach(win, profiles[tonumber(id)]) end
    end)
  end)
end
local function is_remote(tab)
  local panes = tab:panes()
  if #panes == 0 then return false end
  for _, pane in ipairs(panes) do if pane:get_domain_name() ~= 'tmux' then return false end end
  return true
end
local function remote_tab(win, pane, callback)
  local active = win:mux_window():active_tab()
  if active and is_remote(active) then callback(win, pane, active); return end
  local choices, tabs = {}, {}
  for _, mw in ipairs(w.mux.all_windows()) do
    for _, tab in ipairs(mw:tabs()) do
      if is_remote(tab) then
        local id = tostring(tab:tab_id()); tabs[id] = tab
        table.insert(choices, { id = id, label = tab:get_title() .. '  (window ' .. mw:window_id() .. ')' })
      end
    end
  end
  if #choices == 0 then notice(win, '先にtmuxセッションに接続してください'); return end
  select(win, pane, '操作対象のtmux windowを選択', choices, function(win, pane, id) callback(win, pane, tabs[id]) end)
end
local actions = {
  { 'connect', '保存した接続先へアタッチ', connections },
  { 'new-window', 'tmux window追加 → 独立ウィンドウ', function(win, pane)
      remote_tab(win, pane, function(_, _, tab) tab:spawn_tmux_window(false) end)
    end },
  { 'new-tab', 'tmux window追加 → タブ', function(win, pane)
      remote_tab(win, pane, function(_, _, tab) tab:spawn_tmux_window(true) end)
    end },
  { 'merge', 'このタブを別のウィンドウに統合', function(win, pane)
      remote_tab(win, pane, function(win, pane, tab)
        local choices = {}
        for _, mw in ipairs(w.mux.all_windows()) do
          local tabs = mw:tabs(); local eligible = #tabs > 0
          for _, t in ipairs(tabs) do if not is_remote(t) then eligible = false end end
          if eligible and mw:window_id() ~= tab:window():window_id() then
            table.insert(choices, { id = tostring(mw:window_id()), label = mw:get_title() .. ' / ' .. tabs[1]:get_title() .. '  (window ' .. mw:window_id() .. ')' })
          end
        end
        if #choices == 0 then notice(win, '統合先のtmuxウィンドウがありません'); return end
        select(win, pane, '統合先（接続元は除外）', choices, function(_, _, id) tab:move_to_window(tonumber(id)) end)
      end)
    end },
  { 'detach-tab', 'このタブを独立ウィンドウに切り出す', function(win, pane)
      remote_tab(win, pane, function(_, _, tab) tab:move_to_window() end)
    end },
  { 'rename-tab', 'tmux window / タブの名前を変更', function(win, pane)
      remote_tab(win, pane, function(win, pane, tab)
        prompt(win, pane, 'tmux windowの名前（サーバーにも反映）', tab:get_title(), function(_, _, text) tab:rename_tmux_window(text) end)
      end)
    end },
  { 'rename-window', 'OSウィンドウのタイトルを変更', function(win, pane)
      prompt(win, pane, 'ウィンドウタイトル（空欄で自動タイトル）', win:mux_window():get_title(), function(win, _, text) win:mux_window():set_title(text) end)
    end },
  { 'delete', '保存した接続先を削除', function(win, pane)
      local profiles = read_profiles(); local choices = {}
      for i, p in ipairs(profiles) do table.insert(choices, { id = tostring(i), label = p.name }) end
      select(win, pane, '削除する接続先', choices, function(_, _, id) table.remove(profiles, tonumber(id)); save_profiles(profiles) end)
    end },
}
local function menu(win, pane)
  local choices = {}
  local active = win:mux_window():active_tab()
  for _, entry in ipairs(actions) do
    if entry[1] == 'merge' and active and is_remote(active) then
      local found = false
      for _, target in ipairs(w.mux.all_windows()) do
        local tabs = target:tabs()
        local eligible = #tabs > 0 and target:window_id() ~= active:window():window_id()
        for _, tab in ipairs(tabs) do if not is_remote(tab) then eligible = false end end
        if eligible then
          found = true
          table.insert(choices, {
            id = 'merge:' .. active:tab_id() .. ':' .. target:window_id(),
            label = 'このタブを統合 → ' .. tabs[1]:get_title() .. '  (window ' .. target:window_id() .. ')',
          })
        end
      end
      if not found then table.insert(choices, { id = 'no-merge-target', label = '統合先のtmuxウィンドウがありません' }) end
    else
      table.insert(choices, { id = entry[1], label = entry[2] })
    end
  end
  select(win, pane, 'tmux Desktop', choices, function(win, pane, id)
    local source, target = id:match('^merge:(%d+):(%d+)$')
    if source then
      local tab = w.mux.get_tab(tonumber(source))
      local destination = w.mux.get_window(tonumber(target))
      assert(tab and destination, '対象のウィンドウが閉じられました。メニューを開き直してください')
      tab:move_to_window(tonumber(target))
      return
    end
    if id == 'no-merge-target' then notice(win, '別のtmuxウィンドウを開いてください'); return end
    for _, entry in ipairs(actions) do if entry[1] == id then entry[3](win, pane); break end end
  end)
end
w.on('tmux-desktop-menu', menu)
w.on('new-tab-button-click', function(win, pane) menu(win, pane); return false end)
w.on('augment-command-palette', function()
  local result = {}
  for _, entry in ipairs(actions) do
    table.insert(result, { brief = 'tmux: ' .. entry[2], action = w.action_callback(function(win, pane) guarded(win, function() entry[3](win, pane) end) end) })
  end
  return result
end)
w.on('gui-startup', function()
  local _, _, mw = w.mux.spawn_window {}
  mw:set_title('tmux Desktop — ＋で操作メニュー')
  w.time.call_after(0.5, function()
    local gui = mw:gui_window()
    if gui then connections(gui, mw:active_pane()) end
  end)
end)
config.keys = {
  { key = 'M', mods = 'CMD|SHIFT', action = act.EmitEvent 'tmux-desktop-menu' },
  { key = 'M', mods = 'CTRL|SHIFT', action = act.EmitEvent 'tmux-desktop-menu' },
}
return config
