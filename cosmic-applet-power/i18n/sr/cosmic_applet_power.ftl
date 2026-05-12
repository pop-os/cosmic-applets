power = Напајање
lock-screen = Закључај екран
shutdown = Угаси
log-out = Одјава
restart = Поново покрени
log-out-shortcut = Супер + Shift + Esc
cancel = Откажи
suspend = Обустави
confirm = Потврди
settings = Подешавања...
lock-screen-shortcut = Супер + Esc
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Искључи
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-body =
    Систем ће { $action ->
        [restart] поново покренути
        [suspend] обуставити
        [shutdown] искључити
        [lock-screen] закључати екран
        [log-out] одјавити се
       *[other] применити изабрану радњу
    } самостално за { $countdown } секунде.
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Затвори све програме и одјави се
       *[other] Примени изабрану радњу
    } сада?
