power = Напајање
settings = Подешавања...
lock-screen = Закључавање
lock-screen-shortcut = Super + Escape
log-out = Од‌јављивање
suspend = Стање спавања
restart = Поново покрени
shutdown = Искључи
confirm = Потврди
cancel = Прекини
confirm-body =
    { $action ->
        [restart] Поновно покретање
        [suspend] Стање спавања
        [shutdown] Искључивање
        [lock-screen] Закључавање екрана
        [log-out] Од‌јављивање
       *[other] Изабрани поступак
    } ће почети за { $countdown } секунди.
log-out-shortcut = Super + Shift + Escape
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Power off
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Quit all applications and log out
       *[other] Apply the selected action
    } now?
