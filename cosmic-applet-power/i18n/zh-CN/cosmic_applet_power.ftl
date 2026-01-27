power = 电源
settings = 设置...
lock-screen = 锁定屏幕
lock-screen-shortcut = Super + Escape
log-out = 登出
log-out-shortcut = Super + Shift + Escape
suspend = 待机
restart = 重启
shutdown = 关机
confirm = 确认
cancel = 取消
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] 关机
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    立即{ $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] 退出所有应用并登出
       *[other] 执行所选操作
    }？
confirm-body =
    系统将在 { $countdown } 秒后自动{ $action ->
        [restart] 重启
        [suspend] 待机
        [shutdown] 关机
        [lock-screen] 锁定屏幕
        [log-out] 登出
       *[other] 执行所选操作
    }。
