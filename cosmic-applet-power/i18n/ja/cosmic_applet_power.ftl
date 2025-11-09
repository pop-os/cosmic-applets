power = 電源
settings = 設定...
lock-screen = 画面をロック
lock-screen-shortcut = スーパー + エスケープ
log-out = ログアウト
suspend = サスペンド
restart = 再起動
shutdown = シャットダウン
confirm = 確認する
cancel = キャンセル
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] 電源オフ
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] 全てのアプリケーションを閉じてログアウト
       *[other] 選択した処理を実行
    }しますか？
confirm-body =
    システムは{ $countdown }秒後に{ $action ->
        [restart] 再起動
        [suspend] サスペンド
        [shutdown] シャットダウン
        [lock-screen] 画面ロック
        [log-out] ログアウト
       *[other] 選択した処理を実行
    }します。
log-out-shortcut = Super + Shift + Escape
