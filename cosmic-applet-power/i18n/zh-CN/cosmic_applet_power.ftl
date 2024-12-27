power = 电源
settings = 设置...
lock-screen = 锁定屏幕
lock-screen-shortcut = Super + Escape
log-out = 注销
suspend = 中止
restart = 重启
shutdown = 关机
confirm = 确认
cancel = 取消
confirm-button = {
    $action -> 
        [restart] { 重启 }
        [suspend] { 中止 }
        [shutdown] 关机
        [log-out] { 注销 }
        *[other] { 确认 }
}
confirm-title = 
    确认要 { $action -> 
        [restart] { 重启 }
        [suspend] { 中止 }
        [shutdown] { 关机 }
        [log-out] 退出所有应用并注销
        *[other] 应用所选操作
    } 吗？
confirm-body = 
    系统将在 { $countdown } 秒内自动 { $action ->
        [restart] 重启
        [suspend] 中止
        [shutdown] 关机
        [lock-screen] 锁定屏幕
        [log-out] 注销
        *[other] 应用所选操作
    }

