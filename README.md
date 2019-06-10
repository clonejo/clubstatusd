
# Description
Implements a status API for hackerspaces. Most actions require authentication
(HTTP Auth with a common password). Also supports announcements (for events or
people announcing their future stay) and presence (people currently staying).

What data the daemon tracks and how the API looks is documented in the [Specification](api-specification.md).

## Integrations
* Publish status and presence changes via MQTT
* Provide a [SpaceAPI](http://spaceapi.net/) 0.13 compatible endpoint at
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

# Dependency Graph
![DAG of dependency crates created using cargo-deps](dependencies.png)
