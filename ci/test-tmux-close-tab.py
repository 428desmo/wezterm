#!/usr/bin/env python3
"""Exercise explicit tmux tab deletion and cancellation in disposable Mac GUIs."""
import json, os, pathlib, shutil, subprocess, sys, tempfile, time
assert sys.platform == 'darwin', 'This GUI smoke test currently supports macOS'
root = pathlib.Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='wezterm-close-tab-') as temp:
    base = [shutil.which('tmux'), '-S', temp+'/tmux.sock', '-f', '/dev/null']
    def tm(*args):
        return subprocess.check_output(base+list(args), text=True).strip()
    process = None
    try:
        tm('new-session','-d','-s','smoke','-n','keep','/bin/sh')
        for name in ('cancel','delete','multi'):
            tm('new-window','-t','smoke','-n',name,'/bin/sh')
        tm('split-window','-t','smoke:3','/bin/sh')
        for phase in (1, 2):
            result = pathlib.Path(temp)/f'result-{phase}'
            config = pathlib.Path(temp)/'config.lua'
            config.write_text(r'''local w=require 'wezterm'
local function remote_tabs()
 local tabs={};local count=0
 for _,win in ipairs(w.mux.all_windows()) do
  for _,tab in ipairs(win:tabs()) do
   if tab:active_pane():get_domain_name()=='tmux' then tabs[tab:get_title()]=tab;count=count+1 end
  end
 end
 return tabs,count
end
local function wait_for(test,done)
 if test() then done() else w.time.call_after(.02,function()wait_for(test,done)end) end
end
local function tab_exists(id)
 return pcall(function()return w.mux.get_tab(id)end)
end
local function finish(gui,pane)
 local f=assert(io.open(RESULT,'w'));f:write('ok');f:close()
 gui:perform_action(w.action.QuitApplication,pane)
end
local function confirmed_close(tab,answer,deleted,done)
 w.log_info('close-test: prompt '..tab:get_title()..' '..string.byte(answer))
 tab:activate()
 local gui=tab:window():gui_window()
 local pane=tab:active_pane();local id=tab:tab_id()
 gui:perform_action(w.action.CloseCurrentTab{confirm=true},pane)
 wait_for(function()return gui:active_pane():pane_id()~=pane:pane_id()end,function()
  w.log_info('close-test: overlay ready')
  local overlay=gui:active_pane()
  gui:perform_action(w.action.SendKey{key=answer=='\x1b' and 'Escape' or answer,mods='NONE'},overlay)
  wait_for(function()
   if deleted then return not tab_exists(id) end
   return gui:active_pane():pane_id()==pane:pane_id()
  end,function()
   if not deleted then assert(w.mux.get_tab(id),'cancel deleted tab') end
   w.log_info('close-test: answer completed')
   done()
  end)
 end)
end
local function start()
 if w.GLOBAL.started then return end
 local tabs,count=remote_tabs()
 if count~=(PHASE==1 and 4 or 2) then w.time.call_after(.1,start);return end
 w.GLOBAL.started=true
 for _,tab in pairs(tabs) do
  local ready,gui=pcall(function()return tab:window():gui_window()end)
  if not ready or not gui then w.GLOBAL.started=false;w.time.call_after(.1,start);return end
 end
 w.log_info('close-test: ready phase PHASE')
 if PHASE==1 then
  local target=tabs.keep:window():window_id()
  tabs.cancel:move_to_window(target);tabs.delete:move_to_window(target)
  confirmed_close(tabs.cancel,'n',false,function()
   confirmed_close(tabs.cancel,'\x1b',false,function()
    confirmed_close(tabs.delete,'y',true,function()
     assert(w.mux.get_tab(tabs.keep:tab_id()) and w.mux.get_tab(tabs.cancel:tab_id()),'sibling tabs deleted')
     assert(#tabs.multi:panes()==2,'multi-pane fixture missing')
     local id=tabs.multi:tab_id();local gui=tabs.multi:window():gui_window()
     gui:perform_action(w.action.CloseCurrentTab{confirm=false},tabs.multi:active_pane())
     wait_for(function()return not tab_exists(id) end,function()
      finish(tabs.keep:window():gui_window(),tabs.keep:active_pane())
     end)
    end)
   end)
  end)
 else
  assert(tabs.keep and tabs.cancel and not tabs.delete and not tabs.multi,'deleted windows resurrected')
  local gui=tabs.keep:window():gui_window()
  gui:perform_action(w.action.SpawnCommandInNewWindow{
   domain={DomainName='local'},args={'/bin/sh','-c',"printf 'LOCAL_SURVIVOR\\n'; exec /bin/sh"},
  },tabs.keep:active_pane())
  local survivor=nil
  wait_for(function()
   for _,win in ipairs(w.mux.all_windows()) do
    for _,tab in ipairs(win:tabs()) do
     local p=tab:active_pane()
     if p:get_domain_name()=='local' and p:get_lines_as_text():find('LOCAL_SURVIVOR',1,true) then survivor=tab;return true end
    end
   end
   return false
  end,function()
   tabs.cancel:activate()
   gui:perform_action(w.action.CloseCurrentTab{confirm=false},tabs.cancel:active_pane())
   wait_for(function()return not tab_exists(tabs.cancel:tab_id()) end,function()
    tabs.keep:activate()
    gui:perform_action(w.action.CloseCurrentTab{confirm=false},tabs.keep:active_pane())
    wait_for(function()local _,n=remote_tabs();return n==0 end,function()
     assert(w.mux.get_tab(survivor:tab_id()),'last tmux window closed unrelated local tab')
     finish(survivor:window():gui_window(),survivor:active_pane())
    end)
   end)
  end)
 end
end
w.on('update-status',start)
w.time.call_after(.1,start)
return {tmux_window_per_os_window=true,automatically_reload_config=false,check_for_updates=false,
 status_update_interval=100,window_close_confirmation='NeverPrompt',mux_enable_ssh_agent=false}
'''.replace('PHASE',str(phase)).replace('RESULT',json.dumps(str(result))))
            with open(temp+f'/gui-{phase}.log','w+') as log:
                process = subprocess.Popen([str(root/'target/debug/wezterm-gui'),'--config-file',str(config),'start','--always-new-process','--']+base+['-CC','attach-session','-t','smoke'],stdout=log,stderr=log,env=dict(os.environ,WEZTERM_TMUX_GROUPS_FILE=temp+'/groups.json'))
                deadline=time.monotonic()+40
                while not result.exists():
                    if process.poll() is not None or time.monotonic()>deadline:
                        log.flush();log.seek(0);raise AssertionError(log.read()+'\nSERVER WINDOWS: '+tm('list-windows','-a','-F','#{window_id} #{window_name}'))
                    time.sleep(.05)
                assert process.wait(timeout=10)==0
                process=None
                log.flush();log.seek(0);contents=log.read()
                assert 'ERROR' not in contents,contents
            if phase==1:
                assert tm('list-windows','-t','smoke','-F','#{window_name}').splitlines()==['keep','cancel']
                assert len(tm('list-panes','-a','-F','#{pane_id}').splitlines())==2
                print('PASS: No/Esc cancel; Yes deletes only target; multi-pane deletion; quit preserves survivors',flush=True)
            else:
                assert subprocess.run(base+['has-session','-t','smoke'],capture_output=True).returncode!=0
                print('PASS: deleted windows stay deleted after reconnect; last remote window leaves local tab alive',flush=True)
    finally:
        if process and process.poll() is None:
            process.terminate();process.wait(timeout=5)
        subprocess.run(base+['kill-server'],capture_output=True)
