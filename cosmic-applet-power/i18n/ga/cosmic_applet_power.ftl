power = Cumhacht
settings = Socruithe...
lock-screen = Glas an Scáileán
lock-screen-shortcut = Super + Éalú
log-out = Logáil Amach
log-out-shortcut = Super + Shift + Éalú
suspend = Cuir ar Fionraí
restart = Atosaigh
shutdown = Múch
confirm = Deimhnigh
cancel = Cealaigh
confirm-button = {
    $action -> 
        [restart] { atosaigh }
        [suspend] { cuir ar fionraí }
        [shutdown] Múch an córas
        [log-out] { logáil amach }
        *[other] { deimhnigh }
}
confirm-title = 
    { $action ->
        [restart] { atosaigh }
        [suspend] { cuir ar fionraí }
        [shutdown] { múch }
        [log-out] Dún gach aip agus logáil amach
        *[other] Cuir an gníomh roghnaithe i bhfeidhm
    } anois?
confirm-body =
    Déanfaidh an córas { $action ->
        [restart] atosaigh
        [suspend] cur ar fionraí
        [shutdown] múchadh
        [lock-screen] glasáil an scáileán
        [log-out] logáil amach
        *[other] an gníomh roghnaithe a chur i bhfeidhm
    } go huathoibríoch i gceann { $countdown } soicind.
