cancel = Sefsex
confirm = Sentem
restart = Ales asekker
suspend = Ḥbes di leɛḍil
log-out = Ffeɣ
shutdown = Sexsi
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Sexsi
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Ffeɣ akk isnasen syen ffeɣ
       *[other] Snes tigawt yettwafernen
    } tura?
confirm-body =
    Anagraw ad { $action ->
        [restart] yales asekker
        [suspend] yeḥbes di leɛḍil
        [shutdown] yexsi
        [lock-screen] isekkeṛ agdil
        [log-out] yeffeɣ
       *[other] isnes tigawt yettwafernen
    } s wudem awurman deg { $countdown } n tsinin.
