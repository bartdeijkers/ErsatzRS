# Teletext admin UX conventions

This reference keeps the channel teletext page (`/channels/{id}/teletext`) and
the teletext element library (`/teletext-elements`) consistent. Shared markup
lives in `crates/ersatzrs-web/src/admin/ui/primitives/teletext.rs`.

## Vocabulary

Use these terms in every culture's translation. Technical terms (revision,
digest, subcode, X/26, JSON, producer) appear only inside an **Advanced**
disclosure.

| UI term (en-US) | UI term (nl) | Meaning | Avoid in the main flow |
| --- | --- | --- | --- |
| Teletext | Teletext | The teletext service of a channel | — |
| Page set | Paginaset | Every page a channel broadcasts, with number and source | service, composition |
| Source | Bron | Where pages come from: an add-on or custom pages | producer |
| Add-on | Add-on | A teletext add-on such as the TV-guide add-on | package |
| Custom page | Eigen pagina | A teletext element linked to a channel | static page |
| Teletext element | Teletext element | An uploaded page design in the library | template, design |
| Page number | Paginanummer | 100–899 | address, owner |
| Subpage | Subpagina | One step of a rotating page (1/3, 2/3) | subcode |
| Settings | Instellingen | A source's fields for this channel | profile |
| Check | Controleren | Validate and save a draft; never changes the broadcast | — |
| Preview | Voorbeeld | The page as a receiver would show it | raster |
| Publish | Publiceren | Explicitly make the checked draft live | activate |
| Live | Live | Published and sent by at least one active output | delivering |
| Advanced | Geavanceerd | Disclosure for technical and Level 2.5 fields | expert |
| Experimental | Experimenteel | Label for Level 2.5 functionality | beta |

Write plain, second-person sentences under 20 words. Give each island one
sentence that says what it does.

## Page anatomy

1. Header: back link, `h1`, subtitle (channel or element name).
2. Status line directly under the header: one sentence, details collapsible.
3. Feedback message after a redirect (`role="status"` or `role="alert"`).
4. Islands in a fixed order: overview, sources, publish.
5. Advanced controls stay inside the island they belong to, as a closed
   `<details>` at its end.

## Islands

- One `island()` per source or task: title row with an optional status badge,
  one help sentence, grouped fields (`h3` group headings), one primary action.
- A disabled, missing or incompatible add-on shows no fields; it shows one
  sentence and a link that fixes the cause.
- Forms stay native `method="post"` forms that redirect with `saved=` or
  `error=` codes, so every action works without JavaScript.

## Status badges

| Tone | Use |
| --- | --- |
| Success | Live, on, linked |
| Warning | User action needed, nothing broken (unpublished changes) |
| Error | Blocks publication (conflict, invalid settings); always with a reason |
| Neutral | Off, not used |

A badge always carries a word; colour is never the only signal.

## Publication

Every page that changes teletext uses the same step bar:
**Check → Preview → Publish**. Saving settings never publishes.

## Preview frame

`preview_frame()` draws the 4:3 black screen, caption (page and subpage),
subpage navigation and a footer slot for diagnostics. Renderers put their own
output (the native raster image or an HTML cell grid) in the content slot. Do
not build a second preview frame.

## Empty states

Title (what is missing), one sentence (why it helps) and one primary action.
