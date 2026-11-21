#!/usr/bin/env python3
"""Disposable local tmux integration test; opens GUI windows on macOS."""
import json, os, pathlib, shutil, subprocess, sys, tempfile, time
root=pathlib.Path(__file__).resolve().parent.parent
if sys.platform != 'darwin': raise SystemExit('This GUI smoke test currently uses macOS runtime socket paths')
cli=str(root/'target/debug/wezterm');gui=str(root/'target/debug/wezterm-gui');tmux=shutil.which('tmux')
assert tmux, 'Install tmux before running this test'
with tempfile.TemporaryDirectory(prefix='wezterm-desktop-') as temp:
    base=[tmux,'-S',temp+'/tmux.sock','-f','/dev/null']
    def tm(*args):return subprocess.check_output(base+list(args),text=True).strip()
    process=None
    config=pathlib.Path(temp)/'config.lua'; result_file=pathlib.Path(temp)/'result.json'
    lua=r'''local w=require 'wezterm'
local function remote_tabs()
 local result={}
 for _,win in ipairs(w.mux.all_windows()) do
  for _,tab in ipairs(win:tabs()) do
   local p=tab:active_pane()
   if p and p:get_domain_name()=='tmux' then table.insert(result,tab) end
  end
 end
 table.sort(result,function(a,b)return a:tab_id()<b:tab_id() end)
 return result
end
w.on('update-status',function()
 local stage=w.GLOBAL.desktop_test_stage or 0
 if stage>=3 then return end
 local original={}
 for _,id in ipairs(w.GLOBAL.desktop_test_tabs or {}) do table.insert(original,w.mux.get_tab(id)) end
 local tabs=remote_tabs()
 if stage==0 and #tabs==3 then
  w.GLOBAL.desktop_test_stage=1;original=tabs
  local ids={};for _,t in ipairs(original) do table.insert(ids,t:tab_id()) end;w.GLOBAL.desktop_test_tabs=ids
  original[1]:rename_tmux_window("work 'quoted'; $HOME 日本語")
  original[1]:spawn_tmux_window(true)
 elseif stage==1 and #tabs==4 then
  assert(tabs[4]:window():window_id()==original[1]:window():window_id(),'new tab created in wrong window')
  w.GLOBAL.desktop_test_stage=2;original[1]:spawn_tmux_window(false)
 elseif stage==2 and #tabs==5 then
  w.GLOBAL.desktop_test_stage=3;w.log_info('desktop-test: merge')
  assert(tabs[5]:window():window_id()~=original[1]:window():window_id(),'new OS window not separate')
  local id=original[3]:tab_id();local pane_id=original[3]:active_pane():pane_id()
  local target=original[1]:window():window_id()
  original[3]:move_to_window(target)
  w.log_info('desktop-test: merged')
  assert(original[3]:window():window_id()==target,'merge failed')
  assert(original[3]:tab_id()==id and original[3]:active_pane():pane_id()==pane_id,'identity changed on merge')
  w.log_info('desktop-test: separate')
  local separated=original[3]:move_to_window()
  w.log_info('desktop-test: separated')
  assert(separated:window_id()~=target and original[3]:window():window_id()==separated:window_id(),'separate failed')
  assert(original[3]:tab_id()==id and original[3]:active_pane():pane_id()==pane_id,'identity changed on separation')
  original[1]:window():set_title('Custom OS window')
  assert(original[1]:window():get_title()=='Custom OS window','window title not changed')
  for i,t in ipairs(original) do t:active_pane():send_text("printf 'DESKTOP_MARKER_"..i.."\\n'\n") end
  local f=assert(io.open(RESULT_FILE,'w'));f:write(w.json_encode{tabs=#tabs,tab_id=id,pane_id=pane_id});f:close()
 end
end)
return {tmux_window_per_os_window=true,check_for_updates=false,window_close_confirmation='NeverPrompt',status_update_interval=100,mux_enable_ssh_agent=false}
'''
    config.write_text(lua.replace('RESULT_FILE',json.dumps(str(result_file))))
    with open(temp+'/gui.log','w+') as log:
      try:
        tm('new-session','-d','-s','smoke','-n','one','/bin/sh')
        tm('new-window','-t','smoke','-n','two','/bin/sh');tm('new-window','-t','smoke','-n','three','/bin/sh')
        process=subprocess.Popen([gui,'--config-file',str(config),'start','--always-new-process','--']+base+['-CC','attach-session','-t','smoke'],stdout=log,stderr=log)
        env=dict(os.environ,WEZTERM_UNIX_SOCKET=str(pathlib.Path.home()/'.local/share/wezterm'/('gui-sock-'+str(process.pid))))
        end=time.monotonic()+30
        while not result_file.exists():
            assert process.poll() is None,'GUI exited'
            if time.monotonic()>end:
                log.flush();log.seek(0);raise AssertionError(log.read())
            time.sleep(.1)
        time.sleep(1)
        names=tm('list-windows','-t','smoke','-F','#{window_name}').splitlines()
        assert "work 'quoted'; $HOME 日本語" in names,names
        assert len(names)==5,names
        for i in range(3):
            text=tm('capture-pane','-p','-t',f'smoke:{i}')
            assert f'DESKTOP_MARKER_{i+1}' in text,(i,text)
        print('PASS: new tmux OS window and tab; whole-tab merge/separate; stable tab/pane IDs; quoted server rename; independent input',flush=True)
        panes=json.loads(subprocess.check_output([cli,'cli','--no-auto-start','list','--format','json'],env=env,text=True))
        for p in sorted(panes,key=lambda p:p['pane_id'],reverse=True):subprocess.run([cli,'cli','--no-auto-start','kill-pane','--pane-id',str(p['pane_id'])],env=env,capture_output=True,timeout=5)
        assert process.wait(timeout=10)==0
        process=None
        log.flush();log.seek(0);contents=log.read();assert 'ERROR' not in contents,contents
      finally:
        if process and process.poll() is None:process.terminate();process.wait(timeout=5)
        subprocess.run(base+['kill-server'],capture_output=True)
