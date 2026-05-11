cancel = Ακύρωση
shutdown = Τερματισμός
log-out = Αποσύνδεση
restart = Επανεκκίνηση
suspend = Αναστολή
confirm = Επιβεβαίωση
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] Τερματισμός
        [log-out] { log-out }
       *[other] { confirm }
    }
power = Ενέργεια
confirm-body =
    Θα εκτελεστεί αυτόματα { $action ->
        [restart] επανεκκίνηση
        [suspend] αναστολή
        [shutdown] τερματισμός
        [lock-screen] κλείδωμα της οθόνης
        [log-out] αποσύνδεση
       *[other] η επιλεγμένη ενέργεια
    } του συστήματος σε { $countdown } δευτερόλεπτα.
lock-screen = Κλείδωμα οθόνης
log-out-shortcut = Super + Shift + Escape
settings = Ρυθμίσεις...
lock-screen-shortcut = Super + Escape
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] Έξοδος από όλες τις εφαρμογές και αποσύνδεση
       *[other] Εφαρμογή της επιλεγμένης ενέργειας
    } τώρα;
