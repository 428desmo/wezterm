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

## タブのグループを再接続時に復元

タブを別ウィンドウに統合・切り出ししたときや並べ替えたときに、tmuxタブの
グループ分けと表示順を `tmux-tab-groups.json` に保存します。
保存先は位置保存ファイルと同じWezTermのデータディレクトリです。
`tmux_window_per_os_window=true` で同じtmuxセッションに再接続すると復元します。
SSH接続元タブもグループに含められます。一般のローカルシェルの再起動は対象外です。
この機能の追加前にはグループ情報が保存されていないため、一度だけ再配置してください。
終了時のタブ削除や途中までのattach状態で、保存したグループを上書きしません。

`ci/test-tmux-group-restore.py` はローカルの使い捨てtmuxを利用し、Mac GUIを
3回起動・終了して、グループと順序の復元、切り出し後の復元、各タブへの入力を検証します。
検証用の保存先は `WEZTERM_TMUX_GROUPS_FILE` で隔離します。

## CC入力の遅延検証

CC入力は `send-keys` の応答を1文字ごとに待たず、最大32件まで送信します。
リサイズ、window作成など入力以外のコマンドは、前後の入力との順序を保つため
応答待ちの境界として扱います。リサイズが返す2つの応答も消費してから後続へ進みます。

Macでは以下の検証ができます。使い捨てローカルtmuxとPTY中継だけを使用し、
実サーバーへのSSH接続やOSのネットワーク設定変更は行いません。

```sh
python3 ci/test-tmux-input-latency.py --rtt-ms 237
python3 ci/test-tmux-input-latency.py --rtt-ms 237 --pattern abcdefghijklmnopqrstuvwxyz --interval 0
```

中継は各方向のデータを118.5msずつ遅らせます。
初期attach完了後、69文字をそれぞれ独立した書き込みとして送信し、
受信内容が欠落・重複・順序違いなく一致することをサーバー側とWezTermの端末内容で確認します。
`--gui /path/to/wezterm-gui` で変更前の実行ファイルとも比較できます。

2026-11-24、Macのdebugビルドでの測定値（単回測定）:

- 40ms間隔の69文字、サーバーの最初から最後のエコー: 変更前17.383秒、変更後2.759秒。
- 最初の入力からWezTermの端末内容に全69文字がそろうまで: 変更前17.650秒、変更後3.017秒。
- 異なる文字を待ち時間なしで69回送信: 修正後の端末内容確認まで0.775秒。内容・順序一致。

端末内容の完了検出はタイマーでポーリングするため小さな測定誤差を含みます。
物理キー入力から液晶表示までの測定ではありません。SSHやhost Bはこの検証に含みません。
Windowsのreleaseビルド・GUIと実際のSSH経路では未確認です。
Windowsでは修正版をreleaseビルドし、同じ接続経路・入力条件で
最初から69文字目までの時間を再測定してください。

## Windowsでタブを切り出したときの画面内配置

タブのドロップ位置は絶対デスクトップ座標として扱います。
左側・上側のディスプレイでは負の座標をそのまま使用し、仮想デスクトップ原点を二重に加算しません。
Windowsで明示座標のウィンドウを作成・配置するときは、対象ディスプレイの作業領域
（タスクバーを除く `rcWork`）へ、タイトルバーと枠を含む外形を収めます。
プログラムからのサイズ変更でも同じ補正を行い、DPI調整などで再びはみ出すのを防ぎます。
作業領域より大きい場合はサイズを縮めます。手動のドラッグ移動は制限しません。

Macで共通の配置計算テストとGUIビルドを確認しています。
Windows向けビルドとGUIは未確認です。Windowsでは更新後のrelease実行ファイルを直接起動し、
各ディスプレイの四隅、左・上の負座標のディスプレイ、表示倍率の異なるディスプレイへの
タブ切り出しを確認してください。大きなウィンドウから小さなディスプレイへ切り出す場合も、
タイトルバーと閉じるボタンがタスクバーを除く画面内に残ることを確認します。
