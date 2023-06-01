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
confirm-question = 
    Are you sure? { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [lock-screen] Locking the screen
        [log-out] Logging out
        *[other] The selected action
    } will continue in { $countdown } seconds.

