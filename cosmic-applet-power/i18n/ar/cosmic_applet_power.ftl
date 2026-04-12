power = الطاقة
settings = الإعدادات...
lock-screen = قفل الشاشة
lock-screen-shortcut = سوبر + Escape
log-out = سجِّل الخروج
log-out-shortcut = Super + Shift + Escape
suspend = علِّق
restart = أعد التشغيل
shutdown = أطفئ
confirm = أكِّد
cancel = ألغِ
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] أطفئ
        [log-out] { log-out }
       *[other] { confirm }
    }
confirm-title =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] إغلاق جميع التطبيقات وتسجيل الخروج
       *[other] تنفيذ الإجراء المحدد
    } الآن؟
confirm-body =
    سيقوم النظام بـ { $action ->
        [restart] إعادة التشغيل
        [suspend] تعليق
        [shutdown] إطفاء
        [lock-screen] قفل الشاشة
        [log-out] تسجيل الخروج
       *[other] تطبيق الإجراء المحدد
    } آليًا خلال { $countdown } ثانية.
