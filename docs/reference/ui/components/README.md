# Admin UI component catalog

The catalog records reusable admin UI contracts and their actual repository
status. Component names describe semantic behavior, not only appearance.

## Status vocabulary

| Status | Meaning |
|---|---|
| Candidate | Repeated evidence exists, but the shared semantic and interaction contract is not settled or implemented |
| Implemented | A shared Rust/CSS surface exists; this does not imply broad adoption or complete accessibility evidence |
| Deprecated | A surface remains in source for migration or compatibility but must not be selected for new work |

Adoption is recorded separately as isolated, limited, or broad. Known gaps are
part of the contract until source and focused evidence close them.

## Catalog

| Component | Status | Adoption | Primary use |
|---|---|---|---|
| [Button and action link](button.md) | Implemented | Limited | Actions, navigation styled as an action, and icon-only actions |
| [Dialog](dialog.md) | Implemented | Broad | Modal decisions and delete confirmation |
| [Data table](data-table.md) | Implemented | Limited | Sortable, server-paged tabular data |
| [Pagination](pagination.md) | Implemented | Broad across `/media/*` | Range reporting, page size, and previous/next navigation |
| [Resource state](resource-state.md) | Implemented | Broad | Stable loading, ready, empty, and failed resource markup |
| [Navigation drawer](navigation-drawer.md) | Implemented | Global shell | Responsive primary admin navigation with keyboard-safe disclosure |
