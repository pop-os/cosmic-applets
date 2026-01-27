power = Живлення
settings = Налаштування...
lock-screen = Заблокувати екран
lock-screen-shortcut = Super + Escape
log-out = Вийти
suspend = Призупинити
restart = Перезавантажити
shutdown = Вимкнути
confirm = Підтвердити
cancel = Скасувати
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Вимкнути
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Вийти з усіх застосунків та завершити сеанс
       *[other] Виконати обрану дію
    } зараз?
confirm-body =
    Система { $action ->
        [restart] перезавантажиться
        [suspend] призупинеться
        [shutdown] вимкнеться
        [lock-screen] заблокує екран
        [log-out] завершить сеанс
       *[other] виконає обрану дію
    } автоматично через { $countdown } секунд.
log-out-shortcut = Super + Shift + Escape
