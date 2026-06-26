cancel = Têk bibe
confirm = Bipejirîne
power = Hêz
restart = Ji nû ve bide destpêkirin
settings = Sazkarî...
shutdown = Vemrîne
suspend = Rawestîne
log-out = Derkeve
lock-screen = Dîmenderê kilît bike
lock-screen-shortcut = Super + Escape
log-out-shortcut = Super + Shift + Escape
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Vemrîne
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Hemû sepanan bigire û derkeve
       *[other] Çalakiyê hilbijartî bisepîne
    } niha?
confirm-body =
    Pergal wê xweber were { $action ->
        [restart] jinûvedestpêkkirin
        [suspend] rawestandin
        [shutdown] vemirandin
        [lock-screen] kilîtkirin
        [log-out] derketin
       *[other] çalakiyê hilbijartî bisepîne
    }di { $countdown } çirke de.
