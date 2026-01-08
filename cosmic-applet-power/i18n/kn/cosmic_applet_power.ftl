power = ಪವರ್
settings = ಸೆಟ್ಟಿಂಗ್‌ಗಳು...
lock-screen = ಸ್ಕ್ರೀನ್ ಲಾಕ್ ಮಾಡಿ
lock-screen-shortcut = Super + Escape
log-out = ಲಾಗ್ ಔಟ್
log-out-shortcut = Super + Shift + Escape
suspend = ಸಸ್ಪೆಂಡ್
restart = ರೀಸ್ಟಾರ್ಟ್
shutdown = ಶಟ್‌ಡೌನ್
confirm = ದೃಢಪಡಿಸಿ
cancel = ರದ್ದುಮಾಡಿ
confirm-button = {
    $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] ಪವರ್ ಆಫ್
        [log-out] { log-out }
        *[other] { confirm }
}
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] ಎಲ್ಲಾ ಅಪ್ಲಿಕೇಶನ್‌ಗಳನ್ನು ಮುಚ್ಚಿ ಲಾಗ್ ಔಟ್ ಮಾಡಿ
        *[other] ಆಯ್ಕೆಮಾಡಿದ ಕ್ರಿಯೆಯನ್ನು ಅನ್ವಯಿಸಿ
    } ಈಗ?
confirm-body =
    ಸಿಸ್ಟಮ್ { $action ->
        [restart] ರೀಸ್ಟಾರ್ಟ್ ಮಾಡುತ್ತದೆ
        [suspend] ಸಸ್ಪೆಂಡ್ ಮಾಡುತ್ತದೆ
        [shutdown] ಪವರ್ ಆಫ್ ಮಾಡುತ್ತದೆ
        [lock-screen] ಸ್ಕ್ರೀನ್ ಲಾಕ್ ಮಾಡುತ್ತದೆ
        [log-out] ಲಾಗ್ ಔಟ್ ಮಾಡುತ್ತದೆ
        *[other] ಆಯ್ಕೆಮಾಡಿದ ಕ್ರಿಯೆಯನ್ನು ಅನ್ವಯಿಸುತ್ತದೆ
    } ಸ್ವಯಂಚಾಲಿತವಾಗಿ { $countdown } ಸೆಕೆಂಡುಗಳಲ್ಲಿ
