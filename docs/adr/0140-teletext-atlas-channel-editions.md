# ADR 0140: Channel teletext page assignments

## Status

Proposed

## Implementation

Implemented

## Context

A channel operator chooses a page number, template, source, subpages and FastText destinations together. These choices need to survive guide refresh and retain the previous published edition while the next edition is edited.

## Decision

Atlas keeps channel-owned page settings separate from shared library templates. The host captures guide assignments in the exact version 3 producer request. Refresh reuses that captured layout with fresh SQL listings. The guide add-on owns the rendering of the index and programme listings. The host resolves links to independently owned pages after validating and composing their source documents.

Library page instances inherit the uploaded element and may override its subpage selection, timing and field values. A single channel save prepares and validates the candidate, requests publication and selects it using the pending and accepted generation observed by the editor. The host records the Atlas revision against the immutable design. SQLite checks that revision and installs the guide binding, generation pointer and edition fingerprint together. A failed check can retain a working draft while output recovery reads the previously committed binding. Switching off every source removes the current pointers after checking both observed IDs and retains publication history.

An Atlas guide preview may invoke the installed, enabled TV-guide package before the channel has a guide binding. The preview reads that channel's configured profile or the installation defaults, checks the installation and settings again before launch, and uses the existing bounded worker and SQL guide capture. Its result is a picture. Only the channel save can create a binding or select a generation.

The index request includes the numbers and titles of channel library pages as destinations. The add-on includes these entries in its sorted index while the library producer retains ownership of their page content.

Channel settings own the teletext output switch. The Channels list opens Atlas when the switch is enabled. Turning it off preserves drafts and accepted generations and suppresses authored output. Live output policies observe changes on their next poll. The version 1 policy applies the choice when a receiver tunes in. Source subtitle selection remains independent.

## Consequences

Historical documents and guide requests without Atlas fields retain their original encoding. A shared guide theme applies to a channel on its next save. The accepted generation identifies the selected edition. Output consumers report application separately.

## References

- [Locked Atlas design](../../plans/teletext-redesign/README.md)
- [Teletext add-on contract](0121-teletext-pages-transport-and-addon-contract.md)
