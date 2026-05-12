---
name: ErsatzTV source tree path
description: The ErsatzTV C# source tree that PORTING.md references via $ERSATZTV_SRC lives at /home/bart/ErsatzTV on this machine; $ERSATZTV_SRC may be unset in agent shells.
type: reference
---

`$ERSATZTV_SRC` is **not exported** in the agent shell — `printenv ERSATZTV_SRC` returns empty. The actual ErsatzTV checkout lives at `/home/bart/ErsatzTV/`.

Layout cheat-sheet for finding things fast:

- Razor pages: `/home/bart/ErsatzTV/ErsatzTV/Pages/*.razor` (e.g. `FFmpeg.razor`, `FFmpegEditor.razor`).
- Razor shared dialogs: `/home/bart/ErsatzTV/ErsatzTV/Shared/*.razor` (e.g. `CopyFFmpegProfileDialog.razor`, `DeleteDialog.razor`).
- ViewModels (form-shape mirrors): `/home/bart/ErsatzTV/ErsatzTV/ViewModels/`.
- Validators (FluentValidation): `/home/bart/ErsatzTV/ErsatzTV/Validators/`.
- Domain enums and records: `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/` and `/home/bart/ErsatzTV/ErsatzTV.Core/FFmpeg/`.
- MediatR commands/queries/handlers: `/home/bart/ErsatzTV/ErsatzTV.Application/<Feature>/{Commands,Queries}/`.
- EF migrations (do **not** port 1:1 — use as a column reference): `/home/bart/ErsatzTV/ErsatzTV.Infrastructure.Sqlite/Migrations/` and `.MySql/Migrations/`.

Open `cd /home/bart/ErsatzTV` and grep before reading any single file — many features have multiple Razor + handler + validator pieces.
