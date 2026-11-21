# Run once after building wezterm-gui on Windows; opens no connection.
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$exe = Join-Path $root 'target\debug\wezterm-gui.exe'
$config = Join-Path $root 'assets\tmux-desktop\wezterm.lua'
if (-not (Test-Path $exe)) { throw 'Build first: cargo build -p wezterm-gui -p wezterm' }
$path = Join-Path $root 'target\WezTerm Tmux.lnk'
$shortcut = (New-Object -ComObject WScript.Shell).CreateShortcut($path)
$shortcut.TargetPath = $exe
$shortcut.Arguments = '--config-file "' + $config + '" start --always-new-process'
$shortcut.WorkingDirectory = $root
$shortcut.IconLocation = $exe
$shortcut.Save()
Write-Output $path
