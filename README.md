
# Description
Implements a status API for hackerspaces. Most actions require authentication (HTTP Auth with a common password). Also supports announcements (for events or people announcing their future stay) and presence (people currently staying).

What data the daemon tracks and how the API looks is documented in the [Specification](api-specification.md).

# Build
Build dependencies: Rust and Cargo

To build, run `cargo build --release`
