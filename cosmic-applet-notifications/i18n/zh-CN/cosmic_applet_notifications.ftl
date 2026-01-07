hours-ago =
    { $duration ->
        [0] 刚刚
        [one] 1 小时前
       *[other] { $duration } 小时前
    }
minutes-ago =
    { $duration ->
        [0] 刚刚
        [one] 1 分钟前
       *[other] { $duration } 分钟前
    }
show-less = 收起
show-more = 显示剩余 { $more } 项
clear-group = 清除群组
clear-all = 清除所有通知
do-not-disturb = 勿扰模式
notification-settings = 通知设置...
no-notifications = 无通知
