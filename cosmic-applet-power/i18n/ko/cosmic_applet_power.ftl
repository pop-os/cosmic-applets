power = 전원
settings = 설정...
lock-screen = 화면 잠그기
lock-screen-shortcut = Super + Escape
log-out = 로그아웃
suspend = 절전
restart = 재시작
shutdown = 종료
confirm = 확인
cancel = 취소
confirm-body =
    { $countdown }초 후 { $action ->
        [restart] 재시작이
        [suspend] 절전이
        [shutdown] 종료가
        [lock-screen] 화면 잠금이
        [log-out] 로그아웃이
       *[other] 선택한 동작이
    } 자동으로 실행됩니다.
confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] 종료
        [log-out] { log-out }
       *[other] { confirm }
    }
log-out-shortcut = Super + Shift + Escape
confirm-title =
    지금 { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] { shutdown }
        [log-out] 모든 앱 종료 후 로그아웃
       *[other] 선택된 동작을 실행
    }할까요?
