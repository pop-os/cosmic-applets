power = Power
settings = Settings...
lock-screen = Lock Screen
lock-screen-shortcut = Super + Escape
log-out = Log Out
log-out-shortcut = Ctrl + Alt + Delete
suspend = Suspend
restart = Restart
shutdown = Shutdown
confirm = Confirm
cancel = Cancel
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Power off
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Quit all applications and log out
        *[other] Apply the selected action
    } now?
confirm-body = 
    The system will { $action ->
        [restart] restart
        [suspend] suspend
        [shutdown] power off
        [lock-screen] lock the screen
        [log-out] log out
        *[other] apply the selected action
    } automatically in { $countdown } seconds.

