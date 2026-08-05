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

confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] Slökkva
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-body = 
    Kerfið mun { $action ->
        [restart] endurræsa
        [suspend] fara í svefn
        [shutdown] slökkva á sér
        [lock-screen] læsa skjánum
        [log-out] skrá út
        *[other] framkvæma völdu aðgerðina
    } sjálfkrafa eftir { $countdown } sekúndur.
