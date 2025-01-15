![Build status badge](https://img.shields.io/gitlab/pipeline/clubstatus/clubstatusd.svg?gitlab_url=https%3A%2F%2Fgitlab.aachen.ccc.de)
![Maintenance status badge](https://img.shields.io/maintenance/yes/2025.svg)

# Description
Implements a status API for hackerspaces. Most actions require authentication
(HTTP Auth with a common password). Also supports announcements (for events or
people announcing their future stay) and presence (people currently staying).

What data the daemon tracks and how the API looks is documented in the [Specification](api-specification.md).

## Integrations
* Publish status and presence changes via MQTT
* Provide a [SpaceAPI](https://spaceapi.io/) 0.13 compatible endpoint at
  `/spaceapi` if configured.

# Dependencies
* Rust and Cargo
* GCC
* OpenSSL and SQLite3 (with headers)
on Debian: `apt-get install gcc openssl libssl-dev sqlite3-0 sqlite3-dev`, use
binary installer on https://www.rust-lang.org/downloads.html

# Build
Build dependencies: Rust and Cargo

To build, run `cargo build --release`

# API examples
## Create announcement
```sh
jq --null-input '{type: "announcement", method: "new", from: 1610612736, to: 1610612737, note: "2^29 * 3", user: "Hans", public: false}' \
  | curl http://localhost:8000/api/v0 -X PUT --data @- -v
```
