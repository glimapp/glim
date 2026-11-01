# Updates and recovery

**English** · [简体中文](updates.zh-CN.md) · [Documentation](README.md)

## How do I update?

Check for updates through the Glim menu or Settings. The check reports its status; an available update opens a window with release notes and actions. Follow recovery hints to retry an error, or download manually from [releases](https://github.com/glimapp/glim/releases/latest). Local builds without a configured updater offer manual downloads.

Modes include manual only, check and notify, and automatic updates. Automatic mode defaults to background download and verification, then installation on normal quit or an explicit restart, while respecting saved preferences. It does not force an interruption or enter administrator credentials for you.

## What happens to work I am editing?

Before installation, Glim checks registered workspaces for unsaved content, saves and file loads, prepares an application backup and records a one-time session checkpoint. Unfinished work or failed records block the install restart and report why. Save important files first.

The update restart can restore supported workspaces, file and playlist tabs, their order and preview modes. Media stays paused; general scroll positions and media time are not fully restored. The checkpoint does not contain unsaved text; drafts have separate recovery.

## What does the backup restore?

Before updating, Glim retains one verified application-bundle ZIP and checksum receipt in `~/Library/Application Support/Glim/UpdateBackups/`. This supports manual application recovery, not a full backup of your documents or preferences. A new-version crash does not trigger automatic health-based rollback.

## What if the update fails?

Offline checks, invalid signatures, damaged or incompatible packages prevent installation. Cancelled authorization, other running copies, read-only locations and permissions can also block replacement. Follow the actual error; disabling system security is unnecessary. Update signature checks differ from Apple notarization; follow [first-install instructions](../README.md#macos).

Maintainers can consult [update internals](auto-update-design.md) and the [release process](releasing.md) for signing, recovery and installation details.
