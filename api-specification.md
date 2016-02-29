
## API Version
The API resides at `/api/`.  
The API is versioned using incrementing integers. Version 1 is at `/api/v1/`
while version 2 is at `/api/v2/`. A list of all supported versions can be found
at `/api/versions`.  
This file documents Version _0_, all paths below refer to `/api/v0/$PATH`.

### History
v0: initial version


## Actions

The server manages one global stream of actions. Every action stored contains
these members:

All actions share these members:
```js
{
    "id": 123,            // unique identifier, always increasing
    "time": 1234567890,   // UNIX timestamp, in seconds
    "type": "status",     // the action type
    "note": "some string" // not interpreted, maximum length of 80 bytes (UTF-8)
}
```

### Action types

* `status`: The status has been set to a new value.
* `announcement`: Someone announces to stay at the club for a certain time
                  range.
* `presence`: These people are at the club at the moment.

The examples below omit the standard members.


### Status actions

```js
{
    "user": "Hans Acker", // the user who changed the status (UTF-8,
                          // 1 to 15 bytes, enclosing whitespace is stripped)
    "status": "closed"    // What the status has been changed to.
                          // Possible values: "public", "private", "closed"
}
```


### Announcement actions

```js
{
    "method": "new",      // "new" | "mod" | "del"
    "aid": 42,            // the announcement to operate on. only needed when
                          // "action=mod" or "action=del"
    "user": "Hans Acker", // the user who announces to come (UTF-8,
                          // 1 to 15 bytes, enclosing whitespace is stripped)
    "from": 123456789,    // UNIX timestamp
    "to": 123456789,      // UNIX timestamp
    "public": true        // if the announcement can be seen without
                          // authenticating
}
```


### Presence actions

#### Client request:
```js
{
    "user": "Hans Acker"  // UTF-8, 1 to 15 bytes, enclosing whitespace is stripped
}
```

#### Action from server:
An action is generated every 10 minutes, but only if the list of present users
has changed.
```js
{
    "users": [                    // alphabetically sorted list of all present
                                  // users
        {
            "user": "Frank Nord",
            "since": 1234567890   // UNIX timestamp
        },
        {
            "user": "Hans Acker"
            "since": 1234567891
        }
    ]
}
```
The server will add a human readable note explaining the changes
(joins/leaves).

## Queries
By default, all actions are encoded (and expected to be encoded) in JSON. Other
encodings might be added in future. Whenever a list is returned, it is wrapped
in `{"actions": […]}`.  
Authenticate using HTTP Authentication, unless you use the public API. The
username is ignored.  
If you want to show relative times in your interface, use the server's time
rather than your own. The server's time can be out of sync.

### GET current status
`GET /status/current`
```js
{
    "last": {action},   // last status action
    "changed": {action} // last status action which changed the status
}
```

### GET current announcements
`GET /announcement/current`  
200 List of actions that have not yet ended. (I.e. also contains future
announcements.)


### Action select queries
The type `all` matches all action types. Ranges are interpreted inclusively.
All filters/options below can be combined.

#### id filter
`GET /{action_type}?id=`  
`GET /{action_type}?id={id}`  
`GET /{action_type}?id={id1}:{id2}`  
Queries actions by id (range). `id=` deactivates this filter.  
Special values: `last` and `last-{int}` (eg. `last-5`)  
Default: `id=`

#### time filter
`GET /{action_type}?time=`  
`GET /{action_type}?time={time}`  
`GET /{action_type}?time={time1}:{time2}`  
Queries actions by timestamp (range). `time=` deactivates this filter. Be aware
the timestamp of an action could be terribly wrong due to an unsynchronized
server clock.  
Special values: `now` and `now-{int}` (eg. `now-3600`)  
Default: `time=`

#### count option
`GET /{action_type}?count={int}`  
The number of actions to return.

#### take option
`GET /{action_type}?take=first`  
`GET /{action_type}?take=last`

### Streaming
The type `all` matches all action types.

`GET /{action_type}/stream?format={format}`  
Streams actions of `{action_type}` in realtime. Supported formats are `newline`
and `SSE`. `{format}` defaults to `newline`. For each action type, the last
action is send immediately on connecting.

### PUT Queries
`PUT /` with an action object as body  
Create a new action. The attributes `id` and `time` are ignored.

#### PUT Status
Mandatory members: `type`, `user`, `status`  
`public` defaults to `false`.  
The server returns the action id.

#### PUT Announcement action
The past can not be modified. But announcements that are currently running can
be extended or shortened (but `to` can't be moved into the past.)  
Special values for `from` and `to`: `now`, `now-{int}` (seconds), `now+{int}`
(seconds).

##### New announcement
Mandatory members: `type`, `user`, `from`, `to`  
200 the created announcement  
400 `from` > `to`  
403 you tried to modify the past

##### Modify announcement
Mandatory members: `type`, `aid`, `user`, `from`, `to`  
200 the updated announcement  
400 `from` > `to`  
403 you tried to modify the past  
404 unknown announcement id

#### Delete announcement
```js
{
    "action": "del"
    "aid": 1234
}
```
200  
403 you tried to modify the past  
404 unknown announcement id

#### PUT Presence
Mandatory members: `type`, `user`  
The presence times out after 15 minutes. The `note` attribute is ignored.  
Please make sure you do proper checking (eg. check if your device connected to
the club wifi/ethernet).


## Public API
The public API is mostly the authenticated API, but with strong restrictions:

* `id` and `user` are stripped from every action object.
* `note` is stripped from every action object, unless it is an announcement
  action with `public=true`.
* The status `private` is treated as `closed`.
* Requests for ids, id ranges and also for `last` are blocked with `401
  Unauthorized`.
* `/status/current` only returns the `changed` key, `last` is stripped.
* `/status/stream` only sends actions when the status actually changed between
  `public` and {`private`, `closed`}.
* `/announcements/stream` is blocked with `401 Unauthorized`.
* `/presence/stream` is blocked with `401 Unauthorized`.
* `/announcements/current` only lists announcements with `public=true`.
* all `PUT` requests are blocked with `401 Unauthorized`.
