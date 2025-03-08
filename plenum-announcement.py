#!/usr/bin/python3

import datetime
import json

# import pytz
from zoneinfo import ZoneInfo

import requests

# from icecream import ic

THURSDAY = 3
FRIDAY = 4


def even_week_no(today):
    return today.isocalendar()[1] % 2 == 0


def next_thursday(today):
    return today + datetime.timedelta(days=(THURSDAY - today.weekday()) % 7)


def next_friday(today):
    return today + datetime.timedelta(days=(FRIDAY - today.weekday()) % 7)


def send_reminder_email(today):
    # friday = next_friday(today)
    thursday = next_thursday(today)
    # TODO
    pass


def create_status_announcement(date):
    # tz = pytz.timezone("Europe/Berlin")
    # start = tz.localize(datetime.datetime.combine(date, datetime.time(20, 0)))
    start = datetime.datetime.combine(
        date, datetime.time(20, 0), tzinfo=ZoneInfo("Europe/Berlin")
    )
    print(start)
    end = start + datetime.timedelta(hours=1)
    week_date = date.isocalendar()[1]
    j = {
        "type": "announcement",
        "method": "new",
        "from": start.timestamp(),
        "to": end.timestamp(),
        "note": f"Plenum KW{week_date:>02}",
        "user": "plenumsbot",
        "public": False,
        "url": "https://pads-intern.aachen.ccc.de/p/Plenum",
    }
    res = requests.put(
        "https://status.aachen.ccc.de/api/v0",
        data=json.dumps(j),
        cookies={
            "clubstatusd-password": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        },
    )
    print(res.status_code, res.text)


def main():
    today = datetime.date.today()
    # 1 = Tuesday
    # 3 = Thursday
    if today.weekday() in {1, 3} and even_week_no(today):
        send_reminder_email(today)

    if today.weekday() == THURSDAY and even_week_no(today):
        # create announcement for next Plenum
        next_plenum = today + datetime.timedelta(days=14)
        while not even_week_no(next_plenum):
            # this is only relevant for years with 53 weeks
            next_plenum += datetime.timedelta(days=7)
        create_status_announcement(next_plenum)


if __name__ == "__main__":
    main()
