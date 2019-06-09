# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Changed
- License clubstatusd under Apache 2.0
- Dependency updates (config)

## v0.3.0 - 2019-06-10
### Added
- Implement SpaceAPI 0.13
- Changelog follows Keep a Changelog from now on

### Changed
- Converted example config to toml, old config format does not seem to work
  anymore.
- Dependency updates (chrono, clap, config, regex, rumqtt, rusqlite, sodiumoxide)

### Fixed
- Error out if configured MQTT broker is not reachable on startup of
  clubstatusd.

## v0.2.5 - 2017-05-27
- push last status action json to mqtt

## v0.2.4 - 2017-05-06
- update rusqlite to 0.11.0 to fix pkg_build problem

## v0.2.3 - 2017-02-25
### Security
- Do not store the password in the session cookie, instead derive a cookie
  value doing a salt. Before `ae1d5e9bb` the browser was sent the correct
  cookie (at some point the password!) even if the received cookie or password
  was wrong.  Clear the cookie if the received cookie was wrong. Changing the
  password is highly recommended.

## v0.2.2 - 2017-02-06
- migrate from mqttc to rumqtt

## v0.2.1 - 2016-08-23
- updated dependencies
- optimized release build
- tested with rust 1.12.0 and cargo 0.13.0

## v0.2.0 - 2016-06-11
- MQTT support
- only created presence actions on changes
- most initial implementation was done here
