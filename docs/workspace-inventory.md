# Workspace inventory (starting audit — issue #911)

Partial start on the full audit this issue asks for. Verified against the
current tree (not the issue body's counts, which are stale — `fee` and
`notification` are already workspace members as of this check):

- Workspace currently declares 38 `contracts/*` members in root `Cargo.toml`.
- `contracts/wallet-status` has a real `Cargo.toml`/`src/` layout but is
  **not** a workspace member — genuinely orphaned, confirmed by diffing
  `contracts/*` directories against the `[workspace] members` list.

## Status key
- `in-workspace`: listed in root `Cargo.toml` members
- `orphaned-crate`: has Cargo.toml + src/, not listed
- `loose-file`: no crate structure

## Findings so far
| Path | Status |
|---|---|
| contracts/wallet-status | orphaned-crate |
| contracts/fee | in-workspace |
| contracts/notification | in-workspace |

Remaining work not done here: auditing the rest of the ~44 `contracts/*`
directories one by one, adding orphaned crates to `[workspace] members`,
resolving any collisions that surfaces, and updating CI to run
`--workspace`. This file is a starting point, not the full inventory the
issue asks for.
