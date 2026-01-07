power = Alimentation
settings = Paramètres...
lock-screen = Verrouiller la session
lock-screen-shortcut = Super + Échap
log-out = Se déconnecter
suspend = Veille
restart = Redémarrer
shutdown = Éteindre
confirm = Confirmer
cancel = Annuler
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Éteindre
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Quitter toutes les applications et se déconnecter
       *[other] Appliquer l'option choisie
    } maintenant ?
confirm-body =
    Cet ordinateur { $action ->
        [restart] redémarrera
        [suspend] se mettra en veille
        [shutdown] s'éteindra
        [lock-screen] se verrouillera
        [log-out] se déconnectera
       *[other] appliquera l'option choisie
    } automatiquement dans { $countdown } secondes.
log-out-shortcut = Super + Maj + Échap
