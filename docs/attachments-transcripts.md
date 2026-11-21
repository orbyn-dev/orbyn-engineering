# Photo conversion, transcript import and earlier messages

What the web composer and thread need from the service for three Beta frame groups. **Planned**: the web side is built hidden, the
service routes are not built. In a production build the web has no client for any of them and sends nothing; a dev-only preview
(`#/preview/attachments`, `#/preview/earlier`) installs the mocks in `web/mock/attachments.ts` and `web/mock/earlier.ts`. A name here
changes only if the service pull request says so. Rows: B8, B43, K36 in [parity](parity.md).

| Group | Frames | Owner of the service side |
| --- | --- | --- |
| Photo conversion | `pc-b8-converting`, `-done`, `-failed`, `-narrow` | Eng 2 (Core upload path) |
| Transcript import and export | `pc-b43-drag-over`, `-dropped`, `-dropped-many`, `-rejected`, `-narrow` | Eng 3 (export and transcript, per Products Research) |
| Earlier messages of an imported thread | `pc-k36-earlier`, `pc-k36-unreadable` | Eng 2 and Eng 4 (read the provider's history) |

Photo conversion and the earlier-messages read are not Person-class: each is a write or read on the thread's own files and history, like
the attachment upload that exists today (`POST /api/threads/{id}/attachments` is not in `PERSON_ROUTES`). **Transcript import is
Person-class** (Products Research ruling): it creates thread context from an untrusted file, so the page sends the person key with it,
and it is listed in `PENDING_PERSON_ROUTES` in `web/src/api/personKey.ts` until `routes.json` lists it (then move it into `PERSON_ROUTES`).
Transcript export stays Local.

Owners (Products Research): transcript export and import: Eng 3, after W8/W9, including `source` and `imported_at` on the thread;
photo conversion: Luan, as an issue, reviewed by Eng 2; the earlier-messages read: Eng 2's R list, after HostDriver. The "Reading
transcript…" chip reuses the converting chip (no new frame).

## Shared rules

- Every route is under the thread: the id in the path is a thread the person can open. A path or file name sent by the client is a
  label only. The service never reads, writes or joins a path from client text; the bytes go to the data directory under a
  service-made id, as attachments do now.
- The body is the raw file (`Content-Type` as the browser gave it), the name is `?name=`, read as a display label and truncated to 255
  characters. The service sniffs the real type from the bytes and ignores the claimed one when they differ.
- The web draws every name, title and message it gets as text, never as markup or a link, and never opens a path from it. The service
  must still treat file content as untrusted: parse with size and depth limits, never run or render it, never put it in a log line.
- Bodies over the cap are refused before they are read in full (`Content-Length`, then a counting reader).

## Photo conversion (B8)

`POST /api/threads/{id}/attachments/convert?name=<label>` with the photo as the body.

- Allowed input types: `image/heic`, `image/heif`, `image/png`, `image/jpeg`, `image/webp`. Anything else is `415 unsupported_type`
  and the web does not send it here (it goes to the ordinary upload). A GIF is never shrunk, since that would drop its frames.
- Behaviour: HEIC and HEIF become JPEG; PNG, JPEG and WebP over 10 MiB are made smaller until they are at most 10 MiB. A photo
  already within the limit and not HEIC is stored as it is and answers `action: "none"` (the web only sends photos that need work).
  The stored file is the converted one; the original is not kept and the chip never offers it back.
- Cap on the input: 50 MiB (`413 file_too_large`) and a pixel cap of 100 megapixels, so a small file cannot expand into memory.
  The decoder runs with a time limit and no network access; metadata that locates a person (GPS) is dropped from the result.
- Answer `200`:

  ```json
  { "id": "…", "name": "IMG_4021.jpg", "size": 2202009, "original_name": "IMG_4021.HEIC", "original_size": 3000000, "action": "converted" }
  ```

  `action` is `converted`, `shrunk` or `none`. `id` goes in the run's `attachment_ids`, as an uploaded file's does. `name` is the new
  name with the new ending; the web shows it.
- Errors, each `{code, message}` as the other routes: `unreadable_image` (`422`, damaged or not an image), `still_too_large` (`422`,
  still over 10 MiB after shrinking), `file_too_large` (`413`), `unsupported_type` (`415`). The web shows its own sentence for the
  first two and the service's message for the rest.
- The request can be cancelled by the client closing it; the service then keeps nothing.

## Transcript import (B43)

`POST /api/threads/{id}/transcripts?name=<label>` with the transcript file as the body.

- The file is the one Core's export writes. **Not built**: the ending (`.transcript`) and type (`application/x-orbyn-transcript`) in the
  web are placeholders until the export exists; the owner fixes the real ones and the two constants in
  `web/src/features/thread/composer/transcriptDrop.ts` follow. The web recognises a transcript by that ending only to choose this route;
  the service decides from the bytes.
- Person-class. Transcript files are untrusted: the size cap, the parse limits and "nothing executes" below are security-queue checks.
- Cap: 50 MiB (`413 file_too_large`), the same as a file.
- The service validates the format version, parses with limits (message count, depth, text length), keeps the transcript for this
  thread and answers `200`:

  ```json
  { "id": "…", "title": "Redirect loop on logout", "message_count": 214 }
  ```

  `id` goes in the run's `attachment_ids`; on send the service starts the message with the transcript as context, marked as another
  thread's history, and the transcript leaves the composer. Deleting the thread deletes the transcript with it.
- Errors: `transcript_too_new` (`422`, made by a newer Orbyn), `transcript_unreadable` (`422`, damaged or not a transcript; the web then
  offers "Add as a file", which uploads the bytes as an ordinary attachment), `file_too_large` (`413`).
- A thread **link** dropped from the left bar needs no route: it already becomes a thread chip.
- The export that makes the file (a thread's "Export transcript") is a separate route, not specified here: it is Eng 3's, and
  its file must stay readable by this import. Opening a file from outside the app is out of scope.

## Earlier messages of an imported thread (K36)

`GET /api/threads/{id}/earlier?before=<cursor>` (the cursor is optional).

- Answers `200` with the last page, or the page before the cursor:

  ```json
  { "source": "Claude Code", "total": 312, "imported_at": "2026-11-21T10:00:00Z", "next_before": "c1",
    "messages": [{ "role": "user", "text": "…" }, { "role": "assistant", "text": "…" }, { "role": "tool", "text": "Read 3 files" }] }
  ```

  `messages` are oldest first, at most 50 a page. `next_before` is `null` at the start of the history. `text` is plain text, at most
  16 KiB per message (cut with an ellipsis), never markup; a tool call is one short line the service writes.
- `404 not_imported` for a thread that was not imported: the web draws nothing.
- `422 history_unreadable` when the provider's history has moved, changed or is too new for Orbyn: the web shows one line at the top
  with "Try again", and the thread works as it does today (the agent resumes from its own session).
- Read only: nothing is changed in the provider's own history and nothing is copied into Orbyn's thread log. The service reads only the
  session file the import recorded, never a path from the request, with the scan limits the import already has (file size, line
  count). The earlier messages are not editable, rewindable or searchable in this version.
- The "Imported" pill in the thread header and its date line need `imported_at` and `source` on the thread itself; the web shows them
  from this answer only.

## What the web does without these routes

Nothing changes: a HEIC photo or an oversized image is uploaded as today (and refused above 10 MiB by the service), a transcript file
dropped on the composer is an ordinary attachment, and an imported thread opens empty.
