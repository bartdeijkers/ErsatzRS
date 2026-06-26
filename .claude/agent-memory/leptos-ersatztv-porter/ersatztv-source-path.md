---
name: ErsatzTV source tree path
description: The ErsatzTV C# source tree is referenced through $ERSATZTV_SRC; agent shells may need that variable set before porting work.
type: reference
---

Use `$ERSATZTV_SRC` for the ErsatzTV checkout. If it is unset, ask the
maintainer to export it or point the shell at the repo-local documented
upstream checkout location.

Layout cheat-sheet for finding things fast:

- Razor pages: `$ERSATZTV_SRC/ErsatzTV/Pages/*.razor` (e.g. `FFmpeg.razor`, `FFmpegEditor.razor`).
- Razor shared dialogs: `$ERSATZTV_SRC/ErsatzTV/Shared/*.razor` (e.g. `CopyFFmpegProfileDialog.razor`, `DeleteDialog.razor`).
- ViewModels (form-shape mirrors): `$ERSATZTV_SRC/ErsatzTV/ViewModels/`.
- Validators (FluentValidation): `$ERSATZTV_SRC/ErsatzTV/Validators/`.
- Domain enums and records: `$ERSATZTV_SRC/ErsatzTV.Core/Domain/` and `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/`.
- MediatR commands/queries/handlers: `$ERSATZTV_SRC/ErsatzTV.Application/<Feature>/{Commands,Queries}/`.
- EF migrations (do **not** port 1:1 — use as a column reference): `$ERSATZTV_SRC/ErsatzTV.Infrastructure.Sqlite/Migrations/` and `.MySql/Migrations/`.

Open `cd "$ERSATZTV_SRC"` and grep before reading any single file — many
features have multiple Razor + handler + validator pieces.
