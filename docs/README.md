# Glim documentation

**English** · [简体中文](README.zh-CN.md) · [Product home](../README.md) · [Download](https://github.com/glimapp/glim/releases/latest)

Glim helps you view files for work, study and projects on a Mac, with basic text editing. Find your task first, then learn the controls you need.

## Use the app

| Your question | Start here |
| --- | --- |
| What can I use it for? | [Office, reading, media, data and specialist topics](topics.md) |
| How do I start? | [Files, workspaces, reading and saving](user-guide.md) |
| Which installer and how do I open it? | [Download and installation](../README.md#download) |
| Can I edit, and why does a preview differ? | [Common questions](faq.md) |
| What does a particular format support? | [Formats and compatibility](file-format-support.md) |
| How do updates and restart recovery work? | [Updates and recovery](updates.md) |
| Are files uploaded, and what uses the network? | [Privacy and network access](privacy.md) |
| What still needs work? | [Current coverage and remaining work](TODO.md) |
| Accessibility or a security concern? | [Accessibility](../ACCESSIBILITY.md) · [Private vulnerability reports](../SECURITY.md) |

## Contribute and maintain

You do not need implementation documents to use Glim. Start with [Contributing](../CONTRIBUTING.md) to report a problem or change code.

| Maintainer reference | Contents |
| --- | --- |
| [Architecture](architecture.md) | Modules, workspaces, file loading and readers |
| [Reader module contracts](reader-modules.md) | Extension points, ownership, task admission and remaining boundaries |
| [Build and release](releasing.md) | Toolchains, packaging, signing and both Mac architectures |
| [Update internals](auto-update-design.md) | Trust chain, install protection and recovery |
| [Telemetry reference](telemetry.md) | Events, metrics and interpretation limits |
| [Reader policy inventory](reader-limits.md) | Generated paging and resource-budget reference |
| [Resources and rendering](reader-resources.md) | Accounting, eviction, viewport scheduling and remaining coverage |
| [Format catalog](format-catalog.md) | Generated format, icon and association declarations |

Documentation describes current source. Consult [Release notes](https://github.com/glimapp/glim/releases) for a published build.
