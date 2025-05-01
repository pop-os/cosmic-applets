power = 電源
settings = 設定...
lock-screen = 鎖定螢幕
lock-screen-shortcut = Super + Escape
log-out = 登出
suspend = 暫停
restart = 重新啟動
shutdown = 關機
confirm = 確認
cancel = 取消
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] 關閉所有應用程式並 { log-out }
        *[other] 立即執行選定的操作
    } 嗎？
confirm-body = 
    系統將在 { $countdown } 秒後自動 { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] { lock-screen }
        [log-out] { log-out }
        *[other] 執行選定的操作
    }。
