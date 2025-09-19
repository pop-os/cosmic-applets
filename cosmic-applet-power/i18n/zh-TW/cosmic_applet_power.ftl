power = 電源
settings = 設定…
lock-screen = 鎖定螢幕
lock-screen-shortcut = Super + Escape
log-out = 登出
log-out-shortcut = Super + Shift + Escape
suspend = 睡眠
restart = 重新啟動
shutdown = 關閉電源
confirm = 確認
cancel = 取消
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] 關閉電源
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] 結束所有應用程式並登出
        *[other] 套用選定的操作
    } 嗎？
confirm-body = 
    系統將在 { $countdown } 秒後自動 { $action ->
        [restart] 重新啟動
        [suspend] 睡眠
        [shutdown] 關閉電源
        [lock-screen] 鎖定螢幕
        [log-out] 登出
        *[other] 套用選定的操作
    }。