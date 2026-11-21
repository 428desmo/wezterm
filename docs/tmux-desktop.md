# 標準Launcherからtmux制御モードを使う

接続先はWezTerm標準のLua設定 `launch_menu` に定義します。
専用ランチャー、起動時の独自メニュー、接続先JSONの読み書きはありません。
既存設定への追記例は `assets/tmux-launch-menu.lua` にあります。
`USER@HOST`、セッション名、必要ならtmuxソケット名を置き換えてください。
既存の `~/.wezterm.lua` がある場合は、設定を丸ごと上書きせず統合してください。

```lua
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
```

SSH認証は通常のSSH端末上で行います。接続先・セッションは明示的な設定であり、
接続した履歴を自動登録するものではありません。旧試作の
`~/.wezterm-tmux-connections.json` は読み書きせず、削除もしません。
必要な項目を `launch_menu` に移してください。

## 起動と接続

標準のWezTermを起動すると通常のローカルターミナルが開きます。
タブバーの「＋」を右クリックしてLauncherを開き、接続先を選択します。
Enterは新しいタブ、Shift+Enterは新しいウィンドウとして起動します。
この修飾操作は `launch_menu` の項目と接続済みDomainの起動項目に適用されます。
ワークスペース切り替えなどの他のLauncher操作は変わりません。

接続するとtmuxの各windowは独立したOSウィンドウとして開きます。
SSH接続元は別に残ります。保存済み配置、倍率変更、終了処理はこれまで通りです。

## 接続後の操作

- **New Tab**（通常の＋クリック／Cmd+T）: 現在のtmux Domainでwindowを追加し、タブとして開きます。
- **New Window (Domain tmux, ID)**: 標準コマンドパレットまたはShellメニューから、そのtmux接続にwindowを追加して独立表示します。IDで複数の接続を区別します。
- **New Window**（Cmd+N）: 既存のアクションと同じく現在のpaneのDomainを使います。tmux paneなら同じセッションにwindowを作ります。ローカルシェルはLauncherのLocal shellを選びます。
- **Move Tab to New Window / Move Tab to Window …**: Windowメニュー／コマンドパレットからタブ全体を移動します。
- **Rename Tab**: 設定不要の標準アクションです。タブ名のダブルクリック、タブの右クリックメニュー、コマンドパレットから呼び出せます。tmuxタブはサーバー側のwindow名も変更します。
- **Rename Window**: タブ／タブバーの空き領域を右クリックし、Rename Windowを選びます。コマンドパレットからも利用できます。空欄で自動タイトルに戻します。

右クリックメニューには既存のTab Navigatorも残しています。OS標準のタイトルバーのダブルクリック（最大化など）は変更しません。
旧設定例の `augment-command-palette` による名前変更項目は不要です。ユーザー設定に残っている場合は、重複を避けるためその名前変更項目だけを外せます。

タブ名を同じウィンドウ内の別のタブ名へドラッグすると、その位置へ並べ替えます。
ローカルタブでも利用でき、tmuxの設定を有効にする必要はありません。
同じWezTermプロセスの別ウィンドウ上で離すと統合します。
ウィンドウ間の移動・切り出しも通常のローカルタブで利用でき、
`tmux_window_per_os_window` を有効にする必要はありません。
接続元ウィンドウにも移動できます。
見えているWezTermの表面以外（他アプリ上も含む）では新しいウィンドウに切り出します。
ドラッグ中のEscは並べ替え・統合・切り出しをキャンセルします。
順序変更はボタンを離した時に確定するため、キャンセル時の順序は変わりません。
並べ替えはWezTerm内の表示順で、tmuxのwindow番号は変更しません。

追加は既存tmuxセッション内へのwindow作成です。別セッションをリンクする操作ではありません。
現在の標準起動操作でのtmux window作成は、コマンド・cwd・環境変数の上書きには対応していません。

## 開発版をGUIから起動

Macでは通常のアプリ名・設定探索を使う開発版を作れます。

```sh
cargo build -p wezterm-gui -p wezterm
python3 ci/build-debug-app.py
```

`target/WezTerm.app` をFinderから起動します。専用設定を強制するラッパーはありません。
通常の設定ファイルを使います。実行ファイル更新後は再パッケージしてください。
旧 `target/WezTerm Tmux.app` は古い試作のコピーなので使わないでください。

Windowsはリポジトリ既存のビルド手順／CIでビルドした `wezterm-gui.exe` を使い、
同じLua設定を適用します。Mac上でWindows用のクロスコンパイル環境は作りません。
WindowsビルドとGUI実行は未確認です。

## 検証

`ci/test-tmux-desktop.py` は使い捨てのローカルtmuxセッションで、標準の
SpawnTab / SpawnCommandInNewWindowアクションからの追加、タブ移動、ID維持、
名前変更と各paneへの入力を確認します。実サーバーには接続しません。
Launcherのマウス／キー操作と名前変更ダイアログは手動確認が必要です。

Windowsでコピーと端末操作を分ける任意設定は [Windows向けキー設定](windows-mac-keys.md) を参照してください。
