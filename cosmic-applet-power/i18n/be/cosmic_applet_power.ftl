power = Сілкаванне
settings = Налады...
lock-screen = Экран блакіроўкі
lock-screen-shortcut = Super + Escape
log-out = Выйсці
log-out-shortcut = Ctrl + Alt + Delete
suspend = Прыпыніць
restart = Перазапусціць
shutdown = Выключэнне
confirm = Пацвердзіць
cancel = Скасаваць
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Выключыць
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Закрыць усе праграмы і выйсці
       *[other] Прымяніць абранае дзеянне
    } зараз?
confirm-body =
    Сістэма { $action ->
        [restart] будзе перазапушчана
        [suspend] будзе прыпынена
        [shutdown] будзе выключана
        [lock-screen] заблакуе экран
        [log-out] выканае выхад
       *[other] прыменіць абранае дзеянне
    } аўтаматычна праз { $countdown } секунд.
