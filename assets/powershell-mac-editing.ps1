# Optional: dot-source from the PowerShell $PROFILE used inside WezTerm.
# Restrict these PSReadLine changes to WezTerm sessions.
if ($env:TERM_PROGRAM -eq 'WezTerm') {
    Import-Module PSReadLine
    Set-PSReadLineKeyHandler -Chord 'Ctrl+d' -Function DeleteChar
    Set-PSReadLineKeyHandler -Chord 'Alt+d' -Function KillWord
}
