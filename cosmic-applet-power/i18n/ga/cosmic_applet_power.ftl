power = Cumhacht
settings = Socruithe...
lock-screen = Glas an Scáileán
lock-screen-shortcut = Super + Éalú
log-out = Logáil Amach
log-out-shortcut = Super + Shift + Éalú
suspend = Cuir ar fionraí
restart = Atosaigh
shutdown = Múchadh
confirm = Deimhnigh
cancel = Cealaigh
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Múch an córas
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Dún gach aipeanna agus logáil amach
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
    } go huathoibríoch i gceann { $countdown } soicindí.
