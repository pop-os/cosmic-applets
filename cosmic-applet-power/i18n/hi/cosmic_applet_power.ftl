power = पावर
settings = सेटिंग्स...
lock-screen = लॉक स्क्रीन
lock-screen-shortcut = सुपर + एस्केप
log-out = लॉग आउट
log-out-shortcut = Ctrl + Alt + Delete
suspend = निलंबित करें
restart = पुनः आरंभ करें
shutdown = बंद करें
confirm = पुष्टि करें
cancel = रद्द करें
confirm-button = {
    $action -> 
        [restart] { restart }
        [suspend] { suspend}
        [shutdown] बंद करें
        [log-out] { log-out }
        *[other] { confirm}
}
confirm-title = 
    { $action -> 
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] सभी अनुप्रयोगों को बंद करें और लॉग आउट करें
        *[other] चयनित क्रिया लागू करें
    } अब?
confirm-body = 
    प्रणाली { $action ->
        [restart] पुनः आरंभ करेगी
        [suspend] निलंबित करेगी
        [shutdown] पावर ऑफ करेगी
        [lock-screen] स्क्रीन लॉक करेगी
        [log-out] लॉग आउट करेगी
        *[other] चयनित क्रिया लागू करेगी
    } स्वचालित रूप से { $countdown } सेकंड में.
