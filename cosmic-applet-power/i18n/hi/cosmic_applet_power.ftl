power = पावर
settings = सेटिंग्स...
lock-screen = स्क्रीन लॉक करें
lock-screen-shortcut = Super + Escape
log-out = लॉग आउट
log-out-shortcut = Super + Shift + Escape
suspend = सस्पेंड
restart = रिस्टार्ट
shutdown = शटडाउन
confirm = पुष्टि करें
cancel = रद्द करें
confirm-button = {
    $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] पावर ऑफ
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] सभी एप्लिकेशन बंद करें और लॉग आउट करें
        *[other] चयनित क्रिया लागू करें
    } अब?
confirm-body =
    सिस्टम { $action ->
        [restart] रिस्टार्ट करेगा
        [suspend] सस्पेंड करेगा
        [shutdown] पावर ऑफ करेगा
        [lock-screen] स्क्रीन लॉक करेगा
        [log-out] लॉग आउट करेगा
        *[other] चयनित क्रिया लागू करेगा
    } स्वतः { $countdown } सेकंड में
