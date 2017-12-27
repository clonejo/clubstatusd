
# v0.2.5
- push last status action json to mqtt

# v0.2.4
- update rusqlite to 0.11.0 to fix pkg_build problem

# v0.2.3
- Do not store the password in the session cookie, instead derive a cookie
  value doing a salt. Before `ae1d5e9bb` the browser was sent the correct
  cookie (at some point the password!) even if the received cookie or password
  was wrong.  Clear the cookie if the received cookie was wrong. Changing the
  password is highly recommended.

# v0.2.2
- migrate from mqttc to rumqtt

# v0.2.1
- updated dependencies
- optimized release build
- tested with rust 1.12.0 and cargo 0.13.0

# v0.2.0
- MQTT support
- only created presence actions on changes
- most initial implementation was done here
