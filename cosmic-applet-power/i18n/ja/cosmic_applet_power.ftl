power = 電源
settings = 設定...
lock-screen = 画面をロック
lock-screen-shortcut = スーパー + エスケープ
log-out = ログアウト
log-out-shortcut = コントロール + Alt + 削除
suspend = サスペンド
restart = 再起動
shutdown = シャットダウン
confirm = 確認
cancel = キャンセル
confirm-body = 
    { $countdown }秒後にシステムは自動的に{ $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] { lock-screen }
        [log-out] { log-out }
        *[other] 選択したことを
    }します。

