cancel = Hætta við
log-out = Skrá út
suspend = Svæfa
restart = Endurræsa
shutdown = Slökkva
confirm = Staðfesta
power = Orka
settings = Stillingar...
lock-screen = Læsa skjá
lock-screen-shortcut = Super + Esc-lykill
log-out-shortcut = Super + Shift + Esc
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Loka öllum forritum og skrá út
       *[other] Virkja valda aðgerð
    } núna?
confirm-body =
    Nú mun tölvan{ $action ->
        [restart] endurræsast
        [suspend] sofna
        [shutdown] slökkva á sér
        [lock-screen] læsa skjánum
        [log-out] skrá út
       *[other] virkja völdu aðgerðina
    } slökkva á sér sjálfkrafa eftir { $countdown } sekúndur.
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Slökkvar
        [log-out] { log-out }
       *[other] { confirm }
    }
