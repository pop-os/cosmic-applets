power = 电源
settings = 设置...
lock-screen = 锁定屏幕
lock-screen-shortcut = Super + Escape
log-out = 登出
log-out-shortcut = Super + Shift + Escape
suspend = 挂起
restart = 重启
shutdown = 关机
confirm = 确认
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
    确认要 { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] 退出所有应用并 { log-out }
        *[other] 应用所选操作
    } 吗？
confirm-body = 
    系统将在 { $countdown } 秒内自动 { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] { lock-screen }
        [log-out] { log-out }
        *[other] 应用所选操作
    } 。

