# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
uses [Semantic Versioning](https://semver.org/).

Entries are written for users: what changed for them, not how. Lead each
bullet with the area in bold.

## [Unreleased]

### Added

- **Record:** events (sling sent, reply landed, file touched, pane moved,
  agent blocked, agent done) and blob ids have a versioned JSON format. A
  reader keeps events of kinds and fields it does not know, so records from
  newer versions stay readable.
- **App:** the groundwork for the canvas pane: an endless canvas you can pan and zoom (within limits) about any point, drawing only what is on screen and repainting only what changed. Not yet shown in the window.
- **Workspaces:** a workspace is defined by a `slinger-workspace.toml` file in its primary root: a name, its directories, an optional agent, its record tag and its default canvas. See `docs/workspace.md`.
- **Canvas:** the canvas model: pages of strokes, text, boxes and arrows, each with its author and time. An agent can change only the items it made, never your ink, and removing something already gone is not an error.
