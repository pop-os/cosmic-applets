confirm-button =
    { $action ->
        [restart] { restart }
        [suspend] { suspend }
        [shutdown] כיבוי
        [log-out] { log-out }
       *[other] { confirm }
    }
