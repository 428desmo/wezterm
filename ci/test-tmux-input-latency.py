#!/usr/bin/env python3
"""Local Mac GUI benchmark: delayed transport, 69 individual input writes.

No SSH or network configuration is used. --gui selects a baseline binary.
Reports server echo timing separately from terminal text completion (polled).
Completion means the terminal model contains the text, not physical screen scanout.
"""
import argparse, errno, heapq, json, os, pathlib, pty, select, shutil
import subprocess, sys, tempfile, time, tty


def relay(command, delay):
    tty.setraw(0)
    master, slave = pty.openpty()
    process = subprocess.Popen(command, stdin=slave, stdout=slave, stderr=slave)
    os.close(slave)
    pending = []
    serial = 0
    try:
        while True:
            now = time.monotonic()
            while pending and pending[0][0] <= now:
                _, _, destination, data = heapq.heappop(pending)
                while data:
                    data = data[os.write(destination, data):]
            readable, _, _ = select.select([0, master], [], [],
                max(0, pending[0][0]-time.monotonic()) if pending else .1)
            for source in readable:
                try:
                    data = os.read(source, 65536)
                except OSError as error:
                    if error.errno == errno.EIO:
                        return
                    raise
                if not data:
                    return
                if source == master:
                    with open(os.environ['WEZTERM_TEST_RELAY_LOG'],'ab') as log: log.write(data)
                serial += 1
                heapq.heappush(pending, (time.monotonic()+delay, serial,
                    master if source == 0 else 1, data))
    finally:
        process.terminate()
        process.wait(timeout=5)
        os.close(master)


if len(sys.argv) > 1 and sys.argv[1] == '--relay':
    relay(sys.argv[3:], float(sys.argv[2]))
    raise SystemExit

parser = argparse.ArgumentParser(description=__doc__)
root = pathlib.Path(__file__).resolve().parent.parent
parser.add_argument('--gui', default=str(root/'target/debug/wezterm-gui'))
parser.add_argument('--rtt-ms', type=float, default=237)
parser.add_argument('--interval', type=float, default=.04)
parser.add_argument('--pattern', default='a', help='ASCII letters repeated to 69 input writes')
args = parser.parse_args()
assert args.pattern.isascii() and args.pattern.isalpha()
assert args.rtt_ms >= 0 and args.interval >= 0
payload = (args.pattern*69)[:69]
assert sys.platform == 'darwin', 'GUI benchmark currently supports macOS'
with tempfile.TemporaryDirectory(prefix='wezterm-input-') as temp:
    temp = pathlib.Path(temp)
    base = [shutil.which('tmux'), '-S', str(temp/'tmux.sock'), '-f', '/dev/null']
    sink = temp/'echo.py'
    received = temp/'received.json'
    result = temp/'displayed'
    started = temp/'started'
    screen = temp/'screen'
    sink.write_text('''import os, tty, time, json
 tty.setraw(0)
 os.write(1,b'READY\\r\\n')
 events=[]
 while True:
  data=os.read(0,4096)
  if not data: break
  for byte in data: events.append((time.monotonic(),byte))
  os.write(1,data)
  with open(PATH,'w') as f: json.dump(events,f)
'''.replace('\n ', '\n').replace('PATH',repr(str(received))))
    config = temp/'config.lua'
    config.write_text(r'''local w=require 'wezterm'
local payload=PAYLOAD
w.on('update-status',function()
 for _,win in ipairs(w.mux.all_windows()) do
  for _,tab in ipairs(win:tabs()) do
   local p=tab:active_pane()
   if p:get_domain_name()=='tmux' then
    local text=p:get_lines_as_text()
    local f=assert(io.open(SCREEN,'w'));f:write(text);f:close()
    if not w.GLOBAL.started and text:find('READY',1,true) then
     w.GLOBAL.started=true
     -- Let attach-time commands and resize notifications finish first.
     w.time.call_after(2,function()
      local f=assert(io.open(STARTED,'w'));f:write('start');f:close()
      local function finish()
       if p:get_lines_as_text():find(payload,1,true) then
        local f=assert(io.open(RESULT,'w'));f:write('displayed');f:close()
        win:gui_window():perform_action(w.action.QuitApplication,p)
       else w.time.call_after(.01,finish) end
      end
      w.time.call_after(.01,finish)
      local function send(i)
       p:send_text(payload:sub(i,i))
       if i<69 then w.time.call_after(INTERVAL,function()send(i+1)end) end
      end
      send(1)
     end)
    end
   end
  end
 end
end)
return {tmux_window_per_os_window=true,check_for_updates=false,automatically_reload_config=false,status_update_interval=10,
 window_close_confirmation='NeverPrompt',mux_enable_ssh_agent=false,initial_cols=100}
'''.replace('RESULT',json.dumps(str(result))).replace('STARTED',json.dumps(str(started))).replace('SCREEN',json.dumps(str(screen))).replace('PAYLOAD',json.dumps(payload)).replace('INTERVAL',str(args.interval)))
    process = None
    try:
        subprocess.run(base+['new-session','-d','-s','smoke',sys.executable+' '+str(sink)],check=True)
        with open(temp/'gui.log','w+') as log:
            process = subprocess.Popen([args.gui,'--config-file',str(config),'start','--always-new-process','--',sys.executable,str(pathlib.Path(__file__).resolve()),'--relay',str(args.rtt_ms/2000)]+base+['-CC','attach-session','-t','smoke'],stdout=log,stderr=log,env=dict(os.environ,WEZTERM_TMUX_GROUPS_FILE=str(temp/'groups.json'),WEZTERM_TEST_RELAY_LOG=str(temp/'relay.log')))
            deadline = time.monotonic()+75
            start = None
            while not result.exists():
                if start is None and started.exists(): start=time.monotonic()
                if process.poll() is not None or time.monotonic()>deadline:
                    log.flush();log.seek(0)
                    raise AssertionError(log.read()+'\nSCREEN:\n'+(screen.read_text() if screen.exists() else 'missing'))
                time.sleep(.01)
            display_elapsed=time.monotonic()-start
            assert process.wait(timeout=10)==0
            process=None
            events=json.loads(received.read_text())
            assert [byte for _,byte in events]==list(payload.encode()),events
            duration=events[-1][0]-events[0][0]
            print(json.dumps(dict(rtt_ms=args.rtt_ms,characters=69,
                first_to_last_server_echo_seconds=round(duration,3),
                first_input_to_terminal_text_seconds=round(display_elapsed,3)),indent=2))
            log.flush();log.seek(0)
            contents=log.read()
            assert 'ERROR' not in contents,contents
    finally:
        if process and process.poll() is None:
            process.terminate();process.wait(timeout=5)
        subprocess.run(base+['kill-server'],capture_output=True)
