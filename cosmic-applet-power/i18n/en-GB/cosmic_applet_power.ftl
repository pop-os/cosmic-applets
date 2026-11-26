confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Power off
        [log-out] { log-out }
       *[other] { confirm }
    }
power = Power
confirm-body =
    The system will { $action ->
        [restart] restart
        [suspend] suspend
        [shutdown] power off
        [lock-screen] lock the screen
        [log-out] log out
       *[other] apply the selected action
    } automatically in { $countdown } seconds.
lock-screen = Lock Screen
shutdown = Shutdown
log-out = Log Out
restart = Restart
log-out-shortcut = Super + Shift + Escape
cancel = Cancel
suspend = Suspend
confirm = Confirm
settings = Settings...
lock-screen-shortcut = Super + Escape
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Quit all applications and log out
       *[other] Apply the selected action
    } now?
