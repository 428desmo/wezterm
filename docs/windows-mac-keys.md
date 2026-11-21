# Windowsでターミナル操作とコピー操作を分ける

既存のWezTermキーバインドで、フォーカスがある時だけキーの動作を定義できます。
OS全体のリマップや常駐フックは追加しません。別のアプリには適用されません。
Windowsキー（WezTermではSUPER/WIN）はOSが先に処理する組み合わせがあり、
全てのショートカットをアプリ側で取得する保証はありません。

## コピーと停止を分ける

`assets/windows-mac-keys.lua` を設定ディレクトリにコピーし、通常の設定の
`return config` より前で読み込みます。デフォルト設定は変更しません。

```lua
dofile(wezterm.config_dir .. '/windows-mac-keys.lua').apply(config)
```

この任意設定ではAlt+Cがコピー、Alt+Vが貼り付け、Ctrl+Cが端末へのCtrl+C送信です。
通常のシェルではCtrl+Cは実行中プログラムへの割り込みですが、端末をraw modeにする
アプリなどでは、そのアプリが解釈します。既存のCtrl+Shift+C/Vも利用できます。
Altを使うため、同じ組み合わせのシェル機能はコピー／貼り付けに置き換わります。
Windows以外では設定を変更しません。

Windowsキーを使いたい場合は `{ copy_modifier = 'SUPER' }` を第2引数に指定できますが、
OSに予約されたWin+CなどがWezTermへ届くかはWindows環境で確認が必要です。
この設定は予約を解除しません。

## Ctrl+DとAlt+D

これは本来、端末よりもシェルの行編集機能です。
SSH先のbash等では通常、Ctrl+Dはカーソル位置の文字削除、Alt+Dは前方の単語削除です。
Ctrl+Dは空行ではEOFとなり、シェルを終了することがあります。Mac固有の機能ではありません。

PowerShellでは `assets/powershell-mac-editing.ps1` を自分のPowerShellプロファイルから
ドットソースすると、WezTerm上のPSReadLineでCtrl+D=DeleteChar、Alt+D=KillWordに
設定します。Ctrl+DをEOFとして使う動作とは異なります。
PowerShell 7とWindows PowerShellではプロファイルの場所が異なるため、使う側の
`$PROFILE` に設定してください。この変更をcmd.exeには適用できません。

どの端末アプリにも一律でCtrl+DをDeleteキーとして送りたい場合だけ、次の設定を使います。

```lua
dofile(wezterm.config_dir .. '/windows-mac-keys.lua').apply(config, {
  literal_delete = true,
})
```

この場合、Ctrl+Shift+Dで元のCtrl+Dを送れます。通常のCtrl+DによるEOFや、
エディタ等が独自に使うCtrl+Dも置き換えるため、シェル側の設定を優先してください。
単語境界はアプリが決めるので、全アプリのAlt+Dを端末だけで統一することはしません。

参照:
- https://wezterm.org/config/keys.html
- https://wezterm.org/config/lua/keyassignment/SendKey.html
- https://learn.microsoft.com/en-us/powershell/module/psreadline/about/about_psreadline_functions
- https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-registerhotkey

Mac上でWindows向け分岐のLua設定読み込みと実アクションへの変換、Macでは変更しないことを確認しました。
Windowsの実キー操作とPowerShellスクリプトの実行は未確認です。
選択範囲をAlt+Cでコピーし、別ペインへAlt+Vで貼り付けること、実行中のコマンドを
Ctrl+Cで中断できること、入力行の途中でCtrl+D/Alt+Dを試すことを確認してください。
別アプリに切り替えた時には、そのアプリの通常の操作になることも確認してください。
