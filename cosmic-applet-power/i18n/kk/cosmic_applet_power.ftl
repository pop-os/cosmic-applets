cancel = Бас тарту
confirm = Растау
log-out = Жүйеден шығу
suspend = Ұйықтату
restart = Қайта іске қосу
shutdown = Сөндіру
power = Қуат
settings = Баптаулар...
lock-screen = Экранды бұғаттау
lock-screen-shortcut = Super + Escape
log-out-shortcut = Super + Shift + Escape
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Сөндіру
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Барлық қолданбаларды жауып, жүйеден шығу
       *[other] Таңдалған әрекетті іске асыру
    } қазір ме?
confirm-body =
    Жүйе { $action ->
        [restart] қайта іске қосылады
        [suspend] ұйықтатылады
        [shutdown] сөндіріледі
        [lock-screen] экранды бұғаттайды
        [log-out] жүйеден шығады
       *[other] таңдалған әрекетті іске асырады
    } { $countdown } секундтан кейін автоматты түрде.
