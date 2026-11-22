#!/usr/bin/env python3
"""Verify tmux tab groups/order across three disposable macOS GUI processes."""
import json, os, pathlib, shutil, subprocess, tempfile, time
root = pathlib.Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix='wezterm-groups-') as temp:
    base = [shutil.which('tmux'), '-S', temp+'/tmux.sock', '-f', '/dev/null']
    def tm(*args):
        return subprocess.check_output(base+list(args), text=True).strip()
    process = None
    try:
        tm('new-session', '-d', '-s', 'smoke', '-n', 't0', '/bin/sh')
        for i in range(1, 4):
            tm('new-window', '-t', 'smoke', '-n', 't'+str(i), '/bin/sh')
        for phase in (1, 2, 3):
            result = pathlib.Path(temp)/f'result-{phase}'
            config = pathlib.Path(temp)/'config.lua'
            config.write_text(r'''
local w=require 'wezterm'
w.on('update-status',function()
 if w.GLOBAL.started then return end
 local tabs={};local count=0
 for _,win in ipairs(w.mux.all_windows()) do
  for _,t in ipairs(win:tabs()) do
   if t:active_pane():get_domain_name()=='tmux' then
    tabs[t:get_title()]=t;count=count+1
   end
  end
 end
 if count~=4 or not tabs.t0 or not tabs.t1 or not tabs.t2 or not tabs.t3 then return end
 w.GLOBAL.started=true
 local ready,gui=pcall(function()return tabs.t2:window():gui_window()end)
 if not ready or not gui then w.GLOBAL.started=false;return end
 local function id(t)return t:window():window_id()end
 local function order(expected)
  local actual=tabs.t2:window():tabs()
  assert(#actual==#expected,'wrong group size')
  for i,name in ipairs(expected) do assert(actual[i]:get_title()==name,'wrong tab order') end
 end
 if PHASE==1 then
  tabs.t0:move_to_window(id(tabs.t2));tabs.t1:move_to_window(id(tabs.t2))
  order{'t2','t0','t1'}
 elseif PHASE==2 then
  assert(id(tabs.t0)==id(tabs.t2) and id(tabs.t1)==id(tabs.t2),'group not restored')
  assert(id(tabs.t3)~=id(tabs.t2),'independent window merged')
  order{'t2','t0','t1'}
  tabs.t0:move_to_window()
 else
  assert(id(tabs.t1)==id(tabs.t2),'remaining group lost')
  assert(id(tabs.t0)~=id(tabs.t2) and id(tabs.t3)~=id(tabs.t2),'tear-off not restored')
  order{'t2','t1'}
  if not w.GLOBAL.input_sent then
   w.GLOBAL.input_sent=true
   for name,t in pairs(tabs) do t:active_pane():send_text("printf 'GROUP_INPUT_"..name.."\\n'\n") end
   w.GLOBAL.started=false;return
  end
  for name,t in pairs(tabs) do
   if not t:active_pane():get_lines_as_text():find('GROUP_INPUT_'..name,1,true) then w.GLOBAL.started=false;return end
  end
 end
 local f=assert(io.open(RESULT,'w'));f:write('ok');f:close()
 gui:perform_action(w.action.QuitApplication,tabs.t2:active_pane())
end)
return {tmux_window_per_os_window=true,check_for_updates=false,window_close_confirmation='NeverPrompt',status_update_interval=100,mux_enable_ssh_agent=false}
'''.replace('PHASE',str(phase)).replace('RESULT',json.dumps(str(result))))
            with open(temp+f'/gui-{phase}.log','w+') as log:
                process=subprocess.Popen([str(root/'target/debug/wezterm-gui'),'--config-file',str(config),'start','--always-new-process','--']+base+['-CC','attach-session','-t','smoke'],env=dict(os.environ,WEZTERM_TMUX_GROUPS_FILE=temp+'/groups.json'),stdout=log,stderr=log)
                deadline=time.monotonic()+30
                while not result.exists():
                    if process.poll() is not None or time.monotonic()>deadline:
                        log.flush();log.seek(0);raise AssertionError(log.read())
                    time.sleep(.1)
                assert process.wait(timeout=10)==0
                process=None
                print(f'PASS: restart phase {phase}',flush=True)
        for i in range(4):
            assert 'GROUP_INPUT_t'+str(i) in tm('capture-pane','-p','-t',f'smoke:{i}')
        print('PASS: grouping, saved order, tear-off and independent input survive restart')
    finally:
        if process and process.poll() is None:
            process.terminate();process.wait(timeout=5)
        subprocess.run(base+['kill-server'],capture_output=True)
